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
    window_start: Instant,
    /// 本窗口周期内是否已发射过背压告警（限频）。
    backpressure_reported: bool,
}

impl EdgeWindow {
    pub(crate) fn new(
        from_pin: String,
        to_node: String,
        to_pin: String,
        edge_kind: PinKind,
        channel_capacity: usize,
    ) -> Self {
        Self {
            from_pin,
            to_node,
            to_pin,
            edge_kind,
            channel_capacity,
            transmit_count: 0,
            max_queue_depth: 0,
            window_start: Instant::now(),
            backpressure_reported: false,
        }
    }

    /// 记录一次边传输并检测背压。
    ///
    /// 当队列深度达到容量 80% 时发射 [`BackpressureDetected`]，
    /// 每窗口周期最多发射一次以避免重复告警。
    pub(crate) fn record(
        &mut self,
        queue_depth: usize,
        from_node: &str,
        event_tx: &mpsc::Sender<ExecutionEvent>,
    ) {
        self.transmit_count += 1;
        self.max_queue_depth = self.max_queue_depth.max(queue_depth);

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
                    policy: BackpressurePolicy::Block,
                    dropped_since_last_report: 0,
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
                "ADR-0016：检测到背压，队列深度达到容量 80% 以上",
            );
        }
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
    pub(crate) fn force_flush(
        &mut self,
        from_node: &str,
        event_tx: &mpsc::Sender<ExecutionEvent>,
    ) {
        if self.transmit_count == 0 {
            return;
        }
        self.do_flush(from_node, event_tx);
    }

    fn do_flush(&mut self, from_node: &str, event_tx: &mpsc::Sender<ExecutionEvent>) {
        let now = Instant::now();
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
                window_started_at: format_instant(self.window_start),
                window_ended_at: format_instant(now),
            }),
        );
        self.transmit_count = 0;
        self.max_queue_depth = 0;
        self.backpressure_reported = false;
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
