//! 流水线的类型定义、句柄方法与构建函数。

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use tokio::sync::mpsc;

use super::runner::run_stage;
use crate::{EngineError, ExecutionEvent, WorkflowContext};

/// 流水线阶段处理器返回的 boxed future。
pub type StageFuture = Pin<Box<dyn Future<Output = Result<WorkflowContext, EngineError>> + Send>>;
type StageHandler = Arc<dyn Fn(WorkflowContext) -> StageFuture + Send + Sync>;

/// 线性流水线中的单个处理步骤。
#[derive(Clone)]
pub struct PipelineStage {
    pub name: String,
    pub timeout: Option<Duration>,
    pub buffer: usize,
    pub(crate) handler: StageHandler,
}

impl PipelineStage {
    /// 使用给定名称和异步处理函数创建阶段。
    pub fn new<F, Fut>(name: impl Into<String>, executor: F) -> Self
    where
        F: Fn(WorkflowContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<WorkflowContext, EngineError>> + Send + 'static,
    {
        let executor = Arc::new(executor);
        let handler: StageHandler = Arc::new(move |ctx: WorkflowContext| {
            let executor = Arc::clone(&executor);
            Box::pin(async move { (*executor)(ctx).await })
        });
        Self {
            name: name.into(),
            timeout: None,
            buffer: 32,
            handler,
        }
    }

    /// 设置该阶段每次调用的超时时间。
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置该阶段与下一阶段之间的 MPSC 通道缓冲区大小（最小为 1）。
    #[must_use]
    pub fn with_buffer(mut self, buffer: usize) -> Self {
        self.buffer = buffer.max(1);
        self
    }
}

/// 运行中线性流水线的句柄，提供入口、结果和事件通道。
pub struct PipelineHandle {
    input_tx: mpsc::Sender<WorkflowContext>,
    result_rx: mpsc::Receiver<WorkflowContext>,
    event_rx: mpsc::Receiver<ExecutionEvent>,
}

impl PipelineHandle {
    /// 将上下文发送到流水线的第一个阶段。
    ///
    /// # Errors
    ///
    /// 入口通道已关闭时返回 [`EngineError::ChannelClosed`]。
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        self.input_tx
            .send(ctx)
            .await
            .map_err(|_| EngineError::ChannelClosed {
                stage: "ingress".to_owned(),
            })
    }

    /// 从最终阶段接收下一个成功完成的上下文。
    pub async fn next_result(&mut self) -> Option<WorkflowContext> {
        self.result_rx.recv().await
    }

    /// 接收下一个生命周期事件（阶段开始/完成/失败）。
    pub async fn next_event(&mut self) -> Option<ExecutionEvent> {
        self.event_rx.recv().await
    }

    /// 克隆入口发送端供外部使用。
    pub fn ingress(&self) -> mpsc::Sender<WorkflowContext> {
        self.input_tx.clone()
    }
}

/// 构建并启动顺序执行的线性流水线。
///
/// 每个阶段在独立的 Tokio 任务中运行，上下文通过 MPSC 通道从一个阶段流向下一个。
/// 阶段处理器中的 panic 会被捕获并转换为 [`EngineError::StagePanicked`]。
///
/// # Errors
///
/// 阶段列表为空或不在 Tokio 运行时中调用时返回错误。
pub fn build_linear_pipeline(
    stages: Vec<PipelineStage>,
    ingress_buffer: usize,
) -> Result<PipelineHandle, EngineError> {
    if stages.is_empty() {
        return Err(EngineError::invalid_pipeline("流水线至少需要一个阶段"));
    }

    let ingress_buffer = ingress_buffer.max(1);
    let (input_tx, input_rx) = mpsc::channel(ingress_buffer);
    let (result_tx, result_rx) = mpsc::channel(ingress_buffer);
    let (event_tx, event_rx) = mpsc::channel(ingress_buffer * (stages.len() + 1));
    let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
        EngineError::invalid_pipeline("build_linear_pipeline 必须在 Tokio 运行时中调用")
    })?;

    let mut current_rx = Some(input_rx);
    let stage_count = stages.len();

    for (index, stage) in stages.into_iter().enumerate() {
        let Some(stage_input_rx) = current_rx.take() else {
            return Err(EngineError::invalid_pipeline("流水线输入通道连接失败"));
        };
        let is_last = index + 1 == stage_count;
        let stage_buffer = stage.buffer.max(1);
        let (output_tx, next_rx) = if is_last {
            (None, None)
        } else {
            let (tx, rx) = mpsc::channel(stage_buffer);
            (Some(tx), Some(rx))
        };

        runtime.spawn(run_stage(
            stage,
            stage_input_rx,
            output_tx,
            result_tx.clone(),
            event_tx.clone(),
        ));

        current_rx = next_rx;
    }

    drop(result_tx);
    drop(event_tx);

    Ok(PipelineHandle {
        input_tx,
        result_rx,
        event_rx,
    })
}
