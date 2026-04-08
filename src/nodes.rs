use std::process::Command;

use async_trait::async_trait;
use chrono::Utc;
use rhai::{
    serde::{from_dynamic, to_dynamic},
    Array, Dynamic, Engine, Scope, AST,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ConnectionLease, EngineError, SharedConnectionManager, WorkflowContext};

#[derive(Debug, Clone)]
pub enum NodeDispatch {
    Broadcast,
    Route(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct NodeOutput {
    pub ctx: WorkflowContext,
    pub dispatch: NodeDispatch,
}

#[derive(Debug, Clone)]
pub struct NodeExecution {
    pub outputs: Vec<NodeOutput>,
}

impl NodeExecution {
    pub fn broadcast(ctx: WorkflowContext) -> Self {
        Self {
            outputs: vec![NodeOutput {
                ctx,
                dispatch: NodeDispatch::Broadcast,
            }],
        }
    }

    pub fn route<I, S>(ctx: WorkflowContext, ports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            outputs: vec![NodeOutput {
                ctx,
                dispatch: NodeDispatch::Route(ports.into_iter().map(Into::into).collect()),
            }],
        }
    }

    pub fn from_outputs(outputs: Vec<NodeOutput>) -> Self {
        Self { outputs }
    }

    pub fn first(&self) -> Option<&NodeOutput> {
        self.outputs.first()
    }
}

#[async_trait]
pub trait NodeTrait: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> &'static str;
    fn ai_description(&self) -> &str;
    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NativeNodeConfig {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub inject: Map<String, Value>,
    #[serde(default)]
    pub connection_id: Option<String>,
}

pub struct NativeNode {
    id: String,
    ai_description: String,
    config: NativeNodeConfig,
    connection_manager: SharedConnectionManager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RhaiNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerNodeConfig {
    #[serde(default = "default_timer_interval_ms")]
    pub interval_ms: u64,
    #[serde(default)]
    pub immediate: bool,
    #[serde(default)]
    pub inject: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusReadNodeConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default = "default_modbus_unit_id")]
    pub unit_id: u16,
    #[serde(default = "default_modbus_register")]
    pub register: u16,
    #[serde(default = "default_modbus_quantity")]
    pub quantity: u16,
    #[serde(default = "default_modbus_base_value")]
    pub base_value: f64,
    #[serde(default = "default_modbus_amplitude")]
    pub amplitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwitchBranchConfig {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchNodeConfig {
    pub script: String,
    #[serde(default)]
    pub branches: Vec<SwitchBranchConfig>,
    #[serde(default = "default_switch_branch")]
    pub default_branch: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TryCatchNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientNodeConfig {
    pub url: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlWriterNodeConfig {
    #[serde(default = "default_sqlite_path")]
    pub database_path: String,
    #[serde(default = "default_sqlite_table")]
    pub table: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConsoleNodeConfig {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_debug_pretty")]
    pub pretty: bool,
}

pub struct RhaiNode {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
}

pub struct TimerNode {
    id: String,
    ai_description: String,
    config: TimerNodeConfig,
}

pub struct ModbusReadNode {
    id: String,
    ai_description: String,
    config: ModbusReadNodeConfig,
    connection_manager: SharedConnectionManager,
}

pub struct IfNode {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
}

pub struct SwitchNode {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
    default_branch: String,
}

pub struct TryCatchNode {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
}

pub struct HttpClientNode {
    id: String,
    ai_description: String,
    config: HttpClientNodeConfig,
}

pub struct SqlWriterNode {
    id: String,
    ai_description: String,
    config: SqlWriterNodeConfig,
}

pub struct DebugConsoleNode {
    id: String,
    ai_description: String,
    config: DebugConsoleNodeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

pub struct LoopNode {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
}

fn default_max_operations() -> u64 {
    50_000
}

fn default_timer_interval_ms() -> u64 {
    5_000
}

fn default_modbus_unit_id() -> u16 {
    1
}

fn default_modbus_register() -> u16 {
    40_001
}

fn default_modbus_quantity() -> u16 {
    1
}

fn default_modbus_base_value() -> f64 {
    64.0
}

fn default_modbus_amplitude() -> f64 {
    6.0
}

fn default_http_method() -> String {
    "POST".to_owned()
}

fn default_sqlite_path() -> String {
    "./nazh-local.sqlite3".to_owned()
}

fn default_sqlite_table() -> String {
    "workflow_logs".to_owned()
}

fn default_debug_pretty() -> bool {
    true
}

fn default_switch_branch() -> String {
    "default".to_owned()
}

fn build_rhai_engine(max_operations: u64) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(max_operations);
    engine
}

fn compile_rhai_ast(
    id: &str,
    engine: &Engine,
    script: &str,
) -> Result<AST, EngineError> {
    engine
        .compile(script)
        .map_err(|error| EngineError::rhai_compile(id.to_owned(), error.to_string()))
}

fn payload_from_scope(
    node_id: &str,
    scope: &Scope<'_>,
) -> Result<Value, EngineError> {
    let payload = scope.get_value::<Dynamic>("payload").ok_or_else(|| {
        EngineError::payload_conversion(
            node_id.to_owned(),
            "script did not leave a `payload` value in scope",
        )
    })?;

    from_dynamic::<Value>(&payload)
        .map_err(|error| EngineError::payload_conversion(node_id.to_owned(), error.to_string()))
}

fn evaluate_rhai(
    node_id: &str,
    engine: &Engine,
    ast: &AST,
    ctx: &WorkflowContext,
) -> Result<(Scope<'static>, Dynamic), EngineError> {
    let payload = to_dynamic(ctx.payload.clone())
        .map_err(|error| EngineError::payload_conversion(node_id.to_owned(), error.to_string()))?;

    let mut scope = Scope::new();
    scope.push_dynamic("payload", payload);

    let result = engine
        .eval_ast_with_scope::<Dynamic>(&mut scope, ast)
        .map_err(|error| EngineError::rhai_runtime(node_id.to_owned(), error.to_string()))?;

    Ok((scope, result))
}

fn build_error_payload(payload: Value, error_message: String) -> Value {
    match payload {
        Value::Object(mut map) => {
            map.insert("_error".to_owned(), Value::String(error_message));
            Value::Object(map)
        }
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map.insert("_error".to_owned(), Value::String(error_message));
            Value::Object(map)
        }
    }
}

fn into_payload_map(payload: Value) -> Map<String, Value> {
    match payload {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    }
}

fn number_to_value(value: f64) -> Value {
    if let Some(number) = serde_json::Number::from_f64(value) {
        Value::Number(number)
    } else {
        Value::Null
    }
}

fn round_measurement(value: f64) -> Value {
    number_to_value((value * 100.0).round() / 100.0)
}

fn insert_connection_lease(
    node_id: &str,
    payload_map: &mut Map<String, Value>,
    lease: &ConnectionLease,
) -> Result<(), EngineError> {
    let lease_value = serde_json::to_value(lease)
        .map_err(|error| EngineError::payload_conversion(node_id.to_owned(), error.to_string()))?;
    payload_map.insert("_connection".to_owned(), lease_value);
    Ok(())
}

fn value_to_header_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn parse_json_or_string(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_owned()))
    }
}

fn sanitize_sqlite_identifier(identifier: &str) -> Option<String> {
    let trimmed = identifier.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }

    if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

fn escape_sqlite_text(input: &str) -> String {
    input.replace('\'', "''")
}

fn with_loop_state(
    payload: Value,
    phase: &str,
    index: Option<usize>,
    count: usize,
    item: Option<Value>,
) -> Value {
    let mut payload_map = match payload {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    };

    let mut loop_map = Map::new();
    loop_map.insert("phase".to_owned(), Value::String(phase.to_owned()));
    loop_map.insert("count".to_owned(), Value::from(count as u64));

    if let Some(index) = index {
        loop_map.insert("index".to_owned(), Value::from(index as u64));
    }

    if let Some(item) = item {
        loop_map.insert("item".to_owned(), item);
    }

    payload_map.insert("_loop".to_owned(), Value::Object(loop_map));
    Value::Object(payload_map)
}

fn collect_loop_items(
    node_id: &str,
    result: Dynamic,
) -> Result<Vec<Option<Value>>, EngineError> {
    if let Some(count) = result.clone().try_cast::<i64>() {
        if count < 0 {
            return Err(EngineError::payload_conversion(
                node_id.to_owned(),
                "loop node script must return a non-negative integer or an array",
            ));
        }

        return Ok((0..count as usize).map(|_| None).collect());
    }

    if let Some(count) = result.clone().try_cast::<u64>() {
        return Ok((0..count as usize).map(|_| None).collect());
    }

    if let Some(items) = result.try_cast::<Array>() {
        return items
            .into_iter()
            .map(|item| {
                from_dynamic::<Value>(&item)
                    .map(Some)
                    .map_err(|error| {
                        EngineError::payload_conversion(
                            node_id.to_owned(),
                            format!("loop node item could not be converted to JSON: {error}"),
                        )
                    })
            })
            .collect();
    }

    Err(EngineError::payload_conversion(
        node_id.to_owned(),
        "loop node script must return a non-negative integer or an array",
    ))
}

impl TimerNode {
    pub fn new(
        id: impl Into<String>,
        config: TimerNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
        }
    }
}

