//! SOEM 后端 —— 基于 SOEM C 库的真实 EtherCAT 主站。
//!
//! 使用 `soem-sys` FFI 绑定，通过后台 OS 线程驱动周期性 TX/RX。
//! 所有 unsafe 操作封装在 `soem-sys` 的安全函数中。

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::borrow_as_ptr,
    clippy::ref_as_ptr
)]

use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use soem_sys::{
    ContextPtr, cyclic_loop, ecx_context, read_slave_name, safe_ecx_close, safe_ecx_config_init,
    safe_ecx_config_map_group, safe_ecx_configdc, safe_ecx_init, safe_ecx_readstate,
    safe_ecx_receive_processdata, safe_ecx_send_processdata, safe_ecx_statecheck,
    safe_ecx_writestate, write_slave_outputs, zero_context,
};
use tokio::sync::Mutex;

use crate::ethercat::{EthercatBus, EthercatConfig, EthercatError, SlaveState};

/// IO map 最大字节数。
const IO_MAP_SIZE: usize = 4096;

/// SOEM 默认响应超时（微秒），与 SOEM 源码 EC_TIMEOUTRET 一致。
const EC_TIMEOUTRET: i32 = 2000;

/// EtherCAT 从站状态常量。
const EC_STATE_SAFE_OP: u16 = soem_sys::ec_state_EC_STATE_SAFE_OP as u16;
const EC_STATE_OPERATIONAL: u16 = soem_sys::ec_state_EC_STATE_OPERATIONAL as u16;
const EC_STATE_PRE_OP: u16 = soem_sys::ec_state_EC_STATE_PRE_OP as u16;

/// AL 状态码到文本（含 EtherCAT 协议定义的语义）。
fn al_status_to_text(code: u16) -> String {
    match code {
        0x0000 => "无错误".to_owned(),
        0x0001 => "未指定错误".to_owned(),
        0x0002 => "内存不足".to_owned(),
        0x0003 => "无效设备配置".to_owned(),
        0x0004 => "无效固件版本".to_owned(),
        0x0006 => "SII/EEPROM 与固件不匹配".to_owned(),
        0x0007 => "固件更新失败".to_owned(),
        0x000E => "许可证错误".to_owned(),
        0x0011 => "无效的状态转换请求".to_owned(),
        0x0012 => "未知请求状态".to_owned(),
        0x0013 => "不支持 Bootstrap".to_owned(),
        0x0014 => "无有效固件".to_owned(),
        0x0015 => "无效邮箱配置 (收)".to_owned(),
        0x0016 => "无效邮箱配置 (发)".to_owned(),
        0x0017 => "无效同步管理器配置".to_owned(),
        0x0018 => "无有效输入".to_owned(),
        0x0019 => "无有效输出".to_owned(),
        0x001A => "同步错误".to_owned(),
        0x001B => "同步管理器看门狗".to_owned(),
        0x001C => "无效同步管理器类型".to_owned(),
        0x001D => "无效输出配置（PDO 输出映射/SM 不匹配）".to_owned(),
        0x001E => "无效输入配置（PDO 输入映射/SM 不匹配）".to_owned(),
        0x001F => "无效看门狗配置".to_owned(),
        0x0020 => "从站需要冷启动".to_owned(),
        0x0021 => "从站需要 INIT".to_owned(),
        0x0022 => "从站需要 PRE-OP".to_owned(),
        0x0023 => "从站需要 SAFE-OP".to_owned(),
        0x0024 => "无效输入映射".to_owned(),
        0x0025 => "无效输出映射".to_owned(),
        0x0026 => "配置不一致".to_owned(),
        0x0030 => "无效 DC SYNC 配置".to_owned(),
        0x0031 => "无效 DC 锁存配置".to_owned(),
        0x0032 => "PLL 错误".to_owned(),
        0x0033 => "DC 同步 IO 错误".to_owned(),
        0x0034 => "DC 同步超时".to_owned(),
        0x0035 => "DC 无效同步周期".to_owned(),
        0x0036 => "DC 无效 SYNC0 周期".to_owned(),
        0x0037 => "DC 无效 SYNC1 周期".to_owned(),
        other => format!("未知(0x{other:04X})"),
    }
}

/// 从站地址映射条目。
struct SlaveEntry {
    address: u16,
    name: String,
    input_len: usize,
    output_len: usize,
}

