//! Nazh 流程控制节点（Ring 1）：if / switch / loop / tryCatch / code。

use std::sync::Arc;

use nazh_core::ai::AiService;
use nazh_core::{NodeCapabilities, NodeRegistry, Plugin, PluginManifest};

mod code_node;
mod if_node;
mod loop_node;
mod switch_node;
mod try_catch;

pub use code_node::{CodeNode, CodeNodeAiConfig, CodeNodeConfig};
pub use if_node::{IfNode, IfNodeConfig};
pub use loop_node::{LoopNode, LoopNodeConfig};
pub use switch_node::{SwitchBranchConfig, SwitchNode, SwitchNodeConfig};
pub use try_catch::{TryCatchNode, TryCatchNodeConfig};

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "nodes-flow",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn register(&self, registry: &mut NodeRegistry) {
        registry.register_with_capabilities("code", NodeCapabilities::empty(), |def, res| {
            let config: CodeNodeConfig = def.parse_config()?;
            let ai_service = res.get::<Arc<dyn AiService>>();
            Ok(Arc::new(CodeNode::new(
                def.id().to_owned(),
                config,
                ai_service,
            )?))
        });

        registry.register_with_capabilities(
            "if",
            NodeCapabilities::PURE | NodeCapabilities::BRANCHING,
            |def, _res| {
                let config: IfNodeConfig = def.parse_config()?;
                Ok(Arc::new(IfNode::new(def.id().to_owned(), config)?))
            },
        );

        registry.register_with_capabilities(
            "switch",
            NodeCapabilities::PURE | NodeCapabilities::BRANCHING,
            |def, _res| {
                let config: SwitchNodeConfig = def.parse_config()?;
                Ok(Arc::new(SwitchNode::new(def.id().to_owned(), config)?))
            },
        );

        registry.register_with_capabilities(
            "tryCatch",
            NodeCapabilities::BRANCHING,
            |def, _res| {
                let config: TryCatchNodeConfig = def.parse_config()?;
                Ok(Arc::new(TryCatchNode::new(def.id().to_owned(), config)?))
            },
        );

        registry.register_with_capabilities(
            "loop",
            NodeCapabilities::BRANCHING | NodeCapabilities::MULTI_OUTPUT,
            |def, _res| {
                let config: LoopNodeConfig = def.parse_config()?;
                Ok(Arc::new(LoopNode::new(def.id().to_owned(), config)?))
            },
        );
    }
}
