//! EtherCrab 后端 —— 基于 `ethercrab` 纯 Rust 库的真实 EtherCAT 主站。

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ethercrab::{
    DcSync, DefaultLock, MainDevice, MainDeviceConfig, PduStorage, SubDeviceGroup, Timeouts,
    std::{ethercat_now, tx_rx_task},
    subdevice_group::{DcConfiguration, HasDc, Op},
};
use tokio::sync::Mutex;

use crate::ethercat::{EthercatBus, EthercatConfig, EthercatError, SlaveState};

const MAX_SUBDEVICES: usize = 64;
const PDI_LEN: usize = 2048;
const MAX_PDU_DATA: usize = 2048;
const MAX_FRAMES: usize = 16;
type OpGroup = SubDeviceGroup<MAX_SUBDEVICES, PDI_LEN, DefaultLock, Op, HasDc>;

/// PDU 存储 —— 进程级单例。`PduStorage::try_split()` 只能调用一次（内部
/// `is_split` 是 `AtomicBool`，不可复位），所以 EtherCAT 主站的生命周期与
/// 进程一致：首次成功初始化后绑死在该 interface 上，再次部署若想换网卡必须
/// 重启 nazh-desktop。
static PDU_STORAGE: PduStorage<MAX_FRAMES, MAX_PDU_DATA> = PduStorage::new();

struct PduRuntime {
    maindevice: Arc<MainDevice<'static>>,
    tx_handle: tokio::task::JoinHandle<()>,
    interface: String,
    state_transition_timeout_ms: u64,
}

static PDU_STATE: Mutex<Option<PduRuntime>> = Mutex::const_new(None);

/// 首次初始化 `PduStorage` + `MainDevice` + TX/RX 后台任务，后续命中缓存。
///
/// 命中缓存时校验：
/// - TX/RX 任务存活：若 `is_finished()` 说明 socket 异常退出，给出明确错误
/// - interface 一致：进程级单例，不允许中途切换网卡
async fn ensure_maindevice(
    interface: &str,
    state_transition_timeout_ms: u64,
) -> Result<Arc<MainDevice<'static>>, EthercatError> {
    let mut state = PDU_STATE.lock().await;

    if let Some(rt) = state.as_ref() {
        if rt.tx_handle.is_finished() {
            return Err(EthercatError::InitFailed(format!(
                "EtherCAT TX/RX 任务已终止（接口 `{}`）；请重启 nazh-desktop \
                 后重试，或检查网卡是否被拔出/链路中断",
                rt.interface
            )));
        }
        if rt.interface != interface {
            return Err(EthercatError::InitFailed(format!(
                "EtherCAT 主站已绑定到接口 `{}`，无法在同一进程内切换到 `{}`；\
                 请重启 nazh-desktop",
                rt.interface, interface
            )));
        }
        if rt.state_transition_timeout_ms != state_transition_timeout_ms {
            tracing::warn!(
                current_ms = rt.state_transition_timeout_ms,
                requested_ms = state_transition_timeout_ms,
                "EtherCAT 主站已存在，本次状态切换超时配置不会更新"
            );
        }
        return Ok(Arc::clone(&rt.maindevice));
    }

    let (tx, rx, pdu_loop) = PDU_STORAGE
        .try_split()
        .map_err(|()| EthercatError::InitFailed("PDU 存储已被拆分".to_owned()))?;

    let task = tx_rx_task(interface, tx, rx)
        .map_err(|e| EthercatError::InitFailed(format!("打开网卡 `{interface}` 失败: {e}")))?;

    let tx_handle = tokio::spawn(async move {
        match task.await {
            Ok((_tx, _rx)) => {
                tracing::warn!("EtherCAT TX/RX 任务已结束");
            }
            Err(error) => {
                tracing::error!(?error, "EtherCAT TX/RX 任务异常终止");
            }
        }
    });

    let maindevice = Arc::new(MainDevice::new(
        pdu_loop,
        Timeouts {
            state_transition: Duration::from_millis(state_transition_timeout_ms),
            wait_loop_delay: Duration::from_millis(2),
            mailbox_response: Duration::from_secs(1),
            ..Default::default()
        },
        MainDeviceConfig::default(),
    ));

    *state = Some(PduRuntime {
        maindevice: Arc::clone(&maindevice),
        tx_handle,
        interface: interface.to_owned(),
        state_transition_timeout_ms,
    });

    tracing::info!(interface, "EtherCAT TX/RX 任务已启动");

    Ok(maindevice)
}

