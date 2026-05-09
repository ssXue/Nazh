//! 状态图完整性校验。
//!
//! 覆盖可达性、死胡同状态、确定性 transition 环和无条件循环判断。

use std::collections::{HashMap, HashSet};

use nazh_dsl_core::workflow::WorkflowSpec;

use super::report::{SafetyReport, diag_error, diag_warning, diag_warning_with};

/// 检查不可达状态、死胡同状态和循环触发。
pub(super) fn check_state_machine_completeness(
    spec: &WorkflowSpec,
    initial_state: &str,
    report: &mut SafetyReport,
) {
    let reachable = find_reachable_states(spec, initial_state);
    for state_name in spec.states.keys() {
        if !reachable.contains(state_name.as_str()) {
            diag_warning_with(
                report,
                "state_machine_completeness",
                format!("状态 `{state_name}` 不可达（无 incoming transition 且非初始状态）"),
                Some(state_name),
                None,
                None,
                None,
            );
        }
    }

    let dead_ends = find_dead_end_states(spec);
    for state_name in &dead_ends {
        diag_warning_with(
            report,
            "state_machine_completeness",
            format!("状态 `{state_name}` 为死胡同（无 outgoing transition 且无 timeout）"),
            Some(state_name),
            None,
            None,
            None,
        );
    }

    let cycles = find_trigger_cycles(spec);
    for cycle in cycles {
        let path = cycle.join(" → ");
        if is_unconditional_cycle(spec, &cycle) {
            diag_error(
                report,
                "state_machine_completeness",
                format!("检测到无条件循环触发路径: {path}"),
            );
        } else {
            diag_warning(
                report,
                "state_machine_completeness",
                format!("检测到有条件状态回路: {path}，请确认触发条件会等待外部输入或状态变化"),
            );
        }
    }
}

/// 从初始状态沿 transition 做真实可达遍历。
fn find_reachable_states(spec: &WorkflowSpec, initial_state: &str) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    reachable.insert(initial_state.to_owned());

    let mut changed = true;
    while changed {
        changed = false;
        for trans in &spec.transitions {
            if (trans.from == "*" || reachable.contains(&trans.from))
                && !reachable.contains(&trans.to)
            {
                reachable.insert(trans.to.clone());
                changed = true;
            }
        }
    }

    reachable
}

/// 找出所有死胡同状态。
fn find_dead_end_states(spec: &WorkflowSpec) -> Vec<String> {
    let has_outgoing: HashSet<&str> = spec
        .transitions
        .iter()
        .filter(|t| t.from != "*")
        .map(|t| t.from.as_str())
        .collect();

    let has_wildcard_outgoing = spec.transitions.iter().any(|t| t.from == "*");

    spec.states
        .keys()
        .filter(|name| {
            !has_outgoing.contains(name.as_str())
                && !has_wildcard_outgoing
                && !spec.timeout.contains_key(*name)
                && !is_terminal_state_hint(name)
        })
        .cloned()
        .collect()
}

/// 终端状态名称启发式判断。
fn is_terminal_state_hint(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("done")
        || lower.contains("complete")
        || lower.contains("end")
        || lower.contains("fault")
        || lower.contains("error")
        || lower.contains("finish")
}

/// 在确定性 transition 图上用 DFS 检测环。
fn find_trigger_cycles(spec: &WorkflowSpec) -> Vec<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for trans in &spec.transitions {
        if trans.from != "*" {
            adj.entry(&trans.from).or_default().push(&trans.to);
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();
    let mut path: Vec<String> = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for state_name in spec.states.keys() {
        dfs_find_cycles(
            state_name,
            &adj,
            &mut visited,
            &mut in_stack,
            &mut path,
            &mut cycles,
        );
    }

    cycles
}

fn dfs_find_cycles(
    node: &str,
    adj: &HashMap<&str, Vec<&str>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    if in_stack.contains(node) {
        if let Some(start) = path.iter().position(|p| p == node) {
            let cycle: Vec<String> = path[start..].to_vec();
            let mut normalized = cycle.clone();
            normalized.sort();
            if !cycles.iter().any(|existing| {
                let mut ex = existing.clone();
                ex.sort();
                ex == normalized
            }) {
                cycles.push(cycle);
            }
        }
        return;
    }

    if visited.contains(node) {
        return;
    }

    visited.insert(node.to_owned());
    in_stack.insert(node.to_owned());
    path.push(node.to_owned());

    if let Some(neighbors) = adj.get(node) {
        for &next in neighbors {
            dfs_find_cycles(next, adj, visited, in_stack, path, cycles);
        }
    }

    path.pop();
    in_stack.remove(node);
}

fn is_unconditional_cycle(spec: &WorkflowSpec, cycle: &[String]) -> bool {
    if cycle.is_empty() {
        return false;
    }

    for i in 0..cycle.len() {
        let from = &cycle[i];
        let to = &cycle[(i + 1) % cycle.len()];
        let has_unconditional_edge = spec.transitions.iter().any(|trans| {
            trans.from == *from && trans.to == *to && is_unconditional_when(&trans.when)
        });
        if !has_unconditional_edge {
            return false;
        }
    }
    true
}

fn is_unconditional_when(expr: &str) -> bool {
    matches!(expr.trim(), "" | "true")
}
