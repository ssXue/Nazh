//! Nazh 流程控制节点（Ring 1）：if / switch / loop / tryCatch / rhai。

use std::sync::Arc;

use nazh_ai_core::AiService;
use nazh_core::{NodeRegistry, Plugin, PluginManifest};

mod if_node;
mod loop_node;
mod rhai_node;
mod switch_node;
mod try_catch;

pub use if_node::{IfNode, IfNodeConfig};
pub use loop_node::{LoopNode, LoopNodeConfig};
pub use rhai_node::{RhaiNode, RhaiNodeAiConfig, RhaiNodeConfig};
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
        registry.register("rhai", |def, res| {
            let config: RhaiNodeConfig = def.parse_config()?;
            let ai_service = res.get::<Arc<dyn AiService>>();
            Ok(Arc::new(RhaiNode::new(def.id.clone(), config, ai_service)?))
        });
        let _ = registry.alias("code", "rhai");
        let _ = registry.alias("code/rhai", "rhai");

        registry.register("if", |def, _res| {
            let config: IfNodeConfig = def.parse_config()?;
            Ok(Arc::new(IfNode::new(def.id.clone(), config)?))
        });

        registry.register("switch", |def, _res| {
            let config: SwitchNodeConfig = def.parse_config()?;
            Ok(Arc::new(SwitchNode::new(def.id.clone(), config)?))
        });

        registry.register("tryCatch", |def, _res| {
            let config: TryCatchNodeConfig = def.parse_config()?;
            Ok(Arc::new(TryCatchNode::new(def.id.clone(), config)?))
        });

        registry.register("loop", |def, _res| {
            let config: LoopNodeConfig = def.parse_config()?;
            Ok(Arc::new(LoopNode::new(def.id.clone(), config)?))
        });
    }
}