/// SOEM 真实后端。
pub struct SoemBackend {
    context: Mutex<Box<ecx_context>>,
    _io_map: Box<[u8; IO_MAP_SIZE]>,
    slaves: Vec<SlaveEntry>,
    cycle_duration: Duration,
    shutdown_flag: Arc<AtomicBool>,
    thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl SoemBackend {
    /// 创建 SOEM EtherCAT 主站后端。
    pub fn create(config: &EthercatConfig) -> Result<Self, EthercatError> {
        let cycle_duration = config.cycle_duration()?;

        let ifname = CString::new(config.interface.as_str())
            .map_err(|e| EthercatError::InitFailed(format!("接口名包含空字节: {e}")))?;

        let mut context = Box::new(zero_context());
        let mut io_map = Box::new([0u8; IO_MAP_SIZE]);

        if safe_ecx_init(&mut context, ifname.as_ptr()) <= 0 {
            return Err(EthercatError::InitFailed(format!(
                "SOEM 初始化失败：无法打开接口 `{}`",
                config.interface
            )));
        }

        let slave_count = safe_ecx_config_init(&mut context);
        if slave_count <= 0 {
            safe_ecx_close(&mut context);
            return Err(EthercatError::InitFailed("未发现 EtherCAT 从站".to_owned()));
        }
        tracing::debug!(slave_count, "SOEM 从站发现完成");

        safe_ecx_config_map_group(
            &mut context,
            io_map.as_mut_ptr().cast::<std::ffi::c_void>(),
            0,
        );
        log_pdo_mapping(&context);
        safe_ecx_configdc(&mut context);

        context.slavelist[0].state = EC_STATE_SAFE_OP;
        safe_ecx_writestate(&mut context, 0);
        let state = safe_ecx_statecheck(
            &mut context,
            0,
            EC_STATE_SAFE_OP,
            i32::try_from(config.op_timeout_ms).unwrap_or(i32::MAX) * 1000,
        );
        if state != EC_STATE_SAFE_OP {
            log_soem_errors(&context);
            let diagnostic = format_diagnostic(&mut context);
            safe_ecx_close(&mut context);
            return Err(EthercatError::InitFailed(format!(
                "进入 SAFE-OP 超时（期望 0x{EC_STATE_SAFE_OP:02X}, 实际 0x{state:02X}，\
                 发现 {} 个从站）。\n{diagnostic}\n\
                 请检查从站配置与链路",
                context.slavecount
            )));
        }
        tracing::debug!("SOEM 已进入 SAFE-OP 状态");

        context.slavelist[0].state = EC_STATE_OPERATIONAL;
        safe_ecx_writestate(&mut context, 0);
        let state = safe_ecx_statecheck(
            &mut context,
            0,
            EC_STATE_OPERATIONAL,
            i32::try_from(config.op_timeout_ms).unwrap_or(i32::MAX) * 1000,
        );
        if state != EC_STATE_OPERATIONAL {
            log_soem_errors(&context);
            safe_ecx_readstate(&mut context);
            let count = context.slavecount;
            safe_ecx_close(&mut context);
            return Err(EthercatError::InitFailed(format!(
                "进入 OP 超时（期望 0x{EC_STATE_OPERATIONAL:02X}, 实际 0x{state:02X}，\
                 发现 {count} 个从站）。请检查 PDO 映射、SM 看门狗与链路"
            )));
        }

        safe_ecx_readstate(&mut context);

        let slaves = collect_slave_entries(&context);
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let context_ptr = ContextPtr(&mut *context as *mut ecx_context);
        let thread_handle = cyclic_loop(context_ptr, shutdown_flag.clone(), cycle_duration)
            .map_err(EthercatError::InitFailed)?;

        tracing::info!(
            slave_count = slaves.len(),
            cycle = %format_cycle_duration(cycle_duration),
            "SOEM EtherCAT 主站已进入 OP 状态"
        );

        Ok(Self {
            context: Mutex::new(context),
            _io_map: io_map,
            slaves,
            cycle_duration,
            shutdown_flag,
            thread_handle: Mutex::new(Some(thread_handle)),
        })
    }
}

impl Drop for SoemBackend {
    fn drop(&mut self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.thread_handle.try_lock()
            && let Some(handle) = guard.take()
        {
            let _ = handle.join();
        }
        if let Ok(mut ctx) = self.context.try_lock() {
            ctx.slavelist[0].state = EC_STATE_PRE_OP;
            safe_ecx_writestate(&mut ctx, 0);
            safe_ecx_close(&mut ctx);
        }
        tracing::debug!("SOEM EtherCAT 主站已关闭");
    }
}

#[async_trait]
impl EthercatBus for SoemBackend {
    async fn read_inputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError> {
        let index = resolve_slave_index(&self.slaves, slave_address).ok_or(
            EthercatError::SlaveNotFound {
                address: slave_address,
            },
        )?;

        let mut ctx = self.context.lock().await;
        let expected_wk = safe_ecx_send_processdata(&mut ctx);
        let wk = safe_ecx_receive_processdata(&mut ctx, EC_TIMEOUTRET);
        if wk < 0 || (expected_wk > 0 && wk != expected_wk) {
            return Err(EthercatError::PdoReadFailed(format!(
                "TX/RX 失败: working_counter={wk}, expected={expected_wk}"
            )));
        }

        let slave_count = usize::try_from(ctx.slavecount).unwrap_or(0);
        let slave_index = index + 1;
        if slave_index > slave_count {
            return Err(EthercatError::SlaveNotFound {
                address: slave_address,
            });
        }
        let slave = &ctx.slavelist[slave_index];
        Ok(soem_sys::read_slave_inputs(slave))
    }

