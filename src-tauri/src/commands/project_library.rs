use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::fs;

use crate::workspace::resolve_project_workspace_dir;

const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;
const MAX_EXPORT_FILE_BYTES: usize = 25 * 1024 * 1024;
const PROJECT_BOARDS_DIR: &str = "boards";
const PROJECT_EXPORTS_DIR: &str = "exports";
const PROJECT_BOARD_FILE_SUFFIX: &str = ".nazh-board.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectWorkspaceStorageInfo {
    pub(crate) workspace_path: String,
    pub(crate) boards_directory_path: String,
    pub(crate) using_default_location: bool,
    pub(crate) board_file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectWorkspaceLoadResult {
    pub(crate) storage: ProjectWorkspaceStorageInfo,
    pub(crate) board_files: Vec<ProjectWorkspaceBoardFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectWorkspaceBoardFile {
    pub(crate) file_name: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedWorkspaceFile {
    pub(crate) file_path: String,
}

#[tauri::command]
pub(crate) async fn load_project_board_files(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<ProjectWorkspaceLoadResult, String> {
    let storage = resolve_project_workspace_storage(&app, workspace_path.as_deref())?;
    let board_file_paths =
        list_project_board_file_paths(Path::new(&storage.boards_directory_path))?;
    let mut board_files = Vec::with_capacity(board_file_paths.len());
    for file_path in board_file_paths {
        let text = fs::read_to_string(&file_path)
            .await
            .map_err(|error| format!("读取看板文件失败 `{}`: {error}", file_path.display()))?;
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("看板文件名无效: {}", file_path.display()))?
            .to_owned();
        board_files.push(ProjectWorkspaceBoardFile { file_name, text });
    }

    Ok(ProjectWorkspaceLoadResult {
        storage,
        board_files,
    })
}

#[tauri::command]
pub(crate) async fn save_project_board_files(
    app: AppHandle,
    workspace_path: Option<String>,
    board_files: Vec<ProjectWorkspaceBoardFile>,
) -> Result<ProjectWorkspaceStorageInfo, String> {
    for board_file in &board_files {
        if board_file.text.len() > MAX_IPC_INPUT_BYTES {
            return Err(format!(
                "看板文件 `{}` 超过最大允许大小（10 MB）",
                board_file.file_name
            ));
        }
    }

    let storage = resolve_project_workspace_storage(&app, workspace_path.as_deref())?;
    let workspace_dir = PathBuf::from(&storage.workspace_path);
    let boards_dir = PathBuf::from(&storage.boards_directory_path);

    fs::create_dir_all(&workspace_dir)
        .await
        .map_err(|error| format!("创建工程目录失败: {error}"))?;
    fs::create_dir_all(&boards_dir)
        .await
        .map_err(|error| format!("创建看板目录失败: {error}"))?;

    let mut expected_paths = std::collections::HashSet::new();
    for board_file in board_files {
        let file_name = sanitize_project_board_file_name(&board_file.file_name)?;
        let file_path = boards_dir.join(&file_name);
        expected_paths.insert(file_path.clone());
        fs::write(&file_path, board_file.text)
            .await
            .map_err(|error| format!("写入看板文件失败 `{}`: {error}", file_path.display()))?;
    }

    for existing_path in list_project_board_file_paths(&boards_dir)? {
        if expected_paths.contains(&existing_path) {
            continue;
        }

        fs::remove_file(&existing_path).await.map_err(|error| {
            format!("删除旧看板文件失败 `{}`: {error}", existing_path.display())
        })?;
    }

    resolve_project_workspace_storage(&app, workspace_path.as_deref())
}

#[tauri::command]
pub(crate) async fn save_flowgram_export_file(
    app: AppHandle,
    workspace_path: Option<String>,
    file_name: String,
    text: Option<String>,
    bytes: Option<Vec<u8>>,
) -> Result<SavedWorkspaceFile, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    let export_dir = workspace_dir.join(PROJECT_EXPORTS_DIR);
    let sanitized_file_name = sanitize_export_file_name(&file_name)?;
    let target_path = build_nonconflicting_file_path(&export_dir, &sanitized_file_name);

    fs::create_dir_all(&export_dir)
        .await
        .map_err(|error| format!("创建导出目录失败: {error}"))?;

    match (text, bytes) {
        (Some(text), None) => {
            if text.len() > MAX_EXPORT_FILE_BYTES {
                return Err("导出文件超过最大允许大小（25 MB）".to_owned());
            }
            fs::write(&target_path, text).await.map_err(|error| {
                format!("写入导出文件失败 `{}`: {error}", target_path.display())
            })?;
        }
        (None, Some(bytes)) => {
            if bytes.len() > MAX_EXPORT_FILE_BYTES {
                return Err("导出文件超过最大允许大小（25 MB）".to_owned());
            }
            fs::write(&target_path, bytes).await.map_err(|error| {
                format!("写入导出文件失败 `{}`: {error}", target_path.display())
            })?;
        }
        (None, None) => {
            return Err("导出内容不能为空。".to_owned());
        }
        (Some(_), Some(_)) => {
            return Err("导出内容不能同时包含文本和二进制。".to_owned());
        }
    }

    Ok(SavedWorkspaceFile {
        file_path: target_path.to_string_lossy().to_string(),
    })
}

fn resolve_project_workspace_storage(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<ProjectWorkspaceStorageInfo, String> {
    let (workspace_dir, using_default_location) =
        resolve_project_workspace_dir(app, workspace_path)?;
    let boards_directory_path = workspace_dir.join(PROJECT_BOARDS_DIR);

    Ok(ProjectWorkspaceStorageInfo {
        workspace_path: workspace_dir.to_string_lossy().to_string(),
        boards_directory_path: boards_directory_path.to_string_lossy().to_string(),
        using_default_location,
        board_file_count: count_project_board_files(&boards_directory_path)?,
    })
}

fn sanitize_project_board_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err("看板文件名不能为空。".to_owned());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(format!("看板文件名不允许包含路径分隔符: {trimmed}"));
    }
    if !trimmed.ends_with(PROJECT_BOARD_FILE_SUFFIX) {
        return Err(format!(
            "看板文件名必须以 `{PROJECT_BOARD_FILE_SUFFIX}` 结尾: {trimmed}"
        ));
    }
    Ok(trimmed.to_owned())
}