#[async_trait]
impl NodeTrait for TimerNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "timer"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let mut payload_map = into_payload_map(ctx.payload);

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        let existing_timer = payload_map
            .remove("_timer")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
        let mut timer_meta = existing_timer;
        timer_meta.insert("node_id".to_owned(), Value::String(self.id.clone()));
        timer_meta.insert(
            "interval_ms".to_owned(),
            Value::from(self.config.interval_ms.max(1)),
        );
        timer_meta.insert("immediate".to_owned(), Value::Bool(self.config.immediate));
        timer_meta.insert(
            "triggered_at".to_owned(),
            Value::String(Utc::now().to_rfc3339()),
        );
        payload_map.insert("_timer".to_owned(), Value::Object(timer_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}

impl ModbusReadNode {
    pub fn new(
        id: impl Into<String>,
        config: ModbusReadNodeConfig,
        ai_description: impl Into<String>,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
            connection_manager,
        }
    }
}

#[async_trait]
impl NodeTrait for ModbusReadNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "modbusRead"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let leased_connection = if let Some(connection_id) = &self.config.connection_id {
            let mut manager = self.connection_manager.write().await;
            Some(manager.borrow(connection_id)?)
        } else {
            None
        };

        let now_seconds = Utc::now().timestamp_millis() as f64 / 1000.0;
        let quantity = self.config.quantity.max(1).min(32);
        let values = (0..quantity)
            .map(|offset| {
                let phase = now_seconds / 4.8
                    + (self.config.register as f64 / 113.0)
                    + (offset as f64 * 0.41);
                round_measurement(self.config.base_value + self.config.amplitude * phase.sin())
            })
            .collect::<Vec<_>>();

        let result = (|| -> Result<WorkflowContext, EngineError> {
            let mut payload_map = into_payload_map(ctx.payload);

            if quantity == 1 {
                if let Some(value) = values.first() {
                    payload_map.insert("value".to_owned(), value.clone());
                }
            } else {
                payload_map.insert("values".to_owned(), Value::Array(values.clone()));
            }

            let mut modbus_meta = Map::new();
            modbus_meta.insert("simulated".to_owned(), Value::Bool(true));
            modbus_meta.insert("unit_id".to_owned(), Value::from(self.config.unit_id));
            modbus_meta.insert("register".to_owned(), Value::from(self.config.register));
            modbus_meta.insert("quantity".to_owned(), Value::from(quantity));
            modbus_meta.insert("sampled_at".to_owned(), Value::String(Utc::now().to_rfc3339()));
            payload_map.insert("_modbus".to_owned(), Value::Object(modbus_meta));

            if let Some(lease) = leased_connection.as_ref() {
                insert_connection_lease(&self.id, &mut payload_map, lease)?;
            }

            Ok(WorkflowContext::from_parts(
                ctx.trace_id,
                Utc::now(),
                Value::Object(payload_map),
            ))
        })();

        if let Some(connection_id) = &self.config.connection_id {
            let mut manager = self.connection_manager.write().await;
            let release_result = manager.release(connection_id);
            if result.is_ok() {
                release_result?;
            }
        }

        result.map(NodeExecution::broadcast)
    }
}

impl NativeNode {
    pub fn new(
        id: impl Into<String>,
        config: NativeNodeConfig,
        ai_description: impl Into<String>,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
            connection_manager,
        }
    }

    fn build_payload(
        &self,
        ctx: WorkflowContext,
        lease: Option<&ConnectionLease>,
    ) -> Result<WorkflowContext, EngineError> {
        let mut payload_map = into_payload_map(ctx.payload);

        if let Some(message) = &self.config.message {
            payload_map.insert("_native_message".to_owned(), Value::String(message.clone()));
        }

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        if let Some(lease) = lease {
            insert_connection_lease(&self.id, &mut payload_map, lease)?;
        }

        println!(
            "[native:{}] trace_id={} message={}",
            self.id,
            ctx.trace_id,
            self.config.message.as_deref().unwrap_or("passthrough"),
        );

        Ok(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        ))
    }
}

