//! ADR-0016：边传输统计窗口与背压检测。
//!
//! [`EdgeWindow`] 在 100ms 窗口内累计边传输统计并检测背压；
//! 窗口满时发出 [`EdgeTransmitSummary`] 事件并重置计数。

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use nazh_core::{
    BackpressureDetected, BackpressurePolicy, EdgeTransmitSummary, ExecutionEvent, PinKind,
    event::emit_event,
};

/// 单条边的传输累计窗口。
///
/// 每次 `record()` 累加一次传输统计并检测背压；窗口满（≥100ms）时
/// `flush_if_ready()` 发出 [`EdgeTransmitSummary`] 事件并重置计数。
/// 循环退出时 `force_flush()` 无条件刷新剩余数据。
pub(crate) struct EdgeWindow {
    from_pin: String,
    to_node: String,
    to_pin: String,
    edge_kind: PinKind,
    channel_capacity: usize,
    transmit_count: usize,
    max_queue_depth: usize,
    /// 窗口内 `queue_depth` 之和（用于计算平均值）。
    queue_depth_sum: usize,
    /// 窗口内所有传输 payload 的序列化字节数之和。
    total_payload_bytes: u64,
    window_start: Instant,
    /// 本窗口周期内是否已发射过背压告警（限频）。
    backpressure_reported: bool,
    /// ADR-0016：该边的背压处理策略。
    policy: BackpressurePolicy,
    /// ADR-0016：本窗口周期内因策略丢弃的消息数。
    dropped_in_window: u64,
}

impl EdgeWindow {
    pub(crate) fn new(
        from_pin: String,
        to_node: String,
        to_pin: String,
        edge_kind: PinKind,
        channel_capacity: usize,
        policy: BackpressurePolicy,
    ) -> Self {
        Self {
            from_pin,
            to_node,
            to_pin,
            edge_kind,
            channel_capacity,
            policy,
            transmit_count: 0,
            max_queue_depth: 0,
            queue_depth_sum: 0,
            total_payload_bytes: 0,
            window_start: Instant::now(),
            backpressure_reported: false,
            dropped_in_window: 0,
        }
    }

    /// 记录一次边传输并检测背压。
    ///
    /// `payload_bytes` 为本次传输 payload 的序列化字节数，累计到窗口统计中。
    /// 当队列深度达到容量 80% 时发射 [`BackpressureDetected`]，
    /// 每窗口周期最多发射一次以避免重复告警。
    pub(crate) fn record(
        &mut self,
        queue_depth: usize,
        payload_bytes: u64,
        from_node: &str,
        event_tx: &mpsc::Sender<ExecutionEvent>,
    ) {
        self.transmit_count += 1;
        self.max_queue_depth = self.max_queue_depth.max(queue_depth);
        self.queue_depth_sum += queue_depth;
        self.total_payload_bytes += payload_bytes;

        if !self.backpressure_reported
            && self.channel_capacity > 0
            && queue_depth * 10 >= self.channel_capacity * 8
        {
            emit_event(
                event_tx,
                ExecutionEvent::BackpressureDetected(BackpressureDetected {
                    at_node: self.to_node.clone(),
                    incoming_pin: self.to_pin.clone(),
                    channel_capacity: self.channel_capacity,
                    channel_depth: queue_depth,
                    policy: self.policy,
                    dropped_since_last_report: self.dropped_in_window,
                    detected_at: format_instant(Instant::now()),
                }),
            );
            self.backpressure_reported = true;
            tracing::warn!(
                from_node,
                from_pin = %self.from_pin,
                to_node = %self.to_node,
                to_pin = %self.to_pin,
                queue_depth,
                capacity = self.channel_capacity,
                policy = ?self.policy,
                "ADR-0016：检测到背压，队列深度达到容量 80% 以上",
            );
        }
    }

    /// 记录一次因策略而丢弃的消息（不写入 payload 字节数，因为消息未到达下游）。
    pub(crate) fn record_drop(&mut self, payload_bytes: u64) {
        self.dropped_in_window += 1;
        // 丢弃的消息仍计入窗口统计（transmit_count 不含 drop，单独追踪）。
        let _ = payload_bytes;
    }

    /// 返回本窗口周期内因策略丢弃的消息数（仅测试用）。
    #[cfg(test)]
    pub(crate) fn dropped_count(&self) -> u64 {
        self.dropped_in_window
    }

    /// 若窗口已满（≥100ms）且有数据，构造并发出 [`EdgeTransmitSummary`]，
    /// 然后重置计数。窗口未满时跳过。
    pub(crate) fn flush_if_ready(
        &mut self,
        from_node: &str,
        event_tx: &mpsc::Sender<ExecutionEvent>,
    ) {
        if self.transmit_count == 0 || self.window_start.elapsed() < EDGE_WINDOW_DURATION {
            return;
        }
        self.do_flush(from_node, event_tx);
    }

    /// 无条件刷新剩余数据（用于循环退出时保底）。
    pub(crate) fn force_flush(&mut self, from_node: &str, event_tx: &mpsc::Sender<ExecutionEvent>) {
        if self.transmit_count == 0 {
            return;
        }
        self.do_flush(from_node, event_tx);
    }

    fn do_flush(&mut self, from_node: &str, event_tx: &mpsc::Sender<ExecutionEvent>) {
        let now = Instant::now();
        #[allow(clippy::cast_precision_loss)]
        let avg_queue_depth = if self.transmit_count > 0 {
            self.queue_depth_sum as f64 / self.transmit_count as f64
        } else {
            0.0
        };
        emit_event(
            event_tx,
            ExecutionEvent::EdgeTransmitSummary(EdgeTransmitSummary {
                from_node: from_node.to_owned(),
                from_pin: self.from_pin.clone(),
                to_node: self.to_node.clone(),
                to_pin: self.to_pin.clone(),
                edge_kind: self.edge_kind,
                transmit_count: self.transmit_count,
                max_queue_depth: self.max_queue_depth,
                avg_queue_depth,
                total_payload_bytes: self.total_payload_bytes,
                window_started_at: format_instant(self.window_start),
                window_ended_at: format_instant(now),
            }),
        );
        self.transmit_count = 0;
        self.max_queue_depth = 0;
        self.queue_depth_sum = 0;
        self.total_payload_bytes = 0;
        self.backpressure_reported = false;
        self.dropped_in_window = 0;
        self.window_start = now;
    }
}

/// 将 [`Instant`] 格式化为 RFC3339 字符串。
///
/// [`Instant`] 是单调时钟，无绝对时间语义；此处以"进程启动后偏移"近似。
/// 未来若需精确绝对时间，可传入外部 `now: DateTime<Utc>`。
pub(crate) fn format_instant(instant: Instant) -> String {
    let offset = instant.elapsed();
    // 近似：以当前系统时间减去偏移量作为该 instant 的绝对时间。
    let now = chrono::Utc::now();
    let absolute = now - chrono::Duration::from_std(offset).unwrap_or_default();
    absolute.to_rfc3339()
}

/// 边窗口 key：`(from_pin, to_node, to_pin)`。
pub(crate) type EdgeKey = (String, String, String);

/// 窗口刷新间隔。
pub(crate) const EDGE_WINDOW_DURATION: Duration = Duration::from_millis(100);

// 边窗口 key 类型复用 HashMap，在 runner 中使用。
