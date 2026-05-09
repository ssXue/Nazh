//! 机械互锁校验。

use std::collections::{HashMap, HashSet};

use nazh_dsl_core::capability::{CapabilityImpl, CapabilitySpec};
use nazh_dsl_core::workflow::{ActionTarget, WorkflowSpec};

use crate::context::CompilerContext;

use super::collect_capability_ids;
use super::report::{SafetyReport, diag_warning};

/// 检查同一设备上多个能力是否存在寄存器写入冲突。
pub(super) fn check_mechanical_interlock(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    report: &mut SafetyReport,
) {
    let mut cap_ids: HashSet<String> = HashSet::new();
    for state in spec.states.values() {
        collect_capability_ids(&state.entry, &mut cap_ids);
        collect_capability_ids(&state.exit, &mut cap_ids);
    }
    for trans in &spec.transitions {
        if let Some(action) = &trans.action
            && let ActionTarget::Capability(id) = &action.target
        {
            cap_ids.insert(id.clone());
        }
    }

    let mut by_device: HashMap<String, Vec<&CapabilitySpec>> = HashMap::new();
    for cap_id in &cap_ids {
        if let Some(cap) = ctx.capabilities.get(cap_id) {
            by_device
                .entry(cap.device_id.clone())
                .or_default()
                .push(cap);
        }
    }

    for (device_id, caps) in &by_device {
        let conflicts = find_register_conflicts(caps);
        for (cap_a, cap_b, register) in conflicts {
            diag_warning(
                report,
                "mechanical_interlock",
                format!(
                    "设备 `{device_id}` 上的能力 `{cap_a}` 和 `{cap_b}` 均写入寄存器 {register}，可能存在并发冲突"
                ),
            );
        }
    }
}

/// 在同一设备的能力列表中查找 `ModbusWrite` 寄存器冲突。
fn find_register_conflicts(capabilities: &[&CapabilitySpec]) -> Vec<(String, String, u16)> {
    let mut conflicts = Vec::new();
    for i in 0..capabilities.len() {
        for j in (i + 1)..capabilities.len() {
            if let (
                CapabilityImpl::ModbusWrite { register: r1, .. },
                CapabilityImpl::ModbusWrite { register: r2, .. },
            ) = (
                &capabilities[i].implementation,
                &capabilities[j].implementation,
            ) && r1 == r2
            {
                conflicts.push((capabilities[i].id.clone(), capabilities[j].id.clone(), *r1));
            }
        }
    }
    conflicts
}
