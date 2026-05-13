use super::*;
use nazh_core::{ArenaDataStore, NodeExecution, Uuid};
use serde_json::{Value, json};

struct EchoNode;

#[async_trait::async_trait]
impl NodeTrait for EchoNode {
    fn id(&self) -> &'static str {
        "echo"
    }

    fn kind(&self) -> &'static str {
        "testEcho"
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

struct DataOnlyNode;

#[async_trait::async_trait]
impl NodeTrait for DataOnlyNode {
    fn id(&self) -> &'static str {
        "data-only"
    }

    fn kind(&self) -> &'static str {
        "testDataOnly"
    }

    fn output_pins(&self) -> Vec<nazh_core::PinDefinition> {
        vec![nazh_core::PinDefinition::output_named_data(
            "out",
            "数据输出",
            nazh_core::PinType::Json,
            "仅写缓存，不进入 result",
        )]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

#[tokio::test]
async fn downstream_channel_closed_释放_output_payload() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (input_tx, input_rx) = mpsc::channel(1);
    let (downstream_tx, downstream_rx) = mpsc::channel(1);
    drop(downstream_rx);
    let (result_tx, _result_rx) = mpsc::channel(1);
    let (event_tx, _event_rx) = mpsc::channel(8);

    let trace_id = Uuid::new_v4();
    let data_id = store.write(json!({"value": 42}), 1).unwrap();
    input_tx
        .send(ContextRef::new(trace_id, data_id, None))
        .await
        .unwrap();
    drop(input_tx);

    run_node(
        Arc::new(EchoNode),
        None,
        input_rx,
        vec![DownstreamTarget {
            source_port_id: None,
            sender: downstream_tx,
            target_node_id: "closed-downstream".to_owned(),
            target_port_id: None,
            edge_kind: PinKind::Exec,
        }],
        result_tx,
        event_tx,
        store_dyn,
        Arc::new(OutputCache::new()),
        HashSet::new(),
        Arc::new(EdgesByConsumer::default()),
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(PureMemo::new()),
        HashSet::new(),
    )
    .await;

    assert!(store.is_empty(), "下游关闭时 output payload 必须释放");
}

#[tokio::test]
async fn data_only_output_不进入_result_stream() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (input_tx, input_rx) = mpsc::channel(1);
    let (result_tx, mut result_rx) = mpsc::channel(1);
    let (event_tx, _event_rx) = mpsc::channel(8);
    let output_cache = Arc::new(OutputCache::new());
    output_cache.prepare_slot("out");

    let trace_id = Uuid::new_v4();
    let data_id = store.write(json!({"value": 42}), 1).unwrap();
    input_tx
        .send(ContextRef::new(trace_id, data_id, None))
        .await
        .unwrap();
    drop(input_tx);

    run_node(
        Arc::new(DataOnlyNode),
        None,
        input_rx,
        vec![],
        result_tx,
        event_tx,
        store_dyn,
        Arc::clone(&output_cache),
        HashSet::from(["out".to_owned()]),
        Arc::new(EdgesByConsumer::default()),
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(PureMemo::new()),
        HashSet::new(),
    )
    .await;

    assert!(
        result_rx.try_recv().is_err(),
        "Data-only 输出不应产生 result"
    );
    assert!(
        store.is_empty(),
        "Data-only 输出不应写入无人消费的 DataStore entry"
    );
    assert_eq!(
        output_cache.read("out", None).unwrap().value,
        json!({"value": 42})
    );
}

#[tokio::test]
async fn 边传输窗口在节点空闲时也会定时刷新() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (input_tx, input_rx) = mpsc::channel(1);
    let (downstream_tx, mut downstream_rx) = mpsc::channel(4);
    let (result_tx, _result_rx) = mpsc::channel(1);
    let (event_tx, mut event_rx) = mpsc::channel(16);

    let trace_id = Uuid::new_v4();
    let data_id = store.write(json!({"value": 42}), 1).unwrap();

    let runner = tokio::spawn(run_node(
        Arc::new(EchoNode),
        None,
        input_rx,
        vec![DownstreamTarget {
            source_port_id: None,
            sender: downstream_tx,
            target_node_id: "sink".to_owned(),
            target_port_id: None,
            edge_kind: PinKind::Exec,
        }],
        result_tx,
        event_tx,
        store_dyn,
        Arc::new(OutputCache::new()),
        HashSet::new(),
        Arc::new(EdgesByConsumer::default()),
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(HashMap::new()),
        Arc::new(PureMemo::new()),
        HashSet::new(),
    ));

    input_tx
        .send(ContextRef::new(trace_id, data_id, None))
        .await
        .unwrap();

    let deadline = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(deadline);

    let mut summary = None;
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                if let Some(ExecutionEvent::EdgeTransmitSummary(edge_summary)) = event {
                    summary = Some(edge_summary);
                    break;
                }
            }
            () = &mut deadline => break,
        }
    }

    let summary = summary.expect("应在节点继续等待输入时定时刷新边传输窗口");
    assert_eq!(summary.from_node, "echo");
    assert_eq!(summary.from_pin, "out");
    assert_eq!(summary.to_node, "sink");
    assert_eq!(summary.to_pin, "in");
    assert_eq!(summary.transmit_count, 1);

    assert!(downstream_rx.try_recv().is_ok(), "下游仍应收到 ContextRef");

    drop(input_tx);
    runner.await.unwrap();
}

#[test]
fn broadcast_同时含_exec_与_data_输出时不是_data_only() {
    let data_output_pin_ids = HashSet::from(["latest".to_owned()]);
    let reactive_output_pin_ids = HashSet::new();

    assert!(
        !output_is_data_only(
            &NodeDispatch::Broadcast,
            &[],
            &data_output_pin_ids,
            &reactive_output_pin_ids,
            true,
        ),
        "带默认 Exec out 的节点即使有 Data latest，也必须继续产生 result"
    );
}
