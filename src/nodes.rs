use async_trait::async_trait;
use chrono::Utc;
use rhai::{
    serde::{from_dynamic, to_dynamic},
    Dynamic, Engine, Scope, AST,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    ConnectionLease, EngineError, SharedConnectionManager, WorkflowContext,
};

#[async_trait]
pub trait NodeTrait: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> &'static str;
    fn ai_description(&self) -> &str;
    async fn execute(&self, ctx: WorkflowContext) -> Result<WorkflowContext, EngineError>;
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
        let payload_map = match ctx.payload {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_owned(), other);
                map
            }
        };

        let mut payload_map = payload_map;

        if let Some(message) = &self.config.message {
            payload_map.insert("_native_message".to_owned(), Value::String(message.clone()));
        }

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        if let Some(lease) = lease {
            let lease_value = serde_json::to_value(lease).map_err(|error| {
                EngineError::payload_conversion(self.id.clone(), error.to_string())
            })?;
            payload_map.insert("_connection".to_owned(), lease_value);
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

    async fn execute(&self, ctx: WorkflowContext) -> Result<WorkflowContext, EngineError> {
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

        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RhaiNodeConfig {
    pub script: String,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

fn default_max_operations() -> u64 {
    50_000
}

pub struct RhaiNode {
    id: String,
    ai_description: String,
    engine: Engine,
    ast: AST,
}

impl RhaiNode {
    pub fn new(
        id: impl Into<String>,
        config: RhaiNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let mut engine = Engine::new();
        engine.set_max_operations(config.max_operations);

        let ast = engine
            .compile(&config.script)
            .map_err(|error| EngineError::rhai_compile(id.clone(), error.to_string()))?;

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

    async fn execute(&self, ctx: WorkflowContext) -> Result<WorkflowContext, EngineError> {
        let payload = to_dynamic(ctx.payload.clone())
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;

        let mut scope = Scope::new();
        scope.push_dynamic("payload", payload);

        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .map_err(|error| EngineError::rhai_runtime(self.id.clone(), error.to_string()))?;

        let output = if result.is_unit() {
            scope
                .get_value::<Dynamic>("payload")
                .ok_or_else(|| {
                    EngineError::payload_conversion(
                        self.id.clone(),
                        "script returned unit and did not leave a `payload` value in scope",
                    )
                })?
        } else {
            result
        };

        let payload = from_dynamic::<Value>(&output)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;

        Ok(ctx.with_payload(payload))
    }
}