#[async_trait]
impl NodeTrait for NativeNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "native"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let leased_connection = if let Some(connection_id) = &self.config.connection_id {
            let mut manager = self.connection_manager.write().await;
            Some(manager.borrow(connection_id)?)
        } else {
            None
        };

        let result = self.build_payload(ctx, leased_connection.as_ref());

        if let Some(connection_id) = &self.config.connection_id {
            let mut manager = self.connection_manager.write().await;
            let release_result = manager.release(connection_id);
            if result.is_ok() {
                release_result?;
            }
        }

        result.map(NodeExecution::broadcast)
    }
}

impl RhaiNode {
    pub fn new(
        id: impl Into<String>,
        config: RhaiNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let engine = build_rhai_engine(config.max_operations);
        let ast = compile_rhai_ast(&id, &engine, &config.script)?;

        Ok(Self {
            id,
            ai_description: ai_description.into(),
            engine,
            ast,
        })
    }
}

#[async_trait]
impl NodeTrait for RhaiNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "rhai"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let (scope, result) = evaluate_rhai(&self.id, &self.engine, &self.ast, &ctx)?;

        let payload = if result.is_unit() {
            payload_from_scope(&self.id, &scope)?
        } else {
            from_dynamic::<Value>(&result).map_err(|error| {
                EngineError::payload_conversion(self.id.clone(), error.to_string())
            })?
        };

        Ok(NodeExecution::broadcast(ctx.with_payload(payload)))
    }
}

impl IfNode {
    pub fn new(
        id: impl Into<String>,
        config: IfNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let engine = build_rhai_engine(config.max_operations);
        let ast = compile_rhai_ast(&id, &engine, &config.script)?;

        Ok(Self {
            id,
            ai_description: ai_description.into(),
            engine,
            ast,
        })
    }
}

#[async_trait]
impl NodeTrait for IfNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "if"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let (scope, result) = evaluate_rhai(&self.id, &self.engine, &self.ast, &ctx)?;
        let branch = from_dynamic::<bool>(&result).map_err(|error| {
            EngineError::payload_conversion(
                self.id.clone(),
                format!("if node script must return a boolean: {error}"),
            )
        })?;
        let payload = payload_from_scope(&self.id, &scope)?;

        Ok(NodeExecution::route(
            ctx.with_payload(payload),
            [if branch { "true" } else { "false" }],
        ))
    }
}

impl SwitchNode {
    pub fn new(
        id: impl Into<String>,
        config: SwitchNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let engine = build_rhai_engine(config.max_operations);
        let ast = compile_rhai_ast(&id, &engine, &config.script)?;

        Ok(Self {
            id,
            ai_description: ai_description.into(),
            engine,
            ast,
            default_branch: if config.default_branch.trim().is_empty() {
                default_switch_branch()
            } else {
                config.default_branch
            },
        })
    }
}

#[async_trait]
impl NodeTrait for SwitchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "switch"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let (scope, result) = evaluate_rhai(&self.id, &self.engine, &self.ast, &ctx)?;
        let payload = payload_from_scope(&self.id, &scope)?;
        let next_branch = if result.is_unit() {
            self.default_branch.clone()
        } else {
            let branch = result.to_string();
            let normalized = branch.trim();
            if normalized.is_empty() || normalized == "()" {
                self.default_branch.clone()
            } else {
                normalized.to_owned()
            }
        };

        Ok(NodeExecution::route(ctx.with_payload(payload), [next_branch]))
    }
}

impl TryCatchNode {
    pub fn new(
        id: impl Into<String>,
        config: TryCatchNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let engine = build_rhai_engine(config.max_operations);
        let ast = compile_rhai_ast(&id, &engine, &config.script)?;

        Ok(Self {
            id,
            ai_description: ai_description.into(),
            engine,
            ast,
        })
    }
}

#[async_trait]
impl NodeTrait for TryCatchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "tryCatch"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let payload = to_dynamic(ctx.payload.clone())
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;

        let mut scope = Scope::new();
        scope.push_dynamic("payload", payload);

        match self.engine.eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast) {
            Ok(result) => {
                let payload = if result.is_unit() {
                    payload_from_scope(&self.id, &scope)?
                } else {
                    from_dynamic::<Value>(&result).map_err(|error| {
                        EngineError::payload_conversion(self.id.clone(), error.to_string())
                    })?
                };

                Ok(NodeExecution::route(ctx.with_payload(payload), ["try"]))
            }
            Err(error) => {
                let base_payload = payload_from_scope(&self.id, &scope).unwrap_or_else(|_| ctx.payload.clone());
                let payload = build_error_payload(base_payload, error.to_string());
                Ok(NodeExecution::route(ctx.with_payload(payload), ["catch"]))
            }
        }
    }
}

