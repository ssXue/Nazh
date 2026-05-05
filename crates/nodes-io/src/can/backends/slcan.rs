//! SLCAN（Serial Line CAN）后端。
//!
//! 通过 USB 转串口适配器实现 CAN 通信，支持 Linux / Windows / macOS。
//! 协议为 Lawicel CAN232/CANUSB 定义的 ASCII 文本协议。
//!
//! ## 初始化序列
//!
//! 1. `C\r` — 关闭 CAN 通道（确保干净状态）
//! 2. `S{bitrate_code}\r` — 设置波特率
//! 3. `O\r` — 打开 CAN 通道
//!
//! ## 帧格式
//!
//! - 发送标准帧：`t<ID 3位十六进制><DLC 1位><数据 2*DLC位>\r`
//! - 发送扩展帧：`T<ID 8位十六进制><DLC 1位><数据 2*DLC位>\r`
//! - 接收标准帧：`t<ID 3位十六进制><DLC 1位><数据 2*DLC位>\r`
//! - 接收扩展帧：`T<ID 8位十六进制><DLC 1位><数据 2*DLC位>\r`

use std::{
    io::{BufRead, BufReader, Write},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use tokio::{
    sync::mpsc,
    time::{Instant, timeout},
};

use crate::can::{
    BusState, CanBus, CanBusConfig, CanError, CanFilter, CanFrame, hex, slcan_bitrate_code,
};

/// SLCAN 后端。
pub struct SlCanBackend {
    config: CanBusConfig,
    writer: Arc<Mutex<Box<dyn SerialPort>>>,
    rx_receiver: tokio::sync::Mutex<mpsc::UnboundedReceiver<CanFrame>>,
    filters: Arc<Mutex<Vec<CanFilter>>>,
    state: Arc<Mutex<BusState>>,
    shutdown_tx: mpsc::Sender<()>,
    reader_join: Option<JoinHandle<()>>,
}

impl SlCanBackend {
    pub async fn open(config: &CanBusConfig) -> Result<Self, CanError> {
        let port = serialport::new(&config.channel, config.baud_rate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| CanError::Serial(format!("打开串口 {} 失败: {e}", config.channel)))?;

        // 克隆串口用于后台读取线程
        let port_clone = port
            .try_clone()
            .map_err(|e| CanError::Serial(format!("克隆串口失败: {e}")))?;

        let (tx, rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let state = Arc::new(Mutex::new(BusState::Active));
        let state_clone = Arc::clone(&state);

        // 启动后台接收线程。JoinHandle 由 Drop 回收，确保串口 clone 及时释放。
        let reader_join = std::thread::spawn(move || {
            let mut reader = BufReader::new(port_clone);
            let mut line = String::new();

            loop {
                // 非阻塞检查关闭信号；放在读之前，避免串口空闲超时时无法退出。
                match shutdown_rx.try_recv() {
                    Ok(()) => {
                        tracing::debug!("[slcan] 收到关闭信号，退出接收线程");
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        tracing::debug!("[slcan] 关闭端已释放，退出接收线程");
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }

                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        tracing::debug!("[slcan] 串口 EOF，接收线程退出");
                        break;
                    }
                    Ok(_) => {
                        if let Some(frame) = decode_slcan_frame(&line)
                            && tx.send(frame).is_err()
                        {
                            tracing::debug!("[slcan] 接收端已关闭，退出");
                            break;
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::TimedOut {
                            // 超时是正常现象，继续轮询
                            continue;
                        }
                        tracing::error!("[slcan] 串口读取错误: {e}");
                        if let Ok(mut s) = state_clone.lock() {
                            *s = BusState::Error;
                        }
                        break;
                    }
                }
            }
        });

        let mut backend = Self {
            config: config.clone(),
            writer: Arc::new(Mutex::new(port)),
            rx_receiver: tokio::sync::Mutex::new(rx),
            filters: Arc::new(Mutex::new(Vec::new())),
            state,
            shutdown_tx,
            reader_join: Some(reader_join),
        };

        backend.init_adapter().await?;

        Ok(backend)
    }

    /// SLCAN 适配器初始化序列。
    async fn init_adapter(&mut self) -> Result<(), CanError> {
        // 关闭（如果之前打开）
        self.send_raw(b"C\r")?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 设置波特率
        let code = slcan_bitrate_code(self.config.bitrate).ok_or_else(|| {
            CanError::OpenFailed(format!(
                "不支持的 CAN 波特率: {} bps，SLCAN 仅支持 10k/20k/50k/100k/125k/250k/500k/800k/1M",
                self.config.bitrate
            ))
        })?;
        self.send_raw(format!("S{code}\r").as_bytes())?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 打开 CAN
        self.send_raw(b"O\r")?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        tracing::info!(
            channel = %self.config.channel,
            can_bitrate = self.config.bitrate,
            "[slcan] 适配器初始化完成"
        );

        Ok(())
    }

    /// 发送原始字节到串口。
    fn send_raw(&self, data: &[u8]) -> Result<(), CanError> {
        let mut writer = self.writer.lock().map_err(|_| CanError::LockPoisoned)?;
        writer
            .write_all(data)
            .map_err(|e| CanError::Serial(format!("串口写入失败: {e}")))?;
        writer
            .flush()
            .map_err(|e| CanError::Serial(format!("串口刷新失败: {e}")))?;
        Ok(())
    }
}

impl Drop for SlCanBackend {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.try_send(());
        let _ = self.send_raw(b"C\r");
        if let Some(reader_join) = self.reader_join.take()
            && reader_join.join().is_err()
        {
            tracing::warn!("[slcan] 接收线程退出时发生 panic");
        }
        if let Ok(mut state) = self.state.lock() {
            *state = BusState::Error;
        }
    }
}

#[async_trait]
impl CanBus for SlCanBackend {
    async fn send(&self, frame: &CanFrame) -> Result<(), CanError> {
        let ascii = encode_slcan_frame(frame)?;
        self.send_raw(ascii.as_bytes())?;
        tracing::debug!(
            id = format!("0x{:03X}", frame.id),
            dlc = frame.dlc,
            "[slcan] 帧已发送"
        );
        Ok(())
    }

    async fn recv(&self, timeout_duration: Duration) -> Result<Option<CanFrame>, CanError> {
        let deadline = Instant::now() + timeout_duration;
        let mut rx = self.rx_receiver.lock().await;

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }

            match timeout(deadline.saturating_duration_since(now), rx.recv()).await {
                Ok(Some(frame)) => {
                    let filters = self.filters.lock().map_err(|_| CanError::LockPoisoned)?;
                    if filters.is_empty()
                        || filters
                            .iter()
                            .any(|filter| filter.matches(frame.id, frame.is_extended))
                    {
                        return Ok(Some(frame));
                    }
                }
                Ok(None) => {
                    // 发送端已关闭
                    if let Ok(mut s) = self.state.lock() {
                        *s = BusState::Error;
                    }
                    return Ok(None);
                }
                Err(_) => return Ok(None),
            }
        }
    }

    fn set_filters(&self, filters: &[CanFilter]) -> Result<(), CanError> {
        // SLCAN 硬件过滤器（m/M 命令）在不同适配器上的支持不稳定，
        // 先统一走软件过滤，避免节点级 can_id 被静默忽略。
        let mut current = self.filters.lock().map_err(|_| CanError::LockPoisoned)?;
        *current = filters.to_vec();
        Ok(())
    }

    fn shutdown(&self) -> Result<(), CanError> {
        let _ = self.shutdown_tx.try_send(());
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(b"C\r");
            let _ = writer.flush();
        }
        if let Ok(mut s) = self.state.lock() {
            *s = BusState::Error;
        }
        Ok(())
    }

    fn channel_info(&self) -> String {
        format!(
            "SLCAN {} @ {} baud (CAN {} kbps)",
            self.config.channel,
            self.config.baud_rate,
            self.config.bitrate / 1000
        )
    }

    fn state(&self) -> BusState {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// 将 `CanFrame` 编码为 SLCAN ASCII 格式。
fn encode_slcan_frame(frame: &CanFrame) -> Result<String, CanError> {
    let dlc = frame.dlc.clamp(0, 8) as usize;
    let data_hex = hex::encode(&frame.data[..dlc]).to_ascii_uppercase();

    if frame.is_extended {
        if frame.id > 0x1FFF_FFFF {
            return Err(CanError::EncodeFailed(format!(
                "扩展帧 ID 0x{:08X} 超过 29-bit 上限",
                frame.id
            )));
        }
        Ok(format!("T{:08X}{}{}\r", frame.id, dlc, data_hex))
    } else {
        if frame.id > 0x7FF {
            return Err(CanError::EncodeFailed(format!(
                "标准帧 ID 0x{:03X} 超过 11-bit 上限",
                frame.id
            )));
        }
        Ok(format!("t{:03X}{}{}\r", frame.id, dlc, data_hex))
    }
}

/// 解析 SLCAN ASCII 帧为 `CanFrame`。
fn decode_slcan_frame(line: &str) -> Option<CanFrame> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    match line.as_bytes().first()? {
        b't' => {
            // 标准帧: t123412345678
            if line.len() < 5 {
                return None;
            }
            let id = u32::from_str_radix(&line[1..4], 16).ok()?;
            let dlc = line[4..5].parse::<u8>().ok()?;
            let data_len = dlc as usize * 2;
            if line.len() < 5 + data_len {
                return None;
            }
            let data = hex::decode(&line[5..5 + data_len]).ok()?;
            Some(CanFrame {
                id,
                data,
                dlc,
                is_extended: false,
                is_fd: false,
                is_remote: false,
                timestamp: Some(Utc::now()),
            })
        }
        b'T' => {
            // 扩展帧: T1234567881122334455667788
            if line.len() < 11 {
                return None;
            }
            let id = u32::from_str_radix(&line[1..9], 16).ok()?;
            let dlc = line[9..10].parse::<u8>().ok()?;
            let data_len = dlc as usize * 2;
            if line.len() < 10 + data_len {
                return None;
            }
            let data = hex::decode(&line[10..10 + data_len]).ok()?;
            Some(CanFrame {
                id,
                data,
                dlc,
                is_extended: true,
                is_fd: false,
                is_remote: false,
                timestamp: Some(Utc::now()),
            })
        }
        b'r' => {
            // 标准远程帧: r1230
            if line.len() < 5 {
                return None;
            }
            let id = u32::from_str_radix(&line[1..4], 16).ok()?;
            let dlc = line[4..5].parse::<u8>().ok()?;
            Some(CanFrame {
                id,
                data: vec![],
                dlc,
                is_extended: false,
                is_fd: false,
                is_remote: true,
                timestamp: Some(Utc::now()),
            })
        }
        b'R' => {
            // 扩展远程帧: R123456780
            if line.len() < 10 {
                return None;
            }
            let id = u32::from_str_radix(&line[1..9], 16).ok()?;
            let dlc = line[9..10].parse::<u8>().ok()?;
            Some(CanFrame {
                id,
                data: vec![],
                dlc,
                is_extended: true,
                is_fd: false,
                is_remote: true,
                timestamp: Some(Utc::now()),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn 编码标准帧() {
        let frame = CanFrame::new_standard(0x123, &[0x01, 0x02, 0x03]);
        assert_eq!(encode_slcan_frame(&frame).unwrap(), "t1233010203\r");
    }

    #[test]
    fn 编码扩展帧() {
        let frame = CanFrame::new_extended(0x18FF_1234, &[0xAB, 0xCD]);
        assert_eq!(encode_slcan_frame(&frame).unwrap(), "T18FF12342ABCD\r");
    }

    #[test]
    fn 解码标准帧() {
        let frame = decode_slcan_frame("t1233010203\r").unwrap();
        assert_eq!(frame.id, 0x123);
        assert_eq!(frame.dlc, 3);
        assert_eq!(frame.data, vec![0x01, 0x02, 0x03]);
        assert!(!frame.is_extended);
    }

    #[test]
    fn 解码扩展帧() {
        let frame = decode_slcan_frame("T18FF12342ABCD\r").unwrap();
        assert_eq!(frame.id, 0x18FF_1234);
        assert_eq!(frame.dlc, 2);
        assert_eq!(frame.data, vec![0xAB, 0xCD]);
        assert!(frame.is_extended);
    }

    #[test]
    fn 解码远程标准帧() {
        let frame = decode_slcan_frame("r1234").unwrap();
        assert_eq!(frame.id, 0x123);
        assert_eq!(frame.dlc, 4);
        assert!(frame.is_remote);
        assert!(!frame.is_extended);
    }

    #[test]
    fn 解码空行返回_none() {
        assert!(decode_slcan_frame("").is_none());
        assert!(decode_slcan_frame("  ").is_none());
    }

    #[test]
    fn 编码标准帧_id超限() {
        let _frame = CanFrame::new_extended(0x123, &[0x01]);
        // 扩展帧构造时 id 被掩码为 29-bit，但这里手动构造一个超长的
        let bad_frame = CanFrame {
            id: 0xFFFF_FFFF,
            data: vec![0x01],
            dlc: 1,
            is_extended: false,
            is_fd: false,
            is_remote: false,
            timestamp: None,
        };
        assert!(encode_slcan_frame(&bad_frame).is_err());
    }
}
