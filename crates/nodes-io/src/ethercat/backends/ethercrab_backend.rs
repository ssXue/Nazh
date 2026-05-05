//! EtherCrab 后端 —— 基于 `ethercrab` 纯 Rust 库的真实 EtherCAT 主站。

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ethercrab::{
    DefaultLock, MainDevice, MainDeviceConfig, PduStorage, SubDeviceGroup, Timeouts,
    std::{ethercat_now, tx_rx_task},
    subdevice_group::Op,
};
use tokio::sync::Mutex;

use crate::ethercat::{EthercatBus, EthercatConfig, EthercatError, SlaveState};

/// 编译期常量。
///
/// `MAX_PDU_DATA` 必须 ≥ `PDI_LEN`，否则单个 PDU 装不下整段 PDI，ethercrab
/// 会拆帧——能跑但开销变大。`PduStorage` 是 `static`，不在栈上分配，与 tokio
/// worker 栈大小无关。
const MAX_SUBDEVICES: usize = 64;
const PDI_LEN: usize = 2048;
const MAX_PDU_DATA: usize = 2048;
const MAX_FRAMES: usize = 16;
type OpGroup = SubDeviceGroup<MAX_SUBDEVICES, PDI_LEN, DefaultLock, Op>;

/// PDU 存储 —— 进程级单例。`PduStorage::try_split()` 只能调用一次（内部
/// `is_split` 是 `AtomicBool`，不可复位），所以 EtherCAT 主站的生命周期与
/// 进程一致：首次成功初始化后绑死在该 interface 上，再次部署若想换网卡必须
/// 重启 nazh-desktop。
static PDU_STORAGE: PduStorage<MAX_FRAMES, MAX_PDU_DATA> = PduStorage::new();

struct PduRuntime {
    maindevice: Arc<MainDevice<'static>>,
    /// 后台 TX/RX 任务句柄。保活到进程结束；用 `is_finished()` 检测异常退出。
    tx_handle: tokio::task::JoinHandle<()>,
    /// 首次初始化时使用的接口名，用于检测后续部署是否在尝试切换网卡。
    interface: String,
    /// `MainDevice` 创建时固定下来的状态切换超时。
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

    // tx_rx_task 是同步函数：返回 Result<Future, io::Error>。
    // - 同步部分（打开 raw socket、读 MAC/MTU）失败必须立即返回，不能继续构造 MainDevice。
    // - 异步部分（返回的 Future）必须被 tokio::spawn 持续 poll，PDU 收发循环才会运行。
    //   过去版本把 Future 在 `if let Err(e) = tx_rx_task(...)` 的 Ok 分支里直接丢弃，
    //   导致 PDU 永远不上线、`init_single_group` 一律 timeout。
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
            wait_loop_delay: std::time::Duration::from_millis(2),
            mailbox_response: std::time::Duration::from_secs(1),
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
    group: Arc<Mutex<OpGroup>>,
    slaves: Vec<SlaveEntry>,
    cycle_time_ms: u64,
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
        let cycle_time_ms = config.cycle_time_ms;
        let op_timeout_ms = config.op_timeout_ms;
        let interface = config.interface.clone();

        Box::pin(async move {
            let maindevice = ensure_maindevice(&interface, op_timeout_ms).await?;

            // 发现从站并初始化（PreOp 阶段不能访问 PDI）
            let group = maindevice
                .init_single_group::<MAX_SUBDEVICES, PDI_LEN>(ethercat_now)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("从站发现失败: {e}")))?;

            // 收集从站基本信息
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

