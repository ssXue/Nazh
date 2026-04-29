use std::sync::OnceLock;

/// 共享的节点注册表。
///
/// `standard_registry()` 会重新加载所有插件并构造注册表；IPC describe / deploy
/// 路径只读访问，使用 `OnceLock` 避免每次命令重复构造。
pub(crate) fn shared_node_registry() -> &'static nazh_engine::NodeRegistry {
    static CELL: OnceLock<nazh_engine::NodeRegistry> = OnceLock::new();
    CELL.get_or_init(nazh_engine::standard_registry)
}
