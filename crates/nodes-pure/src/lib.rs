//! Nazh 纯计算节点（Ring 1）：c2f / minutesSince 等无副作用变换。
//!
//! ADR-0014 Phase 3 引入。所有节点的 input/output 引脚均为
//! [`PinKind::Data`](nazh_core::PinKind::Data)，即 [`is_pure_form`](nazh_core::is_pure_form)
//! 判定为 `true`——它们不参与触发链，仅在被下游 Data 输入拉取时即时求值。
//!
//! 同时打上 [`NodeCapabilities::PURE`](nazh_core::NodeCapabilities::PURE)
//! capability：与 ADR-0011 PURE 优化提示语义一致（同输入同输出 / 无副作用），
//! 为未来 Phase 4 输入哈希缓存奠定元数据基础。

use nazh_core::{NodeCapabilities, NodeRegistry, Plugin, PluginManifest};

mod c2f;
mod minutes_since;
pub use c2f::C2fNode;
pub use minutes_since::MinutesSinceNode;

pub struct PurePlugin;

impl Plugin for PurePlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "nodes-pure",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn register(&self, registry: &mut NodeRegistry) {
        registry.register_with_capabilities("c2f", NodeCapabilities::PURE, |def, _res| {
            Ok(std::sync::Arc::new(C2fNode::new(def.id().to_owned())))
        });
        registry.register_with_capabilities(
            "minutesSince",
            NodeCapabilities::PURE,
            |def, _res| {
                Ok(std::sync::Arc::new(MinutesSinceNode::new(
                    def.id().to_owned(),
                )))
            },
        );
    }
}
