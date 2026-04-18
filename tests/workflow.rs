use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use nazh_ai_core::{
    AiCompletionRequest, AiCompletionResponse, AiError, AiMessageRole, AiProviderDraft, AiService,
    AiTestResult,
};
use nazh_engine::{
    ConnectionDefinition, ConnectionManager, DebugConsoleNode, DebugConsoleNodeConfig,
    EngineError, HttpClientNode, HttpClientNodeConfig, ModbusReadNode, ModbusReadNodeConfig,
    NodeDispatch, NodeTrait, RhaiNode, RhaiNodeConfig, SerialTriggerNode, SerialTriggerNodeConfig,
    SqlWriterNode, SqlWriterNodeConfig, TimerNode, TimerNodeConfig, WorkflowContext,
    WorkflowGraph, deploy_workflow, deploy_workflow_with_ai, shared_connection_manager,
    standard_registry,
};
use serde_json::json;
use tokio::time::timeout;
use uuid::Uuid;

struct StubAiService;

#[async_trait]
impl AiService for StubAiService {
    async fn complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<AiCompletionResponse, AiError> {
        let system_prompt = request
            .messages
            .iter()
            .find(|message| matches!(message.role, AiMessageRole::System))
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let user_prompt = request
            .messages
            .iter()
            .find(|message| matches!(message.role, AiMessageRole::User))
            .map(|message| message.content.clone())
            .unwrap_or_default();

        Ok(AiCompletionResponse {
            content: format!(
                "provider={} model={} system={} user={}",
                request.provider_id,
                request.model.unwrap_or_else(|| "default-model".to_owned()),
                system_prompt,
                user_prompt
            ),
            usage: None,
            model: "stub-model".to_owned(),
        })
    }

    async fn test_connection(&self, _draft: AiProviderDraft) -> Result<AiTestResult, AiError> {
        panic!("workflow tests should not call test_connection");
    }
}

#[tokio::test]
async fn rhai_node_can_transform_json_payload() {
    let node = match RhaiNode::new(
        "rhai-transform",
        RhaiNodeConfig {
            script: "payload[\"value\"] = payload[\"value\"] + 1; payload".to_owned(),
            max_operations: 10_000,
            ai: None,
        },
        "increment payload",
        None,
    ) {
        Ok(node) => node,
        Err(error) => panic!("rhai node should compile: {error}"),
    };

    let trace_id = Uuid::new_v4();
    let result = node.transform(trace_id, json!({ "value": 9 })).await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                match &first_output.dispatch {
                    NodeDispatch::Broadcast => {}
                    NodeDispatch::Route(_) => panic!("plain rhai node should broadcast"),
                }
                assert_eq!(first_output.payload, json!({ "value": 10 }));
            }
            None => panic!("rhai node should produce a single output"),
        },
        Err(error) => panic!("rhai node should execute successfully: {error}"),
    }
}

#[tokio::test]
async fn workflow_script_node_can_call_ai_complete() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "script": {
                    "type": "rhai",
                    "config": {
                        "script": "payload[\"reply\"] = ai_complete(\"请回复:\" + payload[\"text\"]); payload",
                        "ai": {
                            "providerId": "stub-provider",
                            "model": "gpt-script",
                            "systemPrompt": "你是脚本测试助手",
                            "temperature": 0.2,
                            "maxTokens": 128,
                            "topP": 0.9,
                            "timeoutMs": 5_000
                        }
                    }
                }
            },
            "edges": []
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let ai_service: Arc<dyn AiService> = Arc::new(StubAiService);
    let mut deployment = match deploy_workflow_with_ai(
        graph,
        shared_connection_manager(),
        Some(ai_service),
        &registry,
    )
    .await
    {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment
        .submit(WorkflowContext::new(json!({ "text": "测试脚本节点" })))
        .await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(
                ctx.payload,
                json!({
                    "text": "测试脚本节点",
                    "reply": "provider=stub-provider model=gpt-script system=你是脚本测试助手 user=请回复:测试脚本节点"
                })
            );
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
    }
}