/// AL 状态码到文本（含 EtherCAT 协议定义的语义）。
///
/// ethercrab 0.7 暂不支持读取从站 AL status code，此翻译表预留给未来版本使用。
#[allow(dead_code)]
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

/// EtherCrab 真实后端。
pub struct EthercrabBackend {
    maindevice: Arc<MainDevice<'static>>,
    group: Arc<Mutex<Option<OpGroup>>>,
    slaves: Vec<SlaveEntry>,
    cycle_duration: Duration,
    process_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl EthercrabBackend {
    /// 创建 EtherCAT 主站后端。
    ///
    /// PDU 存储 + MainDevice 为进程级单例，首次调用初始化，后续调用复用。
    /// 内部使用 `Box::pin` 将 async 状态机移到堆上，避免 tokio worker 栈溢出。
    pub fn create(
        config: &EthercatConfig,
    ) -> impl Future<Output = Result<Self, EthercatError>> + '_ {
        let op_timeout_ms = config.op_timeout_ms;
        let interface = config.interface.clone();

        Box::pin(async move {
            let cycle_duration = config.cycle_duration()?;
            let dc_start_delay = config.dc_start_delay()?;
            let dc_sync0_period = config.dc_sync0_period()?;
            let dc_sync0_shift = config.dc_sync0_shift()?;
            let maindevice = ensure_maindevice(&interface, op_timeout_ms).await?;

            let mut group = maindevice
                .init_single_group::<MAX_SUBDEVICES, PDI_LEN>(ethercat_now)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("从站发现失败: {e}")))?;

            let mut slaves = Vec::new();
            for subdevice in group.iter(&maindevice) {
                slaves.push(SlaveEntry {
                    address: subdevice.configured_address(),
                    name: subdevice.name().to_owned(),
                    input_len: 0,
                    output_len: 0,
                });
            }

            tracing::info!(count = slaves.len(), "EtherCAT 从站发现完成");
            log_slave_io(&slaves);

            for mut subdevice in group.iter_mut(&maindevice) {
                if subdevice.dc_support().any() {
                    subdevice.set_dc_sync(DcSync::Sync0);
                }
            }

            let group = group
                .into_pre_op_pdi(&maindevice)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("进入 PreOpPdi 失败: {e}")))?;

            let group = group
                .configure_dc_sync(
                    &maindevice,
                    DcConfiguration {
                        start_delay: dc_start_delay,
                        sync0_period: dc_sync0_period,
                        sync0_shift: dc_sync0_shift,
                    },
                )
                .await
                .map_err(|e| EthercatError::InitFailed(format!("DC SYNC0 配置失败: {e}")))?;
            tracing::info!(
                sync0_period = %format_cycle_duration(dc_sync0_period),
                sync0_shift = %format_cycle_duration(dc_sync0_shift),
                start_delay = %format_cycle_duration(dc_start_delay),
                "EtherCAT DC SYNC0 配置完成"
            );

            let group = group
                .into_safe_op(&maindevice)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("进入 SAFE-OP 状态失败: {e}")))?;
            tracing::info!("EtherCAT 主站已进入 SAFE-OP 状态");

            let group = group
                .request_into_op(&maindevice)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("请求进入 OP 状态失败: {e}")))?;
            wait_for_all_op(&group, &maindevice, op_timeout_ms).await?;

            for subdevice in group.iter(&maindevice) {
                let addr = subdevice.configured_address();
                if let Some(entry) = slaves.iter_mut().find(|s| s.address == addr) {
                    let io = subdevice.io_raw();
                    entry.input_len = io.inputs().len();
                    entry.output_len = io.outputs().len();
                }
            }
            log_slave_io(&slaves);

            let group = Arc::new(Mutex::new(Some(group)));
            let process_handle = spawn_process_data_loop(
                Arc::clone(&maindevice),
                Arc::clone(&group),
                cycle_duration,
            );

            tracing::info!(
                slave_count = slaves.len(),
                cycle = %format_cycle_duration(cycle_duration),
                "EtherCAT 主站已进入 OP 状态"
            );

            Ok(Self {
                maindevice,
                group,
                slaves,
                cycle_duration,
                process_handle: Mutex::new(Some(process_handle)),
            })
        })
    }
}

impl Drop for EthercrabBackend {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.process_handle.try_lock()
            && let Some(handle) = guard.take()
        {
            handle.abort();
        }
    }
}

