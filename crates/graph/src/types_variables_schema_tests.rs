use super::*;
use nazh_core::ArenaDataStore;

fn ingress_with_sender(
    node_id: &str,
    sender: mpsc::Sender<ContextRef>,
    store: Arc<dyn DataStore>,
) -> WorkflowIngress {
    WorkflowIngress {
        root_nodes: vec![node_id.to_owned()],
        root_senders: HashMap::from([(node_id.to_owned(), sender)]),
        store,
    }
}

#[test]
fn 旧图无_variables_字段反序列化不报错() {
    let json = serde_json::json!({
        "nodes": {},
        "edges": []
    });
    let graph: WorkflowGraph = serde_json::from_value(json).unwrap();
    assert!(
        graph.variables.unwrap_or_default().is_empty(),
        "缺省 variables 应为 None（等价空表）"
    );
}

#[test]
fn variables_字段为_null_时反序列化为_none() {
    // `Option<HashMap>` 对应 JSON null → None；前端不会主动产出 null，
    // 但外部修改工程文件时能容错而不是报错。
    let json = serde_json::json!({ "nodes": {}, "edges": [], "variables": null });
    let graph: WorkflowGraph = serde_json::from_value(json).unwrap();
    assert!(
        graph.variables.is_none(),
        "variables: null 应反序列化为 None"
    );
}

#[test]
fn 新图含_variables_字段反序列化正确() {
    let json = serde_json::json!({
        "nodes": {},
        "edges": [],
        "variables": {
            "setpoint": {
                "type": { "kind": "float" },
                "initial": 25.0
            },
            "mode": {
                "type": { "kind": "string" },
                "initial": "auto"
            }
        }
    });
    let graph: WorkflowGraph = serde_json::from_value(json).unwrap();
    let vars = graph.variables.unwrap();
    assert_eq!(vars.len(), 2);
    assert_eq!(vars["setpoint"].variable_type, nazh_core::PinType::Float);
    assert_eq!(vars["mode"].initial, serde_json::Value::from("auto"));
}

#[tokio::test]
async fn submit_to_channel_closed_释放_payload() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (tx, rx) = mpsc::channel::<ContextRef>(1);
    drop(rx);
    let ingress = ingress_with_sender("root", tx, store_dyn);

    let err = ingress
        .submit_to(
            "root",
            WorkflowContext::new(serde_json::json!({"value": 42})),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, EngineError::ChannelClosed { .. }));
    assert!(store.is_empty(), "submit_to 发送失败时必须释放 payload");
}

#[test]
fn blocking_submit_to_channel_closed_释放_payload() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (tx, rx) = mpsc::channel::<ContextRef>(1);
    drop(rx);
    let ingress = ingress_with_sender("root", tx, store_dyn);

    let err = ingress
        .blocking_submit_to(
            "root",
            WorkflowContext::new(serde_json::json!({"value": 42})),
        )
        .unwrap_err();

    assert!(matches!(err, EngineError::ChannelClosed { .. }));
    assert!(
        store.is_empty(),
        "blocking_submit_to 发送失败时必须释放 payload"
    );
}