#[tokio::test]
async fn workflow_rejects_script_ai_config_without_ai_service() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "script": {
                    "type": "rhai",
                    "config": {
                        "script": "payload[\"reply\"] = ai_complete(\"hello\"); payload",
                        "ai": {
                            "providerId": "stub-provider"
                        }
                    }
                }
            },
            "edges": []
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let result = deploy_workflow_with_ai(graph, shared_connection_manager(), None, &registry).await;

    match result {
        Ok(_) => panic!("workflow deployment should fail without ai service"),
        Err(EngineError::InvalidGraph(message)) => {
            assert!(
                message.contains("AiService"),
                "error should mention missing AiService"
            );
        }
        Err(error) => panic!("unexpected error: {error}"),
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
                        "host": "127.0.0.1",
                        "port": 1883,
                        "topic": "test/topic"
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
    let registry = standard_registry();
    let mut deployment = match deploy_workflow(graph, connection_manager.clone(), &registry).await {
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
                    "_native_message": "ingest",
                    "line": "A01",
                    "value": 42
                })
            );
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
    }

    let connections = connection_manager.list().await;
    assert_eq!(connections.len(), 1, "expected one registered connection");
    assert!(
        !connections[0].in_use,
        "connection should have been released"
    );
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

#[tokio::test]
async fn connection_manager_borrows_and_releases_connections() {
    let manager = ConnectionManager::default();
    let register_result = manager
        .register_connection(ConnectionDefinition {
            id: "plc-1".to_owned(),
            kind: "modbus".to_owned(),
            metadata: json!({ "unit_id": 1, "host": "127.0.0.1", "port": 502 }),
        })
        .await;
    assert!(register_result.is_ok(), "connection should register");

    let lease = manager.borrow("plc-1").await;
    assert!(lease.is_ok(), "connection should be borrowable");

    let second_borrow = manager.borrow("plc-1").await;
    match second_borrow {
        Ok(_) => panic!("second borrow should fail"),
        Err(EngineError::ConnectionBusy(connection_id)) => {
            assert_eq!(connection_id, "plc-1");
        }
        Err(error) => panic!("unexpected error: {error}"),
    }

    let release_result = manager.release("plc-1").await;
    assert!(release_result.is_ok(), "connection should release");
}

#[tokio::test]
async fn if_node_routes_only_to_the_matching_branch() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "decision": {
                    "type": "if",
                    "config": {
                        "script": "payload[\"value\"] > 80"
                    }
                },
                "high_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "branch": "high"
                        }
                    }
                },
                "low_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "branch": "low"
                        }
                    }
                }
            },
            "edges": [
                {
                    "from": "decision",
                    "to": "high_path",
                    "source_port_id": "true"
                },
                {
                    "from": "decision",
                    "to": "low_path",
                    "source_port_id": "false"
                }
            ]
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let mut deployment = match deploy_workflow(graph, shared_connection_manager(), &registry).await
    {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment
        .submit(WorkflowContext::new(json!({ "value": 90 })))
        .await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(ctx.payload["branch"], json!("high"));
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
    }
}

#[tokio::test]
async fn switch_node_routes_using_source_ports() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "decision": {
                    "type": "switch",
                    "config": {
                        "script": "payload[\"route\"]",
                        "branches": [
                            { "key": "high", "label": "High" }
                        ]
                    }
                },
                "high_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "route_taken": "high"
                        }
                    }
                },
                "default_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "route_taken": "default"
                        }
                    }
                }
            },
            "edges": [
                {
                    "from": "decision",
                    "to": "high_path",
                    "source_port_id": "high"
                },
                {
                    "from": "decision",
                    "to": "default_path",
                    "source_port_id": "default"
                }
            ]
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let mut deployment = match deploy_workflow(graph, shared_connection_manager(), &registry).await
    {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment
        .submit(WorkflowContext::new(json!({ "route": "high" })))
        .await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(ctx.payload["route_taken"], json!("high"));
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
    }
}

#[tokio::test]
async fn try_catch_node_routes_runtime_errors_to_catch_branch() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "guarded": {
                    "type": "tryCatch",
                    "config": {
                        "script": "missing_handler(payload)"
                    }
                },
                "success_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "handled_by": "try"
                        }
                    }
                },
                "catch_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "handled_by": "catch"
                        }
                    }
                }
            },
            "edges": [
                {
                    "from": "guarded",
                    "to": "success_path",
                    "source_port_id": "try"
                },
                {
                    "from": "guarded",
                    "to": "catch_path",
                    "source_port_id": "catch"
                }
            ]
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let mut deployment = match deploy_workflow(graph, shared_connection_manager(), &registry).await
    {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment.submit(WorkflowContext::new(json!({}))).await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(ctx.payload["handled_by"], json!("catch"));
            assert!(
                ctx.payload["_error"].is_string(),
                "catch branch should include error payload"
            );
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
    }
}