#[async_trait]
impl EthercatBus for EthercrabBackend {
    async fn read_inputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError> {
        let guard = self.group.lock().await;
        let group = guard.as_ref().ok_or(EthercatError::Closed)?;
        let target_index = resolve_slave_index(&self.slaves, slave_address).ok_or(
            EthercatError::SlaveNotFound {
                address: slave_address,
            },
        )?;

        group
            .tx_rx_dc(&self.maindevice)
            .await
            .map_err(|e| EthercatError::PdoReadFailed(format!("TX/RX 失败: {e}")))?;

        for (index, subdevice) in group.iter(&self.maindevice).enumerate() {
            if index == target_index {
                let io = subdevice.io_raw();
                return Ok(io.inputs().to_vec());
            }
        }

        Err(EthercatError::SlaveNotFound {
            address: slave_address,
        })
    }

    async fn write_outputs(&self, slave_address: u16, data: &[u8]) -> Result<(), EthercatError> {
        let guard = self.group.lock().await;
        let group = guard.as_ref().ok_or(EthercatError::Closed)?;
        let target_index = resolve_slave_index(&self.slaves, slave_address).ok_or(
            EthercatError::SlaveNotFound {
                address: slave_address,
            },
        )?;

        let mut staged = false;
        for (index, subdevice) in group.iter(&self.maindevice).enumerate() {
            if index == target_index {
                let mut io = subdevice.io_raw_mut();
                let outputs = io.outputs();
                if data.len() != outputs.len() {
                    return Err(EthercatError::DataLengthMismatch {
                        expected: outputs.len(),
                        actual: data.len(),
                    });
                }
                outputs.copy_from_slice(data);
                staged = true;
                break;
            }
        }
        if !staged {
            return Err(EthercatError::SlaveNotFound {
                address: slave_address,
            });
        }

        group
            .tx_rx_dc(&self.maindevice)
            .await
            .map_err(|e| EthercatError::PdoWriteFailed(format!("TX/RX 失败: {e}")))?;

        Ok(())
    }

    async fn read_outputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError> {
        let guard = self.group.lock().await;
        let group = guard.as_ref().ok_or(EthercatError::Closed)?;
        let target_index = resolve_slave_index(&self.slaves, slave_address).ok_or(
            EthercatError::SlaveNotFound {
                address: slave_address,
            },
        )?;

        for (index, subdevice) in group.iter(&self.maindevice).enumerate() {
            if index == target_index {
                let io = subdevice.io_raw();
                return Ok(io.outputs().to_vec());
            }
        }

        Err(EthercatError::SlaveNotFound {
            address: slave_address,
        })
    }

    fn get_slave_states(&self) -> Vec<SlaveState> {
        self.slaves
            .iter()
            .map(|entry| SlaveState {
                address: entry.address,
                name: entry.name.clone(),
                al_status: 0x08,
                al_status_text: "运行".to_owned(),
                online: true,
                input_bytes: entry.input_len,
                output_bytes: entry.output_len,
            })
            .collect()
    }

    fn shutdown(&self) -> Result<(), EthercatError> {
        if let Ok(mut guard) = self.process_handle.try_lock()
            && let Some(handle) = guard.take()
        {
            handle.abort();
        }
        tracing::info!("EtherCAT 主站会话句柄已释放（进程级 TX/RX 任务随进程保活）");
        Ok(())
    }

    async fn safe_shutdown(&self) -> Result<(), EthercatError> {
        if let Ok(mut guard) = self.process_handle.try_lock()
            && let Some(handle) = guard.take()
        {
            handle.abort();
        }

        let group = {
            let mut guard = self.group.lock().await;
            guard.take()
        };

        let Some(group) = group else {
            tracing::info!("EtherCAT 主站会话已释放（group 已为空）");
            return Ok(());
        };

        for subdevice in group.iter(&self.maindevice) {
            let mut io = subdevice.io_raw_mut();
            let outputs = io.outputs();
            outputs.fill(0);
        }
        if let Err(error) = group.tx_rx_dc(&self.maindevice).await {
            tracing::warn!(?error, "EtherCAT 安全关闭期间最终 TX/RX 失败");
        }

        match group.into_safe_op(&self.maindevice).await {
            Ok(_safe_op_group) => {
                tracing::info!("EtherCAT 从站已安全切换到 SAFE-OP 状态");
            }
            Err(error) => {
                tracing::warn!(?error, "EtherCAT 进入 SAFE-OP 失败，从站可能保持在 OP 状态");
            }
        }

        tracing::info!("EtherCAT 主站安全关闭完成");
        Ok(())
    }

