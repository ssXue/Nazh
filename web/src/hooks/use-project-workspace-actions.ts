import type { RefObject } from 'react';

import type { BoardWorkspaceHandle } from '../components/app/BoardWorkspace';
import type { BoardItem } from '../components/app/BoardsPanel';
import type { UseProjectLibraryResult } from './use-project-library';
import type { UseWorkflowEngineResult } from './use-workflow-engine';
import { formatWorkflowGraph } from '../lib/flowgram';
import { parseWorkflowGraph } from '../lib/graph';
import type { ProjectEnvironmentDiff, ProjectRecord } from '../lib/projects';
import { stripWorkflowNodeLocalAiConfig } from '../lib/workflow-ai';
import { describeUnknownError } from '../lib/workflow-events';

function getDeployProjectId(
  deployInfo: { projectId?: string | null; workflowId?: string | null } | null,
) {
  if (!deployInfo) {
    return null;
  }

  return deployInfo.projectId?.trim() || deployInfo.workflowId?.trim() || null;
}

interface UseProjectWorkspaceActionsOptions {
  activeBoardId: string | null;
  activeProject: ProjectRecord | null;
  clearActiveBoard: () => void;
  engine: UseWorkflowEngineResult;
  flowgramCanvasRef: RefObject<BoardWorkspaceHandle | null>;
  openBoard: (boardId: string) => void;
  projectLibrary: UseProjectLibraryResult;
  setSidebarCollapsed: React.Dispatch<React.SetStateAction<boolean>>;
}