    async fn write_outputs(&self, slave_address: u16, data: &[u8]) -> Result<(), EthercatError> {
        let index = resolve_slave_index(&self.slaves, slave_address).ok_or(
            EthercatError::SlaveNotFound {
                address: slave_address,
            },
        )?;

        let mut ctx = self.context.lock().await;
        let slave_count = usize::try_from(ctx.slavecount).unwrap_or(0);
        let slave_index = index + 1;
        if slave_index > slave_count {
            return Err(EthercatError::SlaveNotFound {
                address: slave_address,
            });
        }
        let slave = &ctx.slavelist[slave_index];
        let output_len = slave.Obytes as usize;
        if data.len() != output_len {
            return Err(EthercatError::DataLengthMismatch {
                expected: output_len,
                actual: data.len(),
            });
        }
        if !write_slave_outputs(slave, data) {
            return Err(EthercatError::PdoWriteFailed(
                "写入从站输出缓冲失败".to_owned(),
            ));
        }

        let expected_wk = safe_ecx_send_processdata(&mut ctx);
        let wk = safe_ecx_receive_processdata(&mut ctx, EC_TIMEOUTRET);
        if wk < 0 || (expected_wk > 0 && wk != expected_wk) {
            return Err(EthercatError::PdoWriteFailed(format!(
                "TX/RX 失败: working_counter={wk}, expected={expected_wk}"
            )));
        }

        Ok(())
    }

    fn get_slave_states(&self) -> Vec<SlaveState> {
        let Ok(mut ctx) = self.context.try_lock() else {
            return self
                .slaves
                .iter()
                .map(|entry| SlaveState {
                    address: entry.address,
                    name: entry.name.clone(),
                    al_status: 0,
                    al_status_text: "锁竞争".to_owned(),
                    online: false,
                    input_bytes: entry.input_len,
                    output_bytes: entry.output_len,
                })
                .collect();
        };
        safe_ecx_readstate(&mut ctx);
        self.slaves
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let slave = &ctx.slavelist[index + 1];
                SlaveState {
                    address: entry.address,
                    name: entry.name.clone(),
                    al_status: slave.ALstatuscode,
                    al_status_text: al_status_to_text(slave.ALstatuscode),
                    online: slave.state != 0,
                    input_bytes: entry.input_len,
                    output_bytes: entry.output_len,
                }
            })
            .collect()
    }

    fn shutdown(&self) -> Result<(), EthercatError> {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        tracing::debug!("SOEM 后台 TX/RX 线程已请求停止");
        Ok(())
    }

    async fn safe_shutdown(&self) -> Result<(), EthercatError> {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        let mut ctx = self.context.lock().await;
        ctx.slavelist[0].state = EC_STATE_SAFE_OP;
        safe_ecx_writestate(&mut ctx, 0);
        safe_ecx_statecheck(&mut ctx, 0, EC_STATE_SAFE_OP, 5_000_000);
        tracing::debug!("SOEM 已安全切换到 SAFE-OP 状态");
        Ok(())
    }

    fn channel_info(&self) -> String {
        format!(
            "soem ({} 从站, {} 周期)",
            self.slaves.len(),
            format_cycle_duration(self.cycle_duration),
        )
    }
}

