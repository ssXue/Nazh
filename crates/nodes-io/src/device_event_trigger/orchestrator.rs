//! Orchestrator task：按协议分组 spawn 后台监听循环，等待取消信号后汇拢。

use connections::SharedConnectionManager;

use super::config::CompiledSignal;

#[cfg(feature = "io-can")]
use super::can_loop;
#[cfg(feature = "io-modbus")]
use super::modbus_loop;
#[cfg(feature = "io-mqtt")]
use super::mqtt_loop;
#[cfg(feature = "io-serial")]
use super::serial_loop;

/// 启动 orchestrator task 管理所有协议 listeners。
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn spawn_orchestrator(
    id: &str,
    connection_id: &str,
    host: &str,
    port: u16,
    device_id: &str,
    mqtt_signals: Vec<CompiledSignal>,
    can_signals: Vec<CompiledSignal>,
    modbus_signals: Vec<CompiledSignal>,
    serial_signals: Vec<CompiledSignal>,
    connection_manager: &SharedConnectionManager,
    handle: &nazh_core::NodeHandle,
    token: &nazh_core::CancellationToken,
    poll_interval_ms: u64,
) -> tokio::task::JoinHandle<()> {
    let id = id.to_owned();
    let connection_id = connection_id.to_owned();
    let host = host.to_owned();
    let device_id = device_id.to_owned();
    let connection_manager = connection_manager.clone();
    let handle = handle.clone();
    let token = token.clone();

    tokio::spawn(async move {
        let mut tasks = Vec::new();

        #[cfg(feature = "io-mqtt")]
        if !mqtt_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                mqtt_loop::run_mqtt_listener_loop(
                    &task_id,
                    &task_conn_id,
                    &host,
                    port,
                    &mqtt_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                )
                .await;
            }));
        }

        #[cfg(feature = "io-can")]
        if !can_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                can_loop::run_can_listener_loop(
                    &task_id,
                    &task_conn_id,
                    &can_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                )
                .await;
            }));
        }

        #[cfg(feature = "io-modbus")]
        if !modbus_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                modbus_loop::run_modbus_poll_loop(
                    &task_id,
                    &task_conn_id,
                    &modbus_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                    poll_interval_ms,
                )
                .await;
            }));
        }

        #[cfg(feature = "io-serial")]
        if !serial_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                serial_loop::run_serial_listen_loop(
                    &task_id,
                    &task_conn_id,
                    &serial_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                )
                .await;
            }));
        }

        // 等待取消信号。
        token.cancelled().await;
        // 子任务会因 token 取消而自行退出。
        for task in tasks {
            let _ = task.await;
        }
    })
}
