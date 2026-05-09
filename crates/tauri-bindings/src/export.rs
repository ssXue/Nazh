#[cfg(feature = "ts-export")]
use ts_rs::{Config, TS};

#[cfg(feature = "ts-export")]
use crate::*;

/// 触发本 crate 与所有依赖 crate 的 ts-rs 导出。
///
/// 集中入口避免新增类型时漏导出；CI 通过 `git diff --exit-code -- web/src/generated/`
/// 兜底，开发者改了 Rust 类型却忘了 regenerate 会立刻失败。
#[cfg(feature = "ts-export")]
pub fn export_all() -> Result<(), ts_rs::ExportError> {
    nazh_core::export_bindings::export_all()?;
    connections::export_bindings::export_all()?;
    ai::export_bindings::export_all()?;
    nazh_engine::export_bindings::export_all()?;

    let cfg = Config::from_env();

    DeployResponse::export(&cfg)?;
    DispatchResponse::export(&cfg)?;
    UndeployResponse::export(&cfg)?;
    NodeTypeEntry::export(&cfg)?;
    ListNodeTypesResponse::export(&cfg)?;
    DescribeNodePinsRequest::export(&cfg)?;
    DescribeNodePinsResponse::export(&cfg)?;
    SnapshotWorkflowVariablesRequest::export(&cfg)?;
    SnapshotWorkflowVariablesResponse::export(&cfg)?;
    SetWorkflowVariableRequest::export(&cfg)?;
    SetWorkflowVariableResponse::export(&cfg)?;
    VariableChangedPayload::export(&cfg)?;
    VariableDeletedPayload::export(&cfg)?;
    DeleteWorkflowVariableRequest::export(&cfg)?;
    DeleteWorkflowVariableResponse::export(&cfg)?;
    ResetWorkflowVariableRequest::export(&cfg)?;
    ResetWorkflowVariableResponse::export(&cfg)?;
    QueryVariableHistoryRequest::export(&cfg)?;
    QueryVariableHistoryResponse::export(&cfg)?;
    HistoryEntryPayload::export(&cfg)?;
    SetGlobalVariableRequest::export(&cfg)?;
    SetGlobalVariableResponse::export(&cfg)?;
    GlobalVariableSnapshot::export(&cfg)?;
    GetGlobalVariableRequest::export(&cfg)?;
    GetGlobalVariableResponse::export(&cfg)?;
    ListGlobalVariablesRequest::export(&cfg)?;
    ListGlobalVariablesResponse::export(&cfg)?;
    DeleteGlobalVariableRequest::export(&cfg)?;
    ReactiveUpdatePayload::export(&cfg)?;

    // 运行时类型（从 src-tauri 迁入）
    RuntimeBackpressureStrategy::export(&cfg)?;
    WorkflowRuntimePolicy::export(&cfg)?;
    WorkflowRuntimePolicyInput::export(&cfg)?;
    DispatchLaneSnapshot::export(&cfg)?;
    RuntimeWorkflowSummary::export(&cfg)?;
    DeadLetterRecord::export(&cfg)?;

    // 可观测性类型（从 src-tauri 迁入）
    ObservabilityContextInput::export(&cfg)?;
    ObservabilityEntry::export(&cfg)?;
    AlertDeliveryRecord::export(&cfg)?;
    ObservabilityTraceSummary::export(&cfg)?;
    ObservabilityQueryResult::export(&cfg)?;

    // 串口类型（从 src-tauri 迁入）
    SerialPortInfo::export(&cfg)?;
    TestSerialResult::export(&cfg)?;

    // 部署会话类型（从 src-tauri 迁入）
    PersistedDeploymentSession::export(&cfg)?;
    PersistedDeploymentSessionCollection::export(&cfg)?;
    PersistedDeploymentSessionState::export(&cfg)?;

    // 连接类型（从 src-tauri 迁入）
    ConnectionDefinitionsLoadResult::export(&cfg)?;

    trim_typescript_trailing_whitespace(cfg.out_dir())?;
    Ok(())
}

#[cfg(feature = "ts-export")]
fn trim_typescript_trailing_whitespace(dir: &std::path::Path) -> Result<(), ts_rs::ExportError> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            trim_typescript_trailing_whitespace(&path)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("ts") {
            continue;
        }

        let source = std::fs::read_to_string(&path)?;
        let mut trimmed = source
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        if source.ends_with('\n') {
            trimmed.push('\n');
        }

        if trimmed != source {
            std::fs::write(path, trimmed)?;
        }
    }

    Ok(())
}