export function useProjectWorkspaceActions({
  activeBoardId,
  activeProject,
  clearActiveBoard,
  engine,
  flowgramCanvasRef,
  openBoard,
  projectLibrary,
  setSidebarCollapsed,
}: UseProjectWorkspaceActionsOptions) {
  function updateProjectDraft(
    projectId: string,
    nextDraft: Partial<Pick<ProjectRecord, 'astText' | 'payloadText' | 'name' | 'description'>>,
  ) {
    projectLibrary.updateProjectDraft(projectId, nextDraft);
  }

  function buildProjectDraftSnapshot(projectId: string) {
    const project = projectLibrary.projects.find((item) => item.id === projectId);
    if (!project) {
      return {
        graph: null,
        astText: null,
        error: '当前工程不存在。',
      };
    }

    const sourceGraphState = parseWorkflowGraph(project.astText);
    if (sourceGraphState.error && !flowgramCanvasRef.current?.getCurrentWorkflowGraph()) {
      return {
        graph: null,
        astText: null,
        error: sourceGraphState.error,
      };
    }

    const currentGraph =
      project.id === activeBoardId
        ? flowgramCanvasRef.current?.getCurrentWorkflowGraph() ?? sourceGraphState.graph
        : sourceGraphState.graph;
    if (!currentGraph) {
      return {
        graph: null,
        astText: null,
        error: '当前没有可执行的工作流。',
      };
    }

    const nextAstText = formatWorkflowGraph(stripWorkflowNodeLocalAiConfig(currentGraph));
    const nextGraphState = parseWorkflowGraph(nextAstText);
    if (nextGraphState.error || !nextGraphState.graph) {
      return {
        graph: null,
        astText: null,
        error: nextGraphState.error ?? '当前工作流快照无法序列化。',
      };
    }

    return {
      graph: nextGraphState.graph,
      astText: nextAstText,
      error: null,
    };
  }

  function applyStructuredGraphChange(nextAstText: string, nextStatusMessage: string) {
    if (!activeProject || nextAstText === activeProject.astText) {
      return;
    }

    updateProjectDraft(activeProject.id, { astText: nextAstText });
    engine.setStatusMessage(nextStatusMessage);
  }

  function handleGraphChange(nextAstText: string) {
    applyStructuredGraphChange(nextAstText, '画布变更已同步回项目草稿。');
  }

  function handlePayloadTextChange(nextText: string) {
    if (!activeProject) {
      return;
    }

    updateProjectDraft(activeProject.id, { payloadText: nextText });
  }

  function handleOpenBoard(board: BoardItem) {
    openBoard(board.id);
    setSidebarCollapsed(true);

    if (getDeployProjectId(engine.deployInfo) === board.id) {
      engine.setStatusMessage(`已进入工程 ${board.name}，已保留当前运行态。`);
      return;
    }

    engine.resetWorkspaceRuntime(`已进入工程 ${board.name}。`);
  }

  function handleBackToBoards() {
    clearActiveBoard();
    setSidebarCollapsed(false);
    engine.resetWorkspaceRuntime('已返回所有看板。');
  }

  function handleCreateBoard() {
    const nextProject = projectLibrary.createProject();
    openBoard(nextProject.id);
    setSidebarCollapsed(true);
    engine.resetWorkspaceRuntime(`已创建工程 ${nextProject.name}。`);
    engine.appendRuntimeLog('project', 'success', '已创建工程', nextProject.name);
  }

  async function handleImportBoardFile(file: File) {
    try {
      const sourceText = await file.text();
      const result = projectLibrary.importProjects(sourceText);
      const nextProject = result.importedProjects[0] ?? null;

      if (nextProject) {
        openBoard(nextProject.id);
      }

      const detail = result.migrationNotes.length > 0 ? result.migrationNotes.join('\n') : null;
      engine.resetWorkspaceRuntime(
        nextProject
          ? `已导入工程 ${nextProject.name}。`
          : `已导入 ${result.importedProjects.length} 个工程。`,
      );
      engine.appendRuntimeLog('project', 'success', '工程导入完成', detail);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '导入工程失败', detail ?? message);
      engine.setStatusMessage(message);
    }
  }

  function handleDeleteBoard(board: BoardItem) {
    const deletedProject = projectLibrary.deleteProject(board.id);
    if (!deletedProject) {
      engine.setStatusMessage('删除失败：当前工程不存在。');
      return;
    }

    if (activeBoardId === board.id) {
      clearActiveBoard();
    }

    engine.setStatusMessage(`已删除工程 ${deletedProject.name}。`);
    engine.appendRuntimeLog('project', 'warn', '已删除工程', deletedProject.name);
  }

  function handleCreateSnapshot() {
    if (!activeProject) {
      return;
    }

    const draftSnapshot = buildProjectDraftSnapshot(activeProject.id);
    if (draftSnapshot.error || !draftSnapshot.astText) {
      engine.appendAppError('command', '创建快照失败', draftSnapshot.error ?? '未知错误');
      engine.setStatusMessage(draftSnapshot.error ?? '创建快照失败。');
      return;
    }

    projectLibrary.saveProject(activeProject.id, {
      astText: draftSnapshot.astText,
      payloadText: activeProject.payloadText,
    });
    const nextProject = projectLibrary.createSnapshot(activeProject.id);
    engine.setStatusMessage(`已为 ${activeProject.name} 创建版本快照。`);
    engine.appendRuntimeLog(
      'project',
      'info',
      '已创建版本快照',
      nextProject ? `${nextProject.snapshots.length} 个版本` : activeProject.name,
    );
  }

  function handleRollbackSnapshot(snapshotId: string) {
    if (!activeProject) {
      return;
    }

    const nextProject = projectLibrary.rollbackProject(activeProject.id, snapshotId);
    engine.setStatusMessage(`已回滚工程 ${activeProject.name}。`);
    engine.appendRuntimeLog(
      'project',
      'warn',
      '已回滚工程版本',
      nextProject?.snapshots[0]?.label ?? activeProject.name,
    );
  }

  function handleDeleteSnapshot(snapshotId: string) {
    if (!activeProject) {
      return;
    }

    const nextProject = projectLibrary.deleteSnapshot(activeProject.id, snapshotId);
    engine.setStatusMessage(`已删除 ${activeProject.name} 的版本快照。`);
    engine.appendRuntimeLog(
      'project',
      'info',
      '已删除版本快照',
      nextProject ? `剩余 ${nextProject.snapshots.length} 个版本` : activeProject.name,
    );
  }

  function handleEnvironmentChange(environmentId: string) {
    if (!activeProject) {
      return;
    }

    const nextEnvironment = activeProject.environments.find(
      (environment) => environment.id === environmentId,
    );
    projectLibrary.setActiveEnvironment(activeProject.id, environmentId);
    engine.setStatusMessage(`已切换到环境 ${nextEnvironment?.name ?? '未命名环境'}。`);
    engine.appendRuntimeLog(
      'project',
      'info',
      '已切换运行环境',
      nextEnvironment?.name ?? environmentId,
    );
  }

  function handleEnvironmentSave(
    environmentId: string,
    patch: { name: string; description: string; diff: ProjectEnvironmentDiff },
  ) {
    if (!activeProject) {
      return;
    }

    projectLibrary.updateEnvironment(activeProject.id, environmentId, {
      name: patch.name,
      description: patch.description,
      diff: patch.diff,
    });
    engine.setStatusMessage(`已更新环境配置 ${patch.name}。`);
    engine.appendRuntimeLog('project', 'success', '环境差异配置已更新', patch.name);
  }

  function handleDuplicateEnvironment(environmentId: string) {
    if (!activeProject) {
      return;
    }

    const nextEnvironment = projectLibrary.duplicateEnvironment(activeProject.id, environmentId);
    if (!nextEnvironment) {
      return;
    }

    engine.setStatusMessage(`已派生环境 ${nextEnvironment.name}。`);
    engine.appendRuntimeLog('project', 'info', '已派生环境', nextEnvironment.name);
  }

  function handleDeleteEnvironment(environmentId: string) {
    if (!activeProject) {
      return;
    }

    projectLibrary.deleteEnvironment(activeProject.id, environmentId);
    engine.setStatusMessage('已删除环境配置。');
    engine.appendRuntimeLog('project', 'warn', '已删除环境配置');
  }

  return {
    buildProjectDraftSnapshot,
    handleBackToBoards,
    handleCreateBoard,
    handleCreateSnapshot,
    handleDeleteBoard,
    handleDeleteEnvironment,
    handleDeleteSnapshot,
    handleDuplicateEnvironment,
    handleEnvironmentChange,
    handleEnvironmentSave,
    handleGraphChange,
    handleImportBoardFile,
    handleOpenBoard,
    handlePayloadTextChange,
    handleRollbackSnapshot,
    updateProjectDraft,
  };
}