#[tokio::test]
async fn loop_node_routes_body_iterations_and_done() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "loop_items": {
                    "type": "loop",
                    "config": {
                        "script": "payload[\"items\"]"
                    }
                },
                "body_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "branch": "body"
                        }
                    }
                },
                "done_path": {
                    "type": "native",
                    "config": {
                        "inject": {
                            "branch": "done"
                        }
                    }
                }
            },
            "edges": [
                {
                    "from": "loop_items",
                    "to": "body_path",
                    "source_port_id": "body"
                },
                {
                    "from": "loop_items",
                    "to": "done_path",
                    "source_port_id": "done"
                }
            ]
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let mut deployment = match deploy_workflow(graph, shared_connection_manager(), &registry).await
    {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment
        .submit(WorkflowContext::new(json!({
            "items": ["alpha", "beta", "gamma"]
        })))
        .await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let mut body_results = Vec::new();
    let mut done_result = None;

    for _ in 0..4 {
        let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
        match result {
            Ok(Some(ctx)) => {
                let phase = ctx.payload["_loop"]["phase"].as_str();
                match phase {
                    Some("body") => body_results.push(ctx.payload),
                    Some("done") => {
                        done_result = Some(ctx.payload);
                    }
                    Some(other) => panic!("unexpected loop phase `{other}`"),
                    None => panic!("loop output should include phase"),
                }
            }
            Ok(None) => panic!("result stream closed unexpectedly"),
            Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
        }
    }

    assert_eq!(
        body_results.len(),
        3,
        "loop should emit three body iterations"
    );

    for (index, payload) in body_results.iter().enumerate() {
        assert_eq!(payload["branch"], json!("body"));
        assert_eq!(payload["_loop"]["phase"], json!("body"));
        assert_eq!(payload["_loop"]["index"], json!(index as u64));
        assert_eq!(payload["_loop"]["count"], json!(3));
        assert_eq!(
            payload["_loop"]["item"],
            json!(["alpha", "beta", "gamma"][index])
        );
    }

    match done_result {
        Some(payload) => {
            assert_eq!(payload["branch"], json!("done"));
            assert_eq!(payload["_loop"]["phase"], json!("done"));
            assert_eq!(payload["_loop"]["count"], json!(3));
        }
        None => panic!("loop should emit a done output"),
    }
}

#[tokio::test]
async fn timer_node_injects_trigger_metadata() {
    let mut inject = serde_json::Map::new();
    inject.insert("source".to_owned(), json!("timer"));

    let node = TimerNode::new(
        "tick",
        TimerNodeConfig {
            interval_ms: 2_500,
            immediate: true,
            inject,
        },
        "interval trigger",
    );

    let trace_id = Uuid::new_v4();
    let result = node.transform(trace_id, json!({ "seed": "keep" })).await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(first_output.payload["seed"], json!("keep"));
                assert_eq!(first_output.payload["source"], json!("timer"));
                assert_eq!(first_output.metadata["timer"]["node_id"], json!("tick"));
                assert_eq!(first_output.metadata["timer"]["interval_ms"], json!(2_500));
                assert_eq!(first_output.metadata["timer"]["immediate"], json!(true));
            }
            None => panic!("timer node should produce one output"),
        },
        Err(error) => panic!("timer node should execute successfully: {error}"),
    }
}