/// 收集从站信息。
fn collect_slave_entries(context: &ecx_context) -> Vec<SlaveEntry> {
    let count = context.slavecount;
    let mut entries = Vec::with_capacity(count as usize);
    for i in 1..=count {
        let slave = &context.slavelist[i as usize];
        entries.push(SlaveEntry {
            address: slave.configadr,
            name: read_slave_name(slave),
            input_len: slave.Ibytes as usize,
            output_len: slave.Obytes as usize,
        });
    }
    entries
}

/// 将用户配置的从站选择器解析为内部索引。
fn resolve_slave_index(slaves: &[SlaveEntry], selector: u16) -> Option<usize> {
    slaves
        .iter()
        .position(|entry| entry.address == selector)
        .or_else(|| {
            usize::from(selector)
                .checked_sub(1)
                .filter(|index| *index < slaves.len())
        })
}

fn format_cycle_duration(duration: Duration) -> String {
    let micros = duration.as_micros();
    if micros >= 1_000 && micros.is_multiple_of(1_000) {
        format!("{}ms", micros / 1_000)
    } else {
        format!("{micros}us")
    }
}

fn log_soem_errors(context: &ecx_context) {
    let slave_count = context.slavecount;
    for i in 1..=slave_count {
        let slave = &context.slavelist[i as usize];
        if slave.ALstatuscode != 0 {
            tracing::error!(
                slave_index = i,
                address = slave.configadr,
                al_status_code = slave.ALstatuscode,
                state = slave.state,
                "从站 AL 状态码诊断"
            );
        }
    }
}

/// 打印 SOEM config_map_group 后的 PDO 映射与 SM 配置诊断。
fn log_pdo_mapping(context: &ecx_context) {
    let count = context.slavecount;
    for i in 1..=count {
        let slave = &context.slavelist[i as usize];
        let obits = slave.Obits;
        let ibits = slave.Ibits;
        let obytes = slave.Obytes;
        let ibytes = slave.Ibytes;
        tracing::debug!(
            slave_index = i,
            name = %read_slave_name(slave),
            obits,
            ibits,
            obytes,
            ibytes,
            "SOEM PDO 映射结果"
        );
        for sm_idx in 0..8 {
            let start_addr = slave.SM[sm_idx].StartAddr;
            let sm_length = slave.SM[sm_idx].SMlength;
            let sm_flags = slave.SM[sm_idx].SMflags;
            if start_addr != 0 {
                tracing::debug!(
                    slave_index = i,
                    sm = sm_idx,
                    start_addr,
                    length = sm_length,
                    flags = sm_flags,
                    "SM 配置"
                );
            }
        }
    }
}

/// 格式化从站诊断信息（含 AL status code 描述、PDO 映射、SM 配置）。
fn format_diagnostic(context: &mut ecx_context) -> String {
    safe_ecx_readstate(context);
    let count = context.slavecount;
    let mut lines = Vec::new();
    for i in 1..=count {
        let slave = &context.slavelist[i as usize];
        let al_desc = al_status_to_text(slave.ALstatuscode);
        let obits = slave.Obits;
        let ibits = slave.Ibits;
        let obytes = slave.Obytes;
        let ibytes = slave.Ibytes;
        lines.push(format!(
            "从站#{i} addr=0x{:04X} state=0x{:02X} ALstatus=0x{:04X}({}) Obits={obits} Ibits={ibits} Obytes={obytes} Ibytes={ibytes}",
            slave.configadr, slave.state, slave.ALstatuscode, al_desc,
        ));
        for sm_idx in 0..8 {
            let start_addr = slave.SM[sm_idx].StartAddr;
            let sm_length = slave.SM[sm_idx].SMlength;
            let sm_flags = slave.SM[sm_idx].SMflags;
            let sm_type = slave.SMtype[sm_idx];
            if start_addr != 0 {
                lines.push(format!(
                    "  SM{sm_idx} start=0x{start_addr:04X} len={sm_length} flags=0x{sm_flags:08X} type={sm_type}"
                ));
            }
        }
    }
    lines.join("\n")
}
