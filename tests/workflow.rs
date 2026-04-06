use std::time::Duration;

use nazh_engine::{
    deploy_workflow, shared_connection_manager, ConnectionDefinition, ConnectionManager,
    EngineError, NodeTrait, RhaiNode, RhaiNodeConfig, WorkflowContext, WorkflowGraph,
};
use serde_json::json;
use tokio::time::timeout;

#[tokio::test]
async fn rhai_node_can_transform_json_payload() {
    let node = match RhaiNode::new(
        "rhai-transform",
        RhaiNodeConfig {
            script: "payload[\"value\"] = payload[\"value\"] + 1; payload".to_owned(),
            max_operations: 10_000,
        },
        "increment payload",
    ) {
        Ok(node) => node,
        Err(error) => panic!("rhai node should compile: {error}"),
    };

    let result = node
        .execute(WorkflowContext::new(json!({ "value": 9 })))
        .await;

    match result {
        Ok(ctx) => assert_eq!(ctx.payload, json!({ "value": 10 })),
        Err(error) => panic!("rhai node should execute successfully: {error}"),
    }
}

#[tokio::test]
async fn workflow_graph_executes_end_to_end() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "connections": [
                {
                    "id": "mqtt-main",
                    "type": "mqtt",
                    "metadata": {
                        "broker": "127.0.0.1:1883"
                    }
                }
            ],
            "nodes": {
                "native_input": {
                    "type": "native",
                    "connection_id": "mqtt-main",
                    "config": {
                        "message": "ingest",
                        "inject": {
                            "line": "A01"
                        }
                    }
                },
                "script": {
                    "type": "rhai",
                    "config": {
                        "script": "payload[\"value\"] = payload[\"value\"] * 2; payload"
                    }
                }
            },
            "edges": [
                {
                    "from": "native_input",
                    "to": "script"
                }
            ]
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let connection_manager = shared_connection_manager();
    let mut deployment = match deploy_workflow(graph, connection_manager.clone()).await {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment
        .submit(WorkflowContext::new(json!({ "value": 21 })))
        .await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(
                ctx.payload,
                json!({
                    "_connection": {
                        "borrowed_at": ctx.payload["_connection"]["borrowed_at"],
                        "id": "mqtt-main",
                        "kind": "mqtt",
                        "metadata": {
                            "broker": "127.0.0.1:1883"
                        }
                    },
                    "_native_message": "ingest",
                    "line": "A01",
                    "value": 42
                })
            );
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(_) => panic!("workflow did not produce a result in time"),
    }

    let connections = connection_manager.read().await.list();
    assert_eq!(connections.len(), 1, "expected one registered connection");
    assert!(!connections[0].in_use, "connection should have been released");
}

#[tokio::test]
async fn invalid_graph_rejects_cycles() {
    let result = WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "a": { "type": "native", "config": {} },
                "b": { "type": "native", "config": {} }
            },
            "edges": [
                { "from": "a", "to": "b" },
                { "from": "b", "to": "a" }
            ]
        })
        .to_string(),
    );

    match result {
        Ok(_) => panic!("cyclic graph should not be accepted"),
        Err(EngineError::InvalidGraph(message)) => {
            assert!(message.contains("DAG"), "cycle error should mention DAG");
        }
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn connection_manager_borrows_and_releases_connections() {
    let mut manager = ConnectionManager::default();
    let register_result = manager.register_connection(ConnectionDefinition {
        id: "plc-1".to_owned(),
        kind: "modbus".to_owned(),
        metadata: json!({ "unit_id": 1 }),
    });
    assert!(register_result.is_ok(), "connection should register");

    let lease = manager.borrow("plc-1");
    assert!(lease.is_ok(), "connection should be borrowable");

    let second_borrow = manager.borrow("plc-1");
    match second_borrow {
        Ok(_) => panic!("second borrow should fail"),
        Err(EngineError::ConnectionBusy(connection_id)) => {
            assert_eq!(connection_id, "plc-1");
        }
        Err(error) => panic!("unexpected error: {error}"),
    }

    let release_result = manager.release("plc-1");
    assert!(release_result.is_ok(), "connection should release");
}