impl HttpClientNode {
    pub fn new(
        id: impl Into<String>,
        config: HttpClientNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
        }
    }
}

#[async_trait]
impl NodeTrait for HttpClientNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "httpClient"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let method = self.config.method.trim().to_uppercase();
        let url = self.config.url.trim().to_owned();
        if url.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "HTTP Client 节点需要配置 URL",
            ));
        }

        let trace_id = ctx.trace_id;
        let payload = ctx.payload.clone();
        let payload_body = serde_json::to_string(&payload)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let headers = self
            .config
            .headers
            .iter()
            .map(|(key, value)| (key.clone(), value_to_header_string(value)))
            .collect::<Vec<_>>();
        let node_id = self.id.clone();
        let url_for_cmd = url.clone();
        let method_for_cmd = method.clone();

        let (status_code, response_value) = tokio::task::spawn_blocking(move || {
            let mut args = vec![
                "-sS".to_owned(),
                "-X".to_owned(),
                method_for_cmd.clone(),
                url_for_cmd.clone(),
                "--write-out".to_owned(),
                "\n__NAZH_STATUS__:%{http_code}".to_owned(),
            ];

            for (key, value) in &headers {
                args.push("-H".to_owned());
                args.push(format!("{key}: {value}"));
            }

            if method_for_cmd != "GET" && method_for_cmd != "HEAD" {
                args.push("-H".to_owned());
                args.push("Content-Type: application/json".to_owned());
                args.push("-d".to_owned());
                args.push(payload_body.clone());
            }

            let output = Command::new("curl")
                .args(&args)
                .output()
                .map_err(|error| {
                    EngineError::stage_execution(
                        node_id.clone(),
                        trace_id,
                        format!("curl 执行失败: {error}"),
                    )
                })?;

            if !output.status.success() {
                return Err(EngineError::stage_execution(
                    node_id.clone(),
                    trace_id,
                    String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                ));
            }

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let (body, status) = stdout
                .rsplit_once("\n__NAZH_STATUS__:")
                .ok_or_else(|| {
                    EngineError::stage_execution(
                        node_id.clone(),
                        trace_id,
                        "无法解析 HTTP Client 返回状态码",
                    )
                })?;
            let status_code = status.trim().parse::<u16>().map_err(|error| {
                EngineError::stage_execution(
                    node_id.clone(),
                    trace_id,
                    format!("HTTP 状态码解析失败: {error}"),
                )
            })?;

            Ok((status_code, parse_json_or_string(body)))
        })
        .await
        .map_err(|_| EngineError::StagePanicked {
            stage: self.id.clone(),
            trace_id,
        })??;

        if status_code >= 400 {
            return Err(EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!("HTTP Client 返回状态码 {status_code}"),
            ));
        }

        let mut payload_map = into_payload_map(payload);
        let mut http_meta = Map::new();
        http_meta.insert("url".to_owned(), Value::String(url));
        http_meta.insert("method".to_owned(), Value::String(method));
        http_meta.insert("status".to_owned(), Value::from(status_code));
        http_meta.insert("ok".to_owned(), Value::Bool(status_code < 400));
        http_meta.insert("requested_at".to_owned(), Value::String(Utc::now().to_rfc3339()));
        payload_map.insert("_http".to_owned(), Value::Object(http_meta));
        payload_map.insert("http_response".to_owned(), response_value);

        Ok(NodeExecution::broadcast(ctx.with_payload(Value::Object(payload_map))))
    }
}

impl SqlWriterNode {
    pub fn new(
        id: impl Into<String>,
        config: SqlWriterNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
        }
    }
}