            // 转换到 SAFE-OP 后请求 OP，并马上开始过程数据交换。
            //
            // 许多从站在 SAFE-OP -> OP 期间需要持续收到过程数据，否则 SM 看门狗
            // 或 DC 同步会先超时；不能用 ethercrab 的 `into_op()` 静态等待。
            let group = group
                .into_safe_op(&maindevice)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("进入 SAFE-OP 状态失败: {e}")))?;
            tracing::info!("EtherCAT 主站已进入 SAFE-OP 状态");

            let group = group
                .request_into_op(&maindevice)
                .await
                .map_err(|e| EthercatError::InitFailed(format!("请求进入 OP 状态失败: {e}")))?;
            wait_for_all_op(&group, &maindevice, cycle_time_ms, op_timeout_ms).await?;

            // OP 阶段更新 PDI 大小
            for subdevice in group.iter(&maindevice) {
                let addr = subdevice.configured_address();
                if let Some(entry) = slaves.iter_mut().find(|s| s.address == addr) {
                    let io = subdevice.io_raw();
                    entry.input_len = io.inputs().len();
                    entry.output_len = io.outputs().len();
                }
            }

            let group = Arc::new(Mutex::new(group));
            let process_handle =
                spawn_process_data_loop(Arc::clone(&maindevice), Arc::clone(&group), cycle_time_ms);

            tracing::info!(cycle_time_ms, op_timeout_ms, "EtherCAT 主站已进入 OP 状态");

            Ok(Self {
                maindevice,
                group,
                slaves,
                cycle_time_ms,
                process_handle: Mutex::new(Some(process_handle)),
            })
        })
    }
}

impl Drop for EthercrabBackend {
    fn drop(&mut self) {
        if let Some(handle) = self.process_handle.get_mut().take() {
            handle.abort();
        }
    }
}

#[async_trait]
impl EthercatBus for EthercrabBackend {
    async fn read_inputs(&self, slave_address: u16) -> Result<Vec<u8>, EthercatError> {
        let group = self.group.lock().await;
        let target_index = resolve_slave_index(&self.slaves, slave_address).ok_or(
            EthercatError::SlaveNotFound {
                address: slave_address,
            },
        )?;

        group
            .tx_rx(&self.maindevice)
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
        let group = self.group.lock().await;
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

        // 写完缓冲区还要触发一次 TX/RX，输出帧才会真正上线。
        // 这里复用 `read_inputs` 路径上的同一把 group 锁，避免与并发 read 竞争 maindevice 的 PDU 通道。
        group
            .tx_rx(&self.maindevice)
            .await
            .map_err(|e| EthercatError::PdoWriteFailed(format!("TX/RX 失败: {e}")))?;

        Ok(())
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
        // 进程级 PDU_STATE 与 TX/RX 任务由 `ensure_maindevice` 统一管理，
        // 工作流撤销只是丢弃当前 backend 持有的 `Arc<MainDevice>` 与 `SubDeviceGroup`。
        tracing::info!("EtherCAT 主站会话句柄已释放（进程级 TX/RX 任务随进程保活）");
        Ok(())
    }

    fn channel_info(&self) -> String {
        format!(
            "ethercrab ({} 从站, {}ms 周期)",
            self.slaves.len(),
            self.cycle_time_ms,
        )
    }
}

/// 等待所有从站进入 OP，同时持续交换过程数据。
async fn wait_for_all_op(
    group: &OpGroup,
    maindevice: &MainDevice<'_>,
    cycle_time_ms: u64,
    op_timeout_ms: u64,
) -> Result<(), EthercatError> {
    let timeout = Duration::from_millis(op_timeout_ms);
    let cycle = Duration::from_millis(cycle_time_ms.max(1));
    let started_at = Instant::now();

    loop {
        let response = group
            .tx_rx(maindevice)
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

        tokio::time::sleep(cycle).await;
    }
}

/// 后台维持 EtherCAT 过程数据周期。
///
/// 部署后如果只在节点触发时才 TX/RX，带 SM 看门狗的从站会因为周期中断离开 OP。
/// 这里以连接配置的周期持续刷新；读写节点仍通过同一把锁串行访问 PDI。
fn spawn_process_data_loop(
    maindevice: Arc<MainDevice<'static>>,
    group: Arc<Mutex<OpGroup>>,
    cycle_time_ms: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let cycle = Duration::from_millis(cycle_time_ms.max(1));
        let mut consecutive_errors = 0_u64;
        let mut consecutive_non_op = 0_u64;

        loop {
            let started_at = Instant::now();
            let result = {
                let group = group.lock().await;
                group.tx_rx(&maindevice).await
            };

            match result {
                Ok(response) => {
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

            if let Some(delay) = cycle.checked_sub(started_at.elapsed()) {
                tokio::time::sleep(delay).await;
            } else {
                tokio::task::yield_now().await;
            }
        }
    })
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
