//! 标准库节点注册：将引擎内置的所有节点类型注册到 [`NodeRegistry`]。
//!
//! 此前本文件包含一个 180 行的 `match` 工厂函数，每新增一种节点都要改动。
//! 现在所有节点通过 [`register_standard_nodes`] 注册到 [`NodeRegistry`]，
//! 引擎核心不再硬编码任何节点类型。
//!
//! ## 添加新节点类型
//!
//! 1. 在 `nodes/` 目录下实现节点并导出 Config 和节点结构体
//! 2. 在下方 [`register_standard_nodes`] 中添加 `register` + `alias` 调用
//! 3. 在 `nodes/mod.rs` 和 `lib.rs` 中导出

use std::sync::Arc;

use crate::registry::NodeRegistry;
use crate::{
    DebugConsoleNode, DebugConsoleNodeConfig, HttpClientNode, HttpClientNodeConfig, IfNode,
    IfNodeConfig, LoopNode, LoopNodeConfig, ModbusReadNode, ModbusReadNodeConfig, NativeNode,
    NativeNodeConfig, RhaiNode, RhaiNodeConfig, SerialTriggerNode, SerialTriggerNodeConfig,
    SqlWriterNode, SqlWriterNodeConfig, SwitchNode, SwitchNodeConfig, TimerNode, TimerNodeConfig,
    TryCatchNode, TryCatchNodeConfig,
};

/// 将所有标准库节点注册到注册表中。
///
/// 标准库包含引擎内置的 12 种节点类型及其别名，涵盖：
/// - 流程原语：if、switch、tryCatch、loop
/// - 脚本执行：rhai / code
/// - 数据注入：native / log
/// - 硬件接口：timer、serialTrigger、modbusRead
/// - 外部通信：httpClient
/// - 持久化：sqlWriter
/// - 调试工具：debugConsole
#[allow(clippy::too_many_lines)]
pub(crate) fn register_standard_nodes(registry: &mut NodeRegistry) {

    registry.register("native", |def, cm| {
        let mut config: NativeNodeConfig = def.parse_config()?;
        if config.connection_id.is_none() {
            config.connection_id.clone_from(&def.connection_id);
        }
        let description = def.resolve_description("打印 payload 元数据，可选附加连接上下文");
        Ok(Arc::new(NativeNode::new(
            def.id.clone(),
            config,
            description,
            cm,
        )))
    });
    // 别名注册不会失败，因为 "native" 刚注册过
    let _ = registry.alias("native/log", "native");
    let _ = registry.alias("log", "native");


    registry.register("rhai", |def, _cm| {
        let config: RhaiNodeConfig = def.parse_config()?;
        let description = def.resolve_description("使用有界 Rhai 脚本执行业务逻辑");
        Ok(Arc::new(RhaiNode::new(
            def.id.clone(),
            config,
            description,
        )?))
    });
    let _ = registry.alias("code", "rhai");
    let _ = registry.alias("code/rhai", "rhai");


    registry.register("timer", |def, _cm| {
        let config: TimerNodeConfig = def.parse_config()?;
        let description = def.resolve_description("按固定间隔触发工作流并注入计时元数据");
        Ok(Arc::new(TimerNode::new(
            def.id.clone(),
            config,
            description,
        )))
    });


    registry.register("serialTrigger", |def, _cm| {
        let config: SerialTriggerNodeConfig = def.parse_config()?;
        let description =
            def.resolve_description("接收串口外设主动上报的 ASCII/HEX 数据流并触发工作流");
        Ok(Arc::new(SerialTriggerNode::new(
            def.id.clone(),
            config,
            description,
        )))
    });
    let _ = registry.alias("serial/trigger", "serialTrigger");
    let _ = registry.alias("serial", "serialTrigger");


    registry.register("modbusRead", |def, cm| {
        let mut config: ModbusReadNodeConfig = def.parse_config()?;
        if config.connection_id.is_none() {
            config.connection_id.clone_from(&def.connection_id);
        }
        let description = def.resolve_description("读取模拟 Modbus 寄存器并将遥测数据写入 payload");
        Ok(Arc::new(ModbusReadNode::new(
            def.id.clone(),
            config,
            description,
            cm,
        )))
    });
    let _ = registry.alias("modbus/read", "modbusRead");


    registry.register("if", |def, _cm| {
        let config: IfNodeConfig = def.parse_config()?;
        let description = def.resolve_description("求值布尔脚本并路由到 true 或 false 分支");
        Ok(Arc::new(IfNode::new(def.id.clone(), config, description)?))
    });


    registry.register("switch", |def, _cm| {
        let config: SwitchNodeConfig = def.parse_config()?;
        let description = def.resolve_description("求值路由脚本并分发到匹配的分支");
        Ok(Arc::new(SwitchNode::new(
            def.id.clone(),
            config,
            description,
        )?))
    });


    registry.register("tryCatch", |def, _cm| {
        let config: TryCatchNodeConfig = def.parse_config()?;
        let description = def.resolve_description("执行受保护的脚本并路由到 try 或 catch 分支");
        Ok(Arc::new(TryCatchNode::new(
            def.id.clone(),
            config,
            description,
        )?))
    });


    registry.register("loop", |def, _cm| {
        let config: LoopNodeConfig = def.parse_config()?;
        let description =
            def.resolve_description("求值可迭代脚本，逐项通过 body 分发，完成后发送 done");
        Ok(Arc::new(LoopNode::new(
            def.id.clone(),
            config,
            description,
        )?))
    });


    registry.register("httpClient", |def, _cm| {
        let config: HttpClientNodeConfig = def.parse_config()?;
        let description =
            def.resolve_description("将 payload 发送到 HTTP 端点（如钉钉机器人告警）");
        Ok(Arc::new(HttpClientNode::new(
            def.id.clone(),
            config,
            description,
        )?))
    });
    let _ = registry.alias("http/client", "httpClient");


    registry.register("sqlWriter", |def, _cm| {
        let config: SqlWriterNodeConfig = def.parse_config()?;
        let description = def.resolve_description("将当前 payload 持久化到本地 SQLite 表");
        Ok(Arc::new(SqlWriterNode::new(
            def.id.clone(),
            config,
            description,
        )))
    });
    let _ = registry.alias("sql/writer", "sqlWriter");


    registry.register("debugConsole", |def, _cm| {
        let config: DebugConsoleNodeConfig = def.parse_config()?;
        let description = def.resolve_description("将 payload 打印到调试控制台以供检查");
        Ok(Arc::new(DebugConsoleNode::new(
            def.id.clone(),
            config,
            description,
        )))
    });
    let _ = registry.alias("debug/console", "debugConsole");
}
