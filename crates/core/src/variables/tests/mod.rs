use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::*;
use crate::PinType;
use serde_json::Value;

impl WorkflowVariables {
    fn dropped_variable_event_count_for_test(&self) -> u64 {
        let Some(sink) = self.event_sink.get() else {
            return 0;
        };
        sink.dropped_events.load(Ordering::Relaxed)
    }
}

fn vars_with(name: &str, ty: PinType, initial: Value) -> Arc<WorkflowVariables> {
    Arc::new(
        WorkflowVariables::from_declarations(&HashMap::from([(
            name.to_owned(),
            VariableDeclaration {
                variable_type: ty,
                initial,
            },
        )]))
        .expect("初始化应成功"),
    )
}

mod basic;
mod events;
mod watch;
