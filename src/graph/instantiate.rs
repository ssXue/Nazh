//! 节点工厂：根据 [`WorkflowNodeDefinition`] 中的 `node_type` 字段
//! 创建对应的 [`NodeTrait`] 实例。
//!
//! ## 添加新节点类型
//!
//! 1. 在 `nodes/` 目录下实现节点并导出 Config 和节点结构体
//! 2. 在下方 `instantiate_node` 的 match 中添加对应分支

use std::sync::Arc;

use serde::de::DeserializeOwned;

use super::types::WorkflowNodeDefinition;
use crate::{
    DebugConsoleNode, DebugConsoleNodeConfig, EngineError, HttpClientNode, HttpClientNodeConfig,
    IfNode, IfNodeConfig, LoopNode, LoopNodeConfig, ModbusReadNode, ModbusReadNodeConfig,
    NativeNode, NativeNodeConfig, NodeTrait, RhaiNode, RhaiNodeConfig, SharedConnectionManager,
    SqlWriterNode, SqlWriterNodeConfig, SwitchNode, SwitchNodeConfig, TimerNode, TimerNodeConfig,
    TryCatchNode, TryCatchNodeConfig,
};

/// 从节点定义中反序列化配置。
fn parse_config<T: DeserializeOwned>(definition: &WorkflowNodeDefinition) -> Result<T, EngineError> {
    serde_json::from_value(definition.config.clone())
        .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))
}

/// 根据节点定义的 `node_type` 实例化具体节点。
///
/// # Errors
///
/// 配置反序列化失败或节点类型不支持时返回 [`EngineError`]。
#[allow(clippy::too_many_lines)]
pub(crate) fn instantiate_node(
    definition: &WorkflowNodeDefinition,
    connection_manager: SharedConnectionManager,
) -> Result<Arc<dyn NodeTrait>, EngineError> {
    match definition.node_type.as_str() {
        "native" | "native/log" | "log" => {
            let mut config: NativeNodeConfig = parse_config(definition)?;
            if config.connection_id.is_none() {
                config.connection_id.clone_from(&definition.connection_id);
            }
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "打印 payload 元数据，可选附加连接上下文".to_owned()
            });
            Ok(Arc::new(NativeNode::new(
                definition.id.clone(),
                config,
                description,
                connection_manager,
            )))
        }
        "rhai" | "code" | "code/rhai" => {
            let config: RhaiNodeConfig = parse_config(definition)?;
            let description = definition
                .ai_description
                .clone()
                .unwrap_or_else(|| "使用有界 Rhai 脚本执行业务逻辑".to_owned());
            Ok(Arc::new(RhaiNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "timer" => {
            let config: TimerNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "按固定间隔触发工作流并注入计时元数据".to_owned()
            });
            Ok(Arc::new(TimerNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        "modbusRead" | "modbus/read" => {
            let mut config: ModbusReadNodeConfig =
                parse_config(definition)?;
            if config.connection_id.is_none() {
                config.connection_id.clone_from(&definition.connection_id);
            }
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "读取模拟 Modbus 寄存器并将遥测数据写入 payload".to_owned()
            });
            Ok(Arc::new(ModbusReadNode::new(
                definition.id.clone(),
                config,
                description,
                connection_manager,
            )))
        }
        "if" => {
            let config: IfNodeConfig =
                parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "求值布尔脚本并路由到 true 或 false 分支".to_owned()
            });
            Ok(Arc::new(IfNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "switch" => {
            let config: SwitchNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "求值路由脚本并分发到匹配的分支".to_owned()
            });
            Ok(Arc::new(SwitchNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "tryCatch" => {
            let config: TryCatchNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "执行受保护的脚本并路由到 try 或 catch 分支".to_owned()
            });
            Ok(Arc::new(TryCatchNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "loop" => {
            let config: LoopNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "求值可迭代脚本，逐项通过 body 分发，完成后发送 done"
                    .to_owned()
            });
            Ok(Arc::new(LoopNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "httpClient" | "http/client" => {
            let config: HttpClientNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "将 payload 发送到 HTTP 端点（如钉钉机器人告警）".to_owned()
            });
            Ok(Arc::new(HttpClientNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        "sqlWriter" | "sql/writer" => {
            let config: SqlWriterNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "将当前 payload 持久化到本地 SQLite 表".to_owned()
            });
            Ok(Arc::new(SqlWriterNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        "debugConsole" | "debug/console" => {
            let config: DebugConsoleNodeConfig = parse_config(definition)?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "将 payload 打印到调试控制台以供检查".to_owned()
            });
            Ok(Arc::new(DebugConsoleNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        other => Err(EngineError::unsupported_node_type(other)),
    }
}
