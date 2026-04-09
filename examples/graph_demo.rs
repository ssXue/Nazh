use nazh_engine::{deploy_workflow, shared_connection_manager, WorkflowContext, WorkflowGraph};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = WorkflowGraph::from_json(
        &json!({
            "connections": [
                {
                    "id": "plc-main",
                    "type": "modbus",
                    "metadata": {
                        "host": "127.0.0.1",
                        "port": 502
                    }
                }
            ],
            "nodes": {
                "log_input": {
                    "type": "native",
                    "connection_id": "plc-main",
                    "config": {
                        "message": "接收到 PLC 数据",
                        "inject": {
                            "gateway": "edge-a"
                        }
                    }
                },
                "transform": {
                    "type": "rhai",
                    "config": {
                        "script": "payload[\"temperature_f\"] = (payload[\"value\"] * 1.8) + 32.0; payload"
                    }
                }
            },
            "edges": [
                {
                    "from": "log_input",
                    "to": "transform"
                }
            ]
        })
        .to_string(),
    )?;

    let connection_manager = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, connection_manager).await?;

    deployment
        .submit(WorkflowContext::new(json!({ "value": 24.5 })))
        .await?;

    if let Some(result) = deployment.next_result().await {
        println!("workflow result: {}", result.payload);
    }

    while let Some(event) = deployment.next_event().await {
        println!("workflow event: {event:?}");
        if matches!(event, nazh_engine::ExecutionEvent::Output { .. }) {
            break;
        }
    }

    Ok(())
}