fn sanitize_export_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err("导出文件名不能为空。".to_owned());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(format!("导出文件名不允许包含路径分隔符: {trimmed}"));
    }
    Ok(trimmed.to_owned())
}

fn build_nonconflicting_file_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("flowgram-export");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    let mut index = 2usize;
    loop {
        let next_name = if ext.is_empty() {
            format!("{stem}-{index}")
        } else {
            format!("{stem}-{index}.{ext}")
        };
        let next_path = dir.join(next_name);
        if !next_path.exists() {
            return next_path;
        }
        index += 1;
    }
}

fn count_project_board_files(boards_dir: &Path) -> Result<usize, String> {
    if !boards_dir.exists() {
        return Ok(0);
    }

    let entries = std::fs::read_dir(boards_dir)
        .map_err(|error| format!("读取看板目录失败 `{}`: {error}", boards_dir.display()))?;
    let mut count = 0usize;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取看板目录条目失败 `{}`: {error}", boards_dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(PROJECT_BOARD_FILE_SUFFIX))
        {
            count += 1;
        }
    }

    Ok(count)
}

fn list_project_board_file_paths(boards_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !boards_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(boards_dir)
        .map_err(|error| format!("读取看板目录失败 `{}`: {error}", boards_dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取看板目录条目失败 `{}`: {error}", boards_dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(PROJECT_BOARD_FILE_SUFFIX))
        {
            paths.push(path);
        }
    }

    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(paths)
}
