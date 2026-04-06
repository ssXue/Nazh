use std::time::Duration;

use nazh_engine::{build_linear_pipeline, EngineError, PipelineEvent, PipelineStage, WorkflowContext};
use serde_json::json;
use tokio::time::{sleep, timeout, Instant};

#[tokio::test]
async fn linear_pipeline_transforms_payload() {
    let mut pipeline = match build_linear_pipeline(
        vec![
            PipelineStage::new("increment", |ctx| async move {
                let trace_id = ctx.trace_id;
                let Some(value) = ctx.payload.get("value").and_then(|value| value.as_i64()) else {
                    return Err(EngineError::stage_execution(
                        "increment",
                        trace_id,
                        "missing integer field `value`",
                    ));
                };

                Ok(ctx.with_payload(json!({ "value": value + 1 })))
            }),
            PipelineStage::new("tag", |ctx| async move {
                let mut ctx = ctx;
                let trace_id = ctx.trace_id;
                let Some(payload) = ctx.payload.as_object_mut() else {
                    return Err(EngineError::stage_execution(
                        "tag",
                        trace_id,
                        "payload is not an object",
                    ));
                };

                payload.insert("status".to_owned(), json!("ok"));
                Ok(ctx.touch())
            }),
        ],
        8,
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => panic!("pipeline should build successfully: {error}"),
    };

    let submit_result = pipeline
        .submit(WorkflowContext::new(json!({ "value": 41 })))
        .await;
    assert!(submit_result.is_ok(), "context should enter pipeline");

    let result = timeout(Duration::from_secs(1), pipeline.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(ctx.payload, json!({ "value": 42, "status": "ok" }));
        }
        Ok(None) => panic!("result channel closed unexpectedly"),
        Err(_) => panic!("pipeline did not produce a result in time"),
    }
}

#[tokio::test]
async fn stage_errors_do_not_block_following_messages() {
    let mut pipeline = match build_linear_pipeline(
        vec![PipelineStage::new("validate", |ctx| async move {
            let trace_id = ctx.trace_id;
            let Some(value) = ctx.payload.get("value").and_then(|value| value.as_i64()) else {
                return Err(EngineError::stage_execution(
                    "validate",
                    trace_id,
                    "missing integer field `value`",
                ));
            };

            Ok(ctx.with_payload(json!({ "value": value + 1 })))
        })],
        8,
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => panic!("pipeline should build successfully: {error}"),
    };

    let failed_ctx = WorkflowContext::new(json!({ "broken": true }));
    let failed_trace_id = failed_ctx.trace_id;
    let ok_ctx = WorkflowContext::new(json!({ "value": 9 }));
    let ok_trace_id = ok_ctx.trace_id;

    let failed_submit = pipeline.submit(failed_ctx).await;
    assert!(failed_submit.is_ok(), "invalid message should still be accepted");

    let ok_submit = pipeline.submit(ok_ctx).await;
    assert!(ok_submit.is_ok(), "valid message should still be accepted");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_failure = false;
    let mut saw_result = false;

    while Instant::now() < deadline && (!saw_failure || !saw_result) {
        if !saw_failure {
            let event = timeout(Duration::from_millis(100), pipeline.next_event()).await;
            if let Ok(Some(PipelineEvent::StageFailed { trace_id, .. })) = event {
                if trace_id == failed_trace_id {
                    saw_failure = true;
                }
            }
        }

        if !saw_result {
            let result = timeout(Duration::from_millis(100), pipeline.next_result()).await;
            if let Ok(Some(ctx)) = result {
                if ctx.trace_id == ok_trace_id {
                    assert_eq!(ctx.payload, json!({ "value": 10 }));
                    saw_result = true;
                }
            }
        }
    }

    assert!(saw_failure, "expected a failure event for invalid input");
    assert!(saw_result, "expected the valid input to complete successfully");
}

#[tokio::test]
async fn panicking_stage_is_isolated() {
    let mut pipeline = match build_linear_pipeline(
        vec![PipelineStage::new("fragile", |ctx| async move {
            if ctx.payload.get("panic").and_then(|value| value.as_bool()) == Some(true) {
                panic!("synthetic panic for resilience test");
            }

            Ok(ctx)
        })],
        8,
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => panic!("pipeline should build successfully: {error}"),
    };

    let panic_ctx = WorkflowContext::new(json!({ "panic": true }));
    let panic_trace_id = panic_ctx.trace_id;
    let ok_ctx = WorkflowContext::new(json!({ "panic": false, "value": 7 }));
    let ok_trace_id = ok_ctx.trace_id;

    let panic_submit = pipeline.submit(panic_ctx).await;
    assert!(panic_submit.is_ok(), "panic case should still enter the pipeline");

    let ok_submit = pipeline.submit(ok_ctx).await;
    assert!(ok_submit.is_ok(), "second message should still enter the pipeline");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_failure = false;
    let mut saw_result = false;

    while Instant::now() < deadline && (!saw_failure || !saw_result) {
        if !saw_failure {
            let event = timeout(Duration::from_millis(100), pipeline.next_event()).await;
            if let Ok(Some(PipelineEvent::StageFailed { trace_id, error, .. })) = event {
                if trace_id == panic_trace_id {
                    assert!(error.contains("panicked"), "failure event should report panic");
                    saw_failure = true;
                }
            }
        }

        if !saw_result {
            let result = timeout(Duration::from_millis(100), pipeline.next_result()).await;
            if let Ok(Some(ctx)) = result {
                if ctx.trace_id == ok_trace_id {
                    saw_result = true;
                }
            }
        }
    }

    assert!(saw_failure, "expected a failure event for panicking input");
    assert!(saw_result, "expected the non-panicking input to complete");
}

#[tokio::test]
async fn timeout_reports_failure_without_killing_pipeline() {
    let mut pipeline = match build_linear_pipeline(
        vec![PipelineStage::new("slow", |ctx| async move {
            sleep(Duration::from_millis(100)).await;
            Ok(ctx)
        })
        .with_timeout(Duration::from_millis(10))],
        8,
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => panic!("pipeline should build successfully: {error}"),
    };

    let ctx = WorkflowContext::new(json!({ "value": 1 }));
    let trace_id = ctx.trace_id;
    let submit_result = pipeline.submit(ctx).await;
    assert!(submit_result.is_ok(), "message should be accepted");

    let event = timeout(Duration::from_secs(1), pipeline.next_event()).await;
    match event {
        Ok(Some(PipelineEvent::StageStarted { trace_id: started_trace_id, .. })) => {
            assert_eq!(started_trace_id, trace_id);
        }
        Ok(Some(other)) => panic!("unexpected first event: {other:?}"),
        Ok(None) => panic!("event channel closed unexpectedly"),
        Err(_) => panic!("timed out waiting for first event"),
    }

    let event = timeout(Duration::from_secs(1), pipeline.next_event()).await;
    match event {
        Ok(Some(PipelineEvent::StageFailed { trace_id: failed_trace_id, error, .. })) => {
            assert_eq!(failed_trace_id, trace_id);
            assert!(error.contains("timed out"), "failure event should mention timeout");
        }
        Ok(Some(other)) => panic!("unexpected second event: {other:?}"),
        Ok(None) => panic!("event channel closed unexpectedly"),
        Err(_) => panic!("timed out waiting for timeout event"),
    }
}