#[tokio::test]
async fn serial_trigger_node_normalizes_ascii_and_hex_frames() {
    let mut inject = serde_json::Map::new();
    inject.insert("source".to_owned(), json!("serial"));

    let node = SerialTriggerNode::new(
        "scan_input",
        SerialTriggerNodeConfig {
            port_path: "/dev/tty.mock".to_owned(),
            baud_rate: 9_600,
            data_bits: 8,
            parity: "none".to_owned(),
            stop_bits: 1,
            flow_control: "none".to_owned(),
            encoding: "hex".to_owned(),
            delimiter: "\\n".to_owned(),
            read_timeout_ms: 100,
            idle_gap_ms: 80,
            max_frame_bytes: 512,
            trim: true,
            inject,
        },
        "serial trigger",
    );

    let trace_id = Uuid::new_v4();
    let result = node
        .transform(
            trace_id,
            json!({
                "_serial_frame": {
                    "ascii": " RFID-42\r\n",
                    "hex": "52 46 49 44 2D 34 32",
                    "byte_len": 9,
                    "port_path": "/dev/tty.mock"
                }
            }),
        )
        .await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(first_output.payload["source"], json!("serial"));
                assert_eq!(first_output.payload["serial_ascii"], json!("RFID-42"));
                assert_eq!(
                    first_output.payload["serial_hex"],
                    json!("52 46 49 44 2D 34 32")
                );
                assert_eq!(
                    first_output.payload["serial_data"],
                    json!("52 46 49 44 2D 34 32")
                );
                assert_eq!(
                    first_output.metadata["serial"]["node_id"],
                    json!("scan_input")
                );
                assert_eq!(
                    first_output.metadata["serial"]["port_path"],
                    json!("/dev/tty.mock")
                );
                assert_eq!(first_output.metadata["serial"]["encoding"], json!("hex"));
            }
            None => panic!("serial trigger node should produce one output"),
        },
        Err(error) => panic!("serial trigger node should execute successfully: {error}"),
    }
}

#[tokio::test]
async fn code_node_alias_executes_like_rhai() {
    let graph = match WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "transform": {
                    "type": "code",
                    "config": {
                        "script": "payload[\"normalized\"] = true; payload[\"value\"] = payload[\"value\"] + 2; payload"
                    }
                }
            },
            "edges": []
        })
        .to_string(),
    ) {
        Ok(graph) => graph,
        Err(error) => panic!("graph JSON should parse: {error}"),
    };

    let registry = standard_registry();
    let mut deployment = match deploy_workflow(graph, shared_connection_manager(), &registry).await
    {
        Ok(deployment) => deployment,
        Err(error) => panic!("workflow should deploy successfully: {error}"),
    };

    let submit_result = deployment
        .submit(WorkflowContext::new(json!({ "value": 40 })))
        .await;
    assert!(submit_result.is_ok(), "workflow should accept payload");

    let result = timeout(Duration::from_secs(1), deployment.next_result()).await;
    match result {
        Ok(Some(ctx)) => {
            assert_eq!(ctx.payload["normalized"], json!(true));
            assert_eq!(ctx.payload["value"], json!(42));
        }
        Ok(None) => panic!("result stream closed unexpectedly"),
        Err(elapsed) => panic!("workflow did not produce a result in time: {elapsed}"),
    }
}

