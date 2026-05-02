//! 部署时注入的 `workflow_id` 类型包装。
//!
//! 通过 [`RuntimeResources`] 注入，节点工厂在实例化时读取。
//! 裸 `Arc<String>` 会与同类型资源冲突，包装为具名类型。

use std::sync::Arc;

/// 部署时注入的 `workflow_id`。
#[derive(Clone)]
pub struct WorkflowId(pub Arc<String>);

impl WorkflowId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
