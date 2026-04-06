use std::{future::Future, panic::AssertUnwindSafe, pin::Pin, sync::Arc, time::Duration};

use futures_util::FutureExt;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{EngineError, WorkflowContext};

pub type StageFuture = Pin<Box<dyn Future<Output = Result<WorkflowContext, EngineError>> + Send>>;
type StageHandler = Arc<dyn Fn(WorkflowContext) -> StageFuture + Send + Sync>;

#[derive(Clone)]
pub struct PipelineStage {
    pub name: String,
    pub timeout: Option<Duration>,
    pub buffer: usize,
    handler: StageHandler,
}

impl PipelineStage {
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

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_buffer(mut self, buffer: usize) -> Self {
        self.buffer = buffer.max(1);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineEvent {
    StageStarted { stage: String, trace_id: Uuid },
    StageCompleted { stage: String, trace_id: Uuid },
    StageFailed {
        stage: String,
        trace_id: Uuid,
        error: String,
    },
    PipelineCompleted { trace_id: Uuid },
}

pub struct PipelineHandle {
    input_tx: mpsc::Sender<WorkflowContext>,
    result_rx: mpsc::Receiver<WorkflowContext>,
    event_rx: mpsc::Receiver<PipelineEvent>,
}

impl PipelineHandle {
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        self.input_tx
            .send(ctx)
            .await
            .map_err(|_| EngineError::ChannelClosed {
                stage: "ingress".to_owned(),
            })
    }

    pub async fn next_result(&mut self) -> Option<WorkflowContext> {
        self.result_rx.recv().await
    }

    pub async fn next_event(&mut self) -> Option<PipelineEvent> {
        self.event_rx.recv().await
    }

    pub fn ingress(&self) -> mpsc::Sender<WorkflowContext> {
        self.input_tx.clone()
    }
}

pub fn build_linear_pipeline(
    stages: Vec<PipelineStage>,
    ingress_buffer: usize,
) -> Result<PipelineHandle, EngineError> {
    if stages.is_empty() {
        return Err(EngineError::invalid_pipeline(
            "at least one pipeline stage is required",
        ));
    }

    let ingress_buffer = ingress_buffer.max(1);
    let (input_tx, input_rx) = mpsc::channel(ingress_buffer);
    let (result_tx, result_rx) = mpsc::channel(ingress_buffer);
    let (event_tx, event_rx) = mpsc::channel(ingress_buffer * (stages.len() + 1));
    let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
        EngineError::invalid_pipeline("build_linear_pipeline must run inside a Tokio runtime")
    })?;

    let mut current_rx = Some(input_rx);
    let stage_count = stages.len();

    for (index, stage) in stages.into_iter().enumerate() {
        let Some(stage_input_rx) = current_rx.take() else {
            return Err(EngineError::invalid_pipeline(
                "failed to wire the pipeline input channel",
            ));
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

async fn run_stage(
    stage: PipelineStage,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    output_tx: Option<mpsc::Sender<WorkflowContext>>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<PipelineEvent>,
) {
    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;
        let stage_name = stage.name.clone();

        emit_event(
            &event_tx,
            PipelineEvent::StageStarted {
                stage: stage_name.clone(),
                trace_id,
            },
        )
        .await;

        let execution = AssertUnwindSafe((stage.handler)(ctx)).catch_unwind();

        let result = if let Some(timeout) = stage.timeout {
            match tokio::time::timeout(timeout, execution).await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(_)) => Err(EngineError::StagePanicked {
                    stage: stage_name.clone(),
                    trace_id,
                }),
                Err(_) => Err(EngineError::StageTimeout {
                    stage: stage_name.clone(),
                    trace_id,
                    timeout_ms: timeout.as_millis(),
                }),
            }
        } else {
            match execution.await {
                Ok(outcome) => outcome,
                Err(_) => Err(EngineError::StagePanicked {
                    stage: stage_name.clone(),
                    trace_id,
                }),
            }
        };

        match result {
            Ok(next_ctx) => {
                let forward_result = if let Some(tx) = &output_tx {
                    tx.send(next_ctx).await.map_err(|_| EngineError::ChannelClosed {
                        stage: stage_name.clone(),
                    })
                } else {
                    result_tx
                        .send(next_ctx)
                        .await
                        .map_err(|_| EngineError::ChannelClosed {
                            stage: stage_name.clone(),
                        })
                };

                match forward_result {
                    Ok(()) => {
                        emit_event(
                            &event_tx,
                            PipelineEvent::StageCompleted {
                                stage: stage_name.clone(),
                                trace_id,
                            },
                        )
                        .await;

                        if output_tx.is_none() {
                            emit_event(&event_tx, PipelineEvent::PipelineCompleted { trace_id })
                                .await;
                        }
                    }
                    Err(error) => {
                        emit_failure(&event_tx, &stage_name, trace_id, &error).await;
                        break;
                    }
                }
            }
            Err(error) => {
                emit_failure(&event_tx, &stage_name, trace_id, &error).await;
            }
        }
    }
}

async fn emit_failure(
    event_tx: &mpsc::Sender<PipelineEvent>,
    stage: &str,
    trace_id: Uuid,
    error: &EngineError,
) {
    emit_event(
        event_tx,
        PipelineEvent::StageFailed {
            stage: stage.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}

async fn emit_event(event_tx: &mpsc::Sender<PipelineEvent>, event: PipelineEvent) {
    let _ = event_tx.send(event).await;
}