#[tokio::test]
async fn modbus_read_node_emits_simulated_values() {
    let node = ModbusReadNode::new(
        "modbus-main",
        ModbusReadNodeConfig {
            connection_id: None,
            unit_id: 1,
            register: 40_001,
            quantity: 2,
            base_value: 72.0,
            amplitude: 5.0,
        },
        "read modbus data",
        shared_connection_manager(),
    );

    let trace_id = Uuid::new_v4();
    let result = node.transform(trace_id, json!({})).await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(first_output.metadata["modbus"]["simulated"], json!(true));
                assert_eq!(first_output.metadata["modbus"]["register"], json!(40_001));
                assert_eq!(first_output.metadata["modbus"]["quantity"], json!(2));
                assert!(
                    first_output.payload["values"].as_array().map(Vec::len) == Some(2),
                    "modbus read node should output two simulated values",
                );
            }
            None => panic!("modbus read node should produce one output"),
        },
        Err(error) => panic!("modbus read node should execute successfully: {error}"),
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn http_client_node_posts_payload_and_records_response() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("listener should bind: {error}"),
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => panic!("listener should expose address: {error}"),
    };

    let server = std::thread::spawn(move || {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) => panic!("request should connect: {error}"),
        };
        let mut request_bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut headers_end = None;
        let mut expected_len = None;

        loop {
            let read_count = match stream.read(&mut buffer) {
                Ok(count) => count,
                Err(error) => panic!("request should be readable: {error}"),
            };
            if read_count == 0 {
                break;
            }

            request_bytes.extend_from_slice(&buffer[..read_count]);

            if headers_end.is_none() {
                headers_end = find_bytes(&request_bytes, b"\r\n\r\n");
                if let Some(index) = headers_end {
                    let header_text = String::from_utf8_lossy(&request_bytes[..index]);
                    let content_length = header_text
                        .lines()
                        .find_map(|line| {
                            line.split_once(':').and_then(|(name, value)| {
                                if name.eq_ignore_ascii_case("Content-Length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or(0);
                    expected_len = Some(index + 4 + content_length);
                }
            }

            if let Some(total_len) = expected_len
                && request_bytes.len() >= total_len
            {
                break;
            }
        }

        let request_text = String::from_utf8_lossy(&request_bytes);
        assert!(
            request_text.starts_with("POST /robot "),
            "request should target POST /robot"
        );
        assert!(
            request_text.contains("\"severity\":\"high\""),
            "request body should include the serialized payload"
        );

        let response_body = r#"{"ok":true,"channel":"dingtalk"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        match stream.write_all(response.as_bytes()) {
            Ok(()) => {}
            Err(error) => panic!("response should be writable: {error}"),
        }
    });

    let node = match HttpClientNode::new(
        "dingtalk-alert",
        HttpClientNodeConfig {
            url: format!("http://{address}/robot"),
            method: "POST".to_owned(),
            ..HttpClientNodeConfig::default()
        },
        "send alarm",
    ) {
        Ok(n) => n,
        Err(e) => panic!("HttpClientNode 创建失败: {e}"),
    };

    let trace_id = Uuid::new_v4();
    let result = node
        .transform(trace_id, json!({ "severity": "high", "value": 92 }))
        .await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(first_output.metadata["http"]["status"], json!(200));
                assert_eq!(first_output.payload["http_response"]["ok"], json!(true));
                assert_eq!(
                    first_output.payload["http_response"]["channel"],
                    json!("dingtalk")
                );
            }
            None => panic!("http client node should produce one output"),
        },
        Err(error) => panic!("http client node should execute successfully: {error}"),
    }

    match server.join() {
        Ok(()) => {}
        Err(error) => panic!("http test server should finish cleanly: {error:?}"),
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn http_alarm_node_renders_dingtalk_markdown_body() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("test server should bind: {error}"),
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => panic!("local address should be available: {error}"),
    };

    let server = std::thread::spawn(move || {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) => panic!("http alarm server should accept a connection: {error}"),
        };

        let mut request_bytes = Vec::new();
        let mut expected_len: Option<usize> = None;

        loop {
            let mut buffer = [0_u8; 1024];
            let bytes_read = match stream.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => panic!("request should be readable: {error}"),
            };

            if bytes_read == 0 {
                break;
            }

            request_bytes.extend_from_slice(&buffer[..bytes_read]);

            if expected_len.is_none() {
                let header_text = String::from_utf8_lossy(&request_bytes);
                if let Some(header_end) = header_text.find("\r\n\r\n") {
                    let headers = &header_text[..header_end];
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if name.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    expected_len = Some(header_end + 4 + content_length);
                }
            }

            if let Some(total_len) = expected_len
                && request_bytes.len() >= total_len
            {
                break;
            }
        }

        let request_text = String::from_utf8_lossy(&request_bytes);
        assert!(
            request_text.starts_with("POST /robot "),
            "request should target POST /robot"
        );
        let request_lower = request_text.to_lowercase();
        assert!(
            request_lower.contains("content-type: application/json"),
            "dingtalk alarm should send a JSON content type"
        );
        assert!(
            request_text.contains("\"msgtype\":\"markdown\""),
            "request body should use DingTalk markdown payload"
        );
        assert!(
            request_text.contains("\"title\":\"Nazh 告警 · boiler-a · alert\""),
            "request body should render the title template"
        );
        assert!(
            request_text.contains(
                "\"text\":\"### 告警\\n- 设备：boiler-a\\n- 严重级别：alert\\n- 温度：92\""
            ),
            "request body should render the markdown body template"
        );
        assert!(
            request_text.contains("\"atMobiles\":[\"13800000000\"]"),
            "request body should include the configured at mobile list"
        );

        let response_body = r#"{"errcode":0,"errmsg":"ok"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        if let Err(error) = stream.write_all(response.as_bytes()) {
            panic!("response should be writable: {error}");
        }
    });

    let node = match HttpClientNode::new(
        "http_alarm",
        HttpClientNodeConfig {
            url: format!("http://{address}/robot"),
            method: "POST".to_owned(),
            webhook_kind: "dingtalk".to_owned(),
            body_mode: "dingtalk_markdown".to_owned(),
            request_timeout_ms: 2_500,
            title_template: "Nazh 告警 · {{payload.tag}} · {{payload.severity}}".to_owned(),
            body_template:
                "### 告警\n- 设备：{{payload.tag}}\n- 严重级别：{{payload.severity}}\n- 温度：{{payload.temperature_c}}"
                    .to_owned(),
            at_mobiles: vec!["13800000000".to_owned()],
            ..HttpClientNodeConfig::default()
        },
        "send formatted dingtalk alarm",
    ) {
        Ok(n) => n,
        Err(e) => panic!("HttpClientNode 创建失败: {e}"),
    };

    let trace_id = Uuid::new_v4();
    let result = node
        .transform(
            trace_id,
            json!({
                "tag": "boiler-a",
                "severity": "alert",
                "temperature_c": 92
            }),
        )
        .await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(first_output.metadata["http"]["status"], json!(200));
                assert_eq!(
                    first_output.metadata["http"]["webhook_kind"],
                    json!("dingtalk")
                );
                assert_eq!(
                    first_output.metadata["http"]["body_mode"],
                    json!("dingtalk_markdown")
                );
                assert_eq!(first_output.payload["http_response"]["errcode"], json!(0));
            }
            None => panic!("http alarm node should produce one output"),
        },
        Err(error) => panic!("http alarm node should execute successfully: {error}"),
    }

    match server.join() {
        Ok(()) => {}
        Err(error) => panic!("http alarm test server should finish cleanly: {error:?}"),
    }
}

