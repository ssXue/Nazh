//! 节点执行系统：统一的异步节点 Trait 与分发策略。
//!
//! 本模块定义了工作流节点的核心抽象 [`NodeTrait`]，以及节点输出的
//! 分发机制 [`NodeDispatch`]。具体节点实现分布在各子模块中：
//!
//! | 子模块 | 节点类型 | 说明 |
//! |--------|----------|------|
//! | [`native`] | `NativeNode` | 纯 Rust 原生逻辑，负责 I/O 与数据注入 |
//! | [`rhai`] | `RhaiNode` | 沙箱化 Rhai 脚本执行 |
//! | [`timer`] | `TimerNode` | 定时触发，注入计时元数据 |
//! | [`serial_trigger`] | `SerialTriggerNode` | 串口被动触发，接收 ASCII/HEX 数据帧 |
//! | [`modbus_read`] | `ModbusReadNode` | Modbus 寄存器读取（当前为模拟） |
//! | [`if_node`] | `IfNode` | 布尔条件分支路由 |
//! | [`switch_node`] | `SwitchNode` | 多路分支路由 |
//! | [`try_catch`] | `TryCatchNode` | 脚本异常捕获路由 |
//! | [`http_client`] | `HttpClientNode` | HTTP 请求与响应处理 |
//! | [`sql_writer`] | `SqlWriterNode` | `SQLite` 持久化写入 |
//! | [`debug_console`] | `DebugConsoleNode` | 调试输出到控制台 |
//! | [`loop_node`] | `LoopNode` | 循环迭代与逐项分发 |
//!
//! ## 添加新节点
//!
//! 1. 在 `nodes/` 下创建新文件，定义 Config 结构体和节点结构体
//! 2. 实现 [`NodeTrait`]；若为脚本节点，嵌入 [`helpers::RhaiNodeBase`]
//! 3. 在本文件中添加 `mod` 声明和 `pub use` 导出
//! 4. 在 `graph/instantiate.rs` 的工厂函数中添加匹配分支

/// 为嵌入 `RhaiNodeBase` 的脚本节点委托 [`NodeTrait`] 元数据方法。
///
/// 需要节点结构体含有 `base: RhaiNodeBase` 字段。
macro_rules! delegate_node_base {
    ($kind:expr) => {
        fn id(&self) -> &str {
            self.base.id()
        }
        fn kind(&self) -> &'static str {
            $kind
        }
        fn ai_description(&self) -> &str {
            self.base.ai_description()
        }
    };
}
#[allow(unused_imports)] // clippy 无法追踪 macro_rules! 宏的使用
pub(crate) use delegate_node_base;

/// 为持有 `id` 和 `ai_description` 字段的非脚本节点实现 [`NodeTrait`] 元数据方法。
macro_rules! impl_node_meta {
    ($kind:expr) => {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            $kind
        }
        fn ai_description(&self) -> &str {
            &self.ai_description
        }
    };
}
#[allow(unused_imports)] // clippy 无法追踪 macro_rules! 宏的使用
pub(crate) use impl_node_meta;

mod helpers;
pub(crate) mod template;

mod debug_console;
mod http_client;
mod if_node;
mod loop_node;
mod modbus_read;
mod native;
mod rhai;
mod serial_trigger;
mod sql_writer;
mod switch_node;
mod timer;
mod try_catch;

use async_trait::async_trait;

use crate::{EngineError, WorkflowContext};

pub use debug_console::{DebugConsoleNode, DebugConsoleNodeConfig};
pub use http_client::{HttpClientNode, HttpClientNodeConfig};
pub use if_node::{IfNode, IfNodeConfig};
pub use loop_node::{LoopNode, LoopNodeConfig};
pub use modbus_read::{ModbusReadNode, ModbusReadNodeConfig};
pub use native::{NativeNode, NativeNodeConfig};
pub use rhai::{RhaiNode, RhaiNodeConfig};
pub use serial_trigger::{SerialTriggerNode, SerialTriggerNodeConfig};
pub use sql_writer::{SqlWriterNode, SqlWriterNodeConfig};
pub use switch_node::{SwitchBranchConfig, SwitchNode, SwitchNodeConfig};
pub use timer::{TimerNode, TimerNodeConfig};
pub use try_catch::{TryCatchNode, TryCatchNodeConfig};

/// 节点输出的分发策略。
#[derive(Debug, Clone)]
pub enum NodeDispatch {
    /// 向所有下游节点广播。
    Broadcast,
    /// 按端口名称路由到特定下游。
    Route(Vec<String>),
}

/// 节点执行后产出的单条输出。
#[derive(Debug, Clone)]
pub struct NodeOutput {
    pub ctx: WorkflowContext,
    pub dispatch: NodeDispatch,
}

/// 节点执行结果，可包含多条输出（如循环节点为每个元素生成一条）。
#[derive(Debug, Clone)]
pub struct NodeExecution {
    pub outputs: Vec<NodeOutput>,
}

impl NodeExecution {
    /// 创建一条广播到所有下游的执行结果。
    pub fn broadcast(ctx: WorkflowContext) -> Self {
        Self {
            outputs: vec![NodeOutput {
                ctx,
                dispatch: NodeDispatch::Broadcast,
            }],
        }
    }

    /// 创建一条按端口路由的执行结果。
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

    /// 从多条输出构造执行结果。
    pub fn from_outputs(outputs: Vec<NodeOutput>) -> Self {
        Self { outputs }
    }

    /// 获取第一条输出（如果存在）。
    pub fn first(&self) -> Option<&NodeOutput> {
        self.outputs.first()
    }
}

/// 所有工作流节点的统一异步 Trait。
///
/// 实现必须满足 `Send + Sync`，因为每个节点在独立的 Tokio 任务中运行。
/// 新节点类型只需实现此 Trait 即可接入工作流 DAG。
#[async_trait]
pub trait NodeTrait: Send + Sync {
    /// 节点在工作流图中的唯一标识。
    fn id(&self) -> &str;
    /// 返回节点类型标识（如 `"native"`、`"rhai"`、`"timer"` 等）。
    fn kind(&self) -> &'static str;
    /// 供 LLM 代码生成使用的自然语言描述。
    fn ai_description(&self) -> &str;
    /// 处理一个 [`WorkflowContext`]，返回包含分发策略的 [`NodeExecution`]。
    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError>;
}