    fn channel_info(&self) -> String {
        format!(
            "ethercrab ({} 从站, {} 周期)",
            self.slaves.len(),
            format_cycle_duration(self.cycle_duration),
        )
    }
}

async fn wait_for_all_op(
    group: &OpGroup,
    maindevice: &MainDevice<'_>,
    op_timeout_ms: u64,
) -> Result<(), EthercatError> {
    let timeout = Duration::from_millis(op_timeout_ms);
    let started_at = Instant::now();

    loop {
        let response = group
            .tx_rx_dc(maindevice)
            .await
            .map_err(|e| EthercatError::InitFailed(format!("等待 OP 状态期间 TX/RX 失败: {e}")))?;

        if response.all_op() {
            tracing::info!(
                elapsed_ms = started_at.elapsed().as_millis(),
                working_counter = response.working_counter,
                "EtherCAT 所有从站已进入 OP 状态"
            );
            return Ok(());
        }

        if started_at.elapsed() >= timeout {
            return Err(EthercatError::InitFailed(format!(
                "进入 OP 状态超时（{op_timeout_ms}ms）：group_state={:?}, \
                 subdevice_states={:?}, working_counter={}。请检查 ESI/PDO 映射、\
                 从站电源与链路、输出 PDO 初值、SM 看门狗或 DC 同步配置",
                response.group_state(),
                response.subdevice_states,
                response.working_counter
            )));
        }

        sleep_or_yield(response.extra.next_cycle_wait).await;
    }
}

/// 后台维持 EtherCAT 过程数据周期。
///
/// 部署后如果只在节点触发时才 TX/RX，带 SM 看门狗的从站会因为周期中断离开 OP。
/// 这里以连接配置的周期持续刷新；读写节点仍通过同一把锁串行访问 PDI。
fn spawn_process_data_loop(
    maindevice: Arc<MainDevice<'static>>,
    group: Arc<Mutex<Option<OpGroup>>>,
    cycle_duration: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut consecutive_errors = 0_u64;
        let mut consecutive_non_op = 0_u64;

        loop {
            let result = {
                let guard = group.lock().await;
                let Some(group) = guard.as_ref() else {
                    break;
                };
                group.tx_rx_dc(&maindevice).await
            };

            let mut next_delay = cycle_duration;
            match result {
                Ok(response) => {
                    next_delay = response.extra.next_cycle_wait;
                    consecutive_errors = 0;
                    if response.all_op() {
                        consecutive_non_op = 0;
                    } else {
                        consecutive_non_op = consecutive_non_op.saturating_add(1);
                    }
                    if consecutive_non_op == 1 || consecutive_non_op.is_multiple_of(100) {
                        tracing::warn!(
                            group_state = ?response.group_state(),
                            subdevice_states = ?response.subdevice_states,
                            working_counter = response.working_counter,
                            consecutive_non_op,
                            "EtherCAT 周期刷新检测到从站未全部处于 OP"
                        );
                    }
                }
                Err(error) => {
                    consecutive_errors = consecutive_errors.saturating_add(1);
                    consecutive_non_op = 0;
                    if consecutive_errors == 1 || consecutive_errors.is_multiple_of(100) {
                        tracing::warn!(?error, consecutive_errors, "EtherCAT 周期刷新失败");
                    }
                }
            }

            sleep_or_yield(next_delay).await;
        }
    })
}

async fn sleep_or_yield(delay: Duration) {
    if delay.is_zero() {
        tokio::task::yield_now().await;
    } else {
        tokio::time::sleep(delay).await;
    }
}

fn format_cycle_duration(duration: Duration) -> String {
    let micros = duration.as_micros();
    if micros >= 1_000 && micros.is_multiple_of(1_000) {
        format!("{}ms", micros / 1_000)
    } else {
        format!("{micros}us")
    }
}

/// 将用户配置的从站选择器解析为 ethercrab 迭代序号。
///
/// 优先按 configured address 精确匹配；未命中时把 `1`、`2`、`3`
/// 解释为第 1、2、3 个从站，兼容 ESI 导入和前端表单里的位置编号。
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

fn log_slave_io(slaves: &[SlaveEntry]) {
    for (index, entry) in slaves.iter().enumerate() {
        tracing::debug!(
            slave_index = index + 1,
            address = entry.address,
            name = %entry.name,
            input_bytes = entry.input_len,
            output_bytes = entry.output_len,
            "EtherCAT 从站 PDI 映射"
        );
    }
}
