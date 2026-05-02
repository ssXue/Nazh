use std::{collections::HashMap, path::PathBuf, sync::Arc};

use ai::{AiConfigFile, OpenAiCompatibleService};
use nazh_engine::{ConnectionDefinition, shared_connection_manager};
use tauri::{AppHandle, Manager};
use tokio::{
    fs,
    sync::{Mutex, RwLock},
};

use crate::{runtime::DesktopWorkflow, workspace::resolve_project_workspace_dir};

/// Tauri 托管的应用状态，持有连接池和当前活跃的工作流。
///
/// `ai_service` 持有具体类型（`Arc<OpenAiCompatibleService>`）而非 `dyn AiService`，
/// 因为壳层除了 trait 方法外还要调用 inherent 的 `test_connection`（草稿配置
/// 测试不属于 Ring 0 运行时关注点）。注入到引擎部署时会自动 unsize 到
/// `Arc<dyn AiService>`。
pub(crate) struct DesktopState {
    pub(crate) connection_manager: nazh_engine::SharedConnectionManager,
    pub(crate) workflows: Mutex<HashMap<String, DesktopWorkflow>>,
    pub(crate) active_workflow_id: Mutex<Option<String>>,
    pub(crate) ai_config: Arc<RwLock<AiConfigFile>>,
    pub(crate) ai_service: Arc<OpenAiCompatibleService>,
    pub(crate) approval_registry: Arc<nazh_engine::ApprovalRegistry>,
}

impl Default for DesktopState {
    fn default() -> Self {
        let ai_config = Arc::new(RwLock::new(AiConfigFile::default()));
        let ai_service = Arc::new(OpenAiCompatibleService::new(Arc::clone(&ai_config)));
        Self {
            connection_manager: shared_connection_manager(),
            workflows: Mutex::new(HashMap::new()),
            active_workflow_id: Mutex::new(None),
            ai_config,
            ai_service,
            approval_registry: Arc::new(nazh_engine::ApprovalRegistry::new()),
        }
    }
}

impl DesktopState {
    pub(crate) fn connections_file_path(
        app: &AppHandle,
        workspace_path: Option<&str>,
    ) -> Result<PathBuf, String> {
        let workspace_dir =
            resolve_project_workspace_dir(app, workspace_path).map(|(dir, _)| dir)?;
        Ok(workspace_dir.join("connections.json"))
    }

    pub(crate) fn deployment_session_file_path(
        app: &AppHandle,
        workspace_path: Option<&str>,
    ) -> Result<PathBuf, String> {
        let workspace_dir =
            resolve_project_workspace_dir(app, workspace_path).map(|(dir, _)| dir)?;
        Ok(workspace_dir.join("deployment-session.json"))
    }

    pub(crate) fn ai_config_file_path(app: &AppHandle) -> Result<PathBuf, String> {
        let data_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("无法解析应用数据目录: {error}"))?;
        Ok(data_dir.join("ai-config.json"))
    }

    pub(crate) async fn load_connections_from_disk(
        app: &AppHandle,
        manager: nazh_engine::SharedConnectionManager,
        workspace_path: Option<&str>,
    ) {
        match Self::connections_file_path(app, workspace_path) {
            Ok(path) => {
                if path.exists() {
                    if let Ok(text) = fs::read_to_string(&path).await {
                        if let Ok(defs) =
                            serde_json::from_str::<Vec<nazh_engine::ConnectionDefinition>>(&text)
                        {
                            manager.replace_connections(defs).await;
                        } else {
                            manager
                                .replace_connections(Vec::<ConnectionDefinition>::new())
                                .await;
                        }
                    } else {
                        manager
                            .replace_connections(Vec::<ConnectionDefinition>::new())
                            .await;
                    }
                } else {
                    manager
                        .replace_connections(Vec::<ConnectionDefinition>::new())
                        .await;
                }
            }
            Err(_) => {
                manager
                    .replace_connections(Vec::<ConnectionDefinition>::new())
                    .await;
            }
        }
    }

    pub(crate) async fn resolve_workflow_id(
        &self,
        requested_workflow_id: Option<&str>,
    ) -> Result<Option<String>, String> {
        if let Some(requested) = requested_workflow_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let workflows = self.workflows.lock().await;
            if workflows.contains_key(requested) {
                return Ok(Some(requested.to_owned()));
            }
            return Err(format!("运行中的工作流 `{requested}` 不存在"));
        }

        Ok(self.active_workflow_id.lock().await.clone())
    }

    pub(crate) async fn choose_fallback_active_workflow(&self) -> Option<String> {
        let workflows = self.workflows.lock().await;
        workflows
            .values()
            .max_by(|left, right| left.metadata.deployed_at.cmp(&right.metadata.deployed_at))
            .map(|workflow| workflow.workflow_id.clone())
    }
}