#[tokio::test]
async fn sql_writer_node_persists_payload_into_sqlite() {
    let database_path =
        std::env::temp_dir().join(format!("nazh-sql-writer-{}.sqlite3", Uuid::new_v4()));
    let database_path_string = database_path.to_string_lossy().to_string();

    let node = SqlWriterNode::new(
        "sqlite-log",
        SqlWriterNodeConfig {
            database_path: database_path_string.clone(),
            table: "workflow_logs".to_owned(),
        },
        "write sqlite log",
    );

    let trace_id = Uuid::new_v4();
    let result = node
        .transform(trace_id, json!({ "value": 7, "status": "stored" }))
        .await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(
                    first_output.metadata["sql_writer"]["table"],
                    json!("workflow_logs")
                );
            }
            None => panic!("sql writer node should produce one output"),
        },
        Err(error) => panic!("sql writer node should execute successfully: {error}"),
    }

    let conn = match rusqlite::Connection::open(&database_path_string) {
        Ok(conn) => conn,
        Err(error) => panic!("should open the test database: {error}"),
    };
    let count: i64 =
        match conn.query_row("SELECT COUNT(*) FROM workflow_logs", [], |row| row.get(0)) {
            Ok(count) => count,
            Err(error) => panic!("should query the test database: {error}"),
        };
    assert_eq!(count, 1);

    let _ = std::fs::remove_file(database_path);
}

#[tokio::test]
async fn debug_console_node_marks_payload_and_passes_through() {
    let node = DebugConsoleNode::new(
        "debug-tap",
        DebugConsoleNodeConfig {
            label: Some("tap".to_owned()),
            pretty: false,
        },
        "debug payload",
    );

    let trace_id = Uuid::new_v4();
    let result = node
        .transform(trace_id, json!({ "status": "ok" }))
        .await;

    match result {
        Ok(execution) => match execution.first() {
            Some(first_output) => {
                assert_eq!(first_output.payload["status"], json!("ok"));
                assert_eq!(
                    first_output.metadata["debug_console"]["label"],
                    json!("tap")
                );
                assert_eq!(
                    first_output.metadata["debug_console"]["pretty"],
                    json!(false)
                );
            }
            None => panic!("debug console node should produce one output"),
        },
        Err(error) => panic!("debug console node should execute successfully: {error}"),
    }
}