#[async_trait]
impl NodeTrait for SqlWriterNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "sqlWriter"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let database_path = self.config.database_path.trim().to_owned();
        let table = sanitize_sqlite_identifier(&self.config.table).ok_or_else(|| {
            EngineError::node_config(
                self.id.clone(),
                "SQL Writer 表名只能包含字母、数字和下划线，且不能以数字开头",
            )
        })?;
        let trace_id = ctx.trace_id;
        let node_id = self.id.clone();
        let payload_json = serde_json::to_string(&ctx.payload)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let timestamp = Utc::now().to_rfc3339();
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY AUTOINCREMENT, trace_id TEXT NOT NULL, node_id TEXT NOT NULL, created_at TEXT NOT NULL, payload_json TEXT NOT NULL); INSERT INTO {table} (trace_id, node_id, created_at, payload_json) VALUES ('{trace}', '{node}', '{created}', '{payload}');",
            table = table,
            trace = escape_sqlite_text(&trace_id.to_string()),
            node = escape_sqlite_text(&node_id),
            created = escape_sqlite_text(&timestamp),
            payload = escape_sqlite_text(&payload_json),
        );
        let database_path_for_cmd = database_path.clone();

        tokio::task::spawn_blocking(move || {
            if let Some(parent) = std::path::Path::new(&database_path_for_cmd).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        EngineError::stage_execution(
                            node_id.clone(),
                            trace_id,
                            format!("创建 SQLite 目录失败: {error}"),
                        )
                    })?;
                }
            }

            let output = Command::new("sqlite3")
                .arg(&database_path_for_cmd)
                .arg(&sql)
                .output()
                .map_err(|error| {
                    EngineError::stage_execution(
                        node_id.clone(),
                        trace_id,
                        format!("sqlite3 执行失败: {error}"),
                    )
                })?;

            if output.status.success() {
                Ok(())
            } else {
                Err(EngineError::stage_execution(
                    node_id.clone(),
                    trace_id,
                    String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                ))
            }
        })
        .await
        .map_err(|_| EngineError::StagePanicked {
            stage: self.id.clone(),
            trace_id,
        })??;

        let trace_id = ctx.trace_id;
        let mut payload_map = into_payload_map(ctx.payload);
        let mut sql_meta = Map::new();
        sql_meta.insert("database_path".to_owned(), Value::String(database_path));
        sql_meta.insert("table".to_owned(), Value::String(table));
        sql_meta.insert("written_at".to_owned(), Value::String(timestamp));
        payload_map.insert("_sql_writer".to_owned(), Value::Object(sql_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}

impl DebugConsoleNode {
    pub fn new(
        id: impl Into<String>,
        config: DebugConsoleNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
        }
    }
}

#[async_trait]
impl NodeTrait for DebugConsoleNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "debugConsole"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let label = self
            .config
            .label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or("Debug Console");
        let rendered_payload = if self.config.pretty {
            serde_json::to_string_pretty(&ctx.payload)
        } else {
            serde_json::to_string(&ctx.payload)
        }
        .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;

        println!(
            "[debug-console:{}] trace_id={} label={}\n{}",
            self.id, ctx.trace_id, label, rendered_payload
        );

        let trace_id = ctx.trace_id;
        let mut payload_map = into_payload_map(ctx.payload);
        let mut debug_meta = Map::new();
        debug_meta.insert("label".to_owned(), Value::String(label.to_owned()));
        debug_meta.insert("pretty".to_owned(), Value::Bool(self.config.pretty));
        debug_meta.insert("logged_at".to_owned(), Value::String(Utc::now().to_rfc3339()));
        payload_map.insert("_debug_console".to_owned(), Value::Object(debug_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}

impl LoopNode {
    pub fn new(
        id: impl Into<String>,
        config: LoopNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let engine = build_rhai_engine(config.max_operations);
        let ast = compile_rhai_ast(&id, &engine, &config.script)?;

        Ok(Self {
            id,
            ai_description: ai_description.into(),
            engine,
            ast,
        })
    }
}

#[async_trait]
impl NodeTrait for LoopNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "loop"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let (scope, result) = evaluate_rhai(&self.id, &self.engine, &self.ast, &ctx)?;
        let payload = payload_from_scope(&self.id, &scope)?;
        let items = collect_loop_items(&self.id, result)?;
        let item_count = items.len();
        let mut outputs = Vec::with_capacity(item_count + 1);

        for (index, item) in items.into_iter().enumerate() {
            outputs.push(NodeOutput {
                ctx: ctx.clone().with_payload(with_loop_state(
                    payload.clone(),
                    "body",
                    Some(index),
                    item_count,
                    item,
                )),
                dispatch: NodeDispatch::Route(vec!["body".to_owned()]),
            });
        }

        outputs.push(NodeOutput {
            ctx: ctx.with_payload(with_loop_state(payload, "done", None, item_count, None)),
            dispatch: NodeDispatch::Route(vec!["done".to_owned()]),
        });

        Ok(NodeExecution::from_outputs(outputs))
    }
}
