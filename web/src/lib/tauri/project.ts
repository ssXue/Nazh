import { invoke } from '@tauri-apps/api/core';

import type {
  PersistedDeploymentSession,
  PersistedDeploymentSessionState,
} from '../deployment-session';

export interface ProjectWorkspaceStorageInfo {
  workspacePath: string;
  boardsDirectoryPath: string;
  usingDefaultLocation: boolean;
  boardFileCount: number;
}

export interface ProjectWorkspaceBoardFile {
  fileName: string;
  text: string;
}

export interface ProjectWorkspaceLoadResult {
  storage: ProjectWorkspaceStorageInfo;
  boardFiles: ProjectWorkspaceBoardFile[];
}

export interface SavedWorkspaceFile {
  filePath: string;
}

export async function loadProjectBoardFiles(
  workspacePath: string,
): Promise<ProjectWorkspaceLoadResult> {
  return invoke<ProjectWorkspaceLoadResult>('load_project_board_files', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveProjectBoardFiles(
  workspacePath: string,
  boardFiles: ProjectWorkspaceBoardFile[],
): Promise<ProjectWorkspaceStorageInfo> {
  return invoke<ProjectWorkspaceStorageInfo>('save_project_board_files', {
    workspacePath: workspacePath.trim() || null,
    boardFiles,
  });
}

export async function saveFlowgramExportFile(
  workspacePath: string,
  fileName: string,
  payload: {
    text?: string;
    bytes?: number[];
  },
): Promise<SavedWorkspaceFile> {
  return invoke<SavedWorkspaceFile>('save_flowgram_export_file', {
    workspacePath: workspacePath.trim() || null,
    fileName,
    text: payload.text ?? null,
    bytes: payload.bytes ?? null,
  });
}

export async function loadDeploymentSessionFile(
  workspacePath: string,
): Promise<PersistedDeploymentSession | null> {
  return invoke<PersistedDeploymentSession | null>('load_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function loadDeploymentSessionStateFile(
  workspacePath: string,
): Promise<PersistedDeploymentSessionState> {
  return invoke<PersistedDeploymentSessionState>('load_deployment_session_state_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function listDeploymentSessionsFile(
  workspacePath: string,
): Promise<PersistedDeploymentSession[]> {
  return invoke<PersistedDeploymentSession[]>('list_deployment_sessions_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveDeploymentSessionFile(
  workspacePath: string,
  session: PersistedDeploymentSession,
  activeProjectId?: string | null,
): Promise<void> {
  return invoke<void>('save_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
    session,
    activeProjectId: activeProjectId === undefined ? null : activeProjectId,
  });
}

export async function setDeploymentSessionActiveProjectFile(
  workspacePath: string,
  projectId: string | null,
): Promise<void> {
  return invoke<void>('set_deployment_session_active_project_file', {
    workspacePath: workspacePath.trim() || null,
    projectId: projectId?.trim() ? projectId.trim() : null,
  });
}

export async function removeDeploymentSessionFile(
  workspacePath: string,
  projectId: string,
): Promise<void> {
  return invoke<void>('remove_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
    projectId: projectId.trim(),
  });
}

export async function clearDeploymentSessionFile(workspacePath: string): Promise<void> {
  return invoke<void>('clear_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
  });
}
