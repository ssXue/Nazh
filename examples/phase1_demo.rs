use std::time::Duration;

use nazh_engine::{
    build_linear_pipeline, EngineError, PipelineEvent, PipelineStage, WorkflowContext,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), EngineError> {
    let stages = vec![
        PipelineStage::new("normalize", |ctx| async move {
            let trace_id = ctx.trace_id;
            let Some(value) = ctx
                .payload
                .get("temperature")
                .and_then(|value| value.as_f64())
            else {
                return Err(EngineError::stage_execution(
                    "normalize",
                    trace_id,
                    "missing numeric field `temperature`",
                ));
            };

            let fahrenheit = (value * 1.8) + 32.0;
            Ok(ctx.with_payload(json!({
                "temperature_c": value,
                "temperature_f": fahrenheit,
            })))
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

            payload.insert("source".to_owned(), json!("phase1-demo"));
            Ok(ctx.touch())
        })
        .with_timeout(Duration::from_secs(1)),
    ];

    let mut pipeline = build_linear_pipeline(stages, 16)?;
    pipeline
        .submit(WorkflowContext::new(json!({ "temperature": 24.5 })))
        .await?;

    if let Some(result) = pipeline.next_result().await {
        println!("result: {}", result.payload);
    }

    while let Some(event) = pipeline.next_event().await {
        match event {
            PipelineEvent::PipelineCompleted { .. } => {
                println!("pipeline finished");
                break;
            }
            other => println!("event: {other:?}"),
        }
    }

    Ok(())
}
