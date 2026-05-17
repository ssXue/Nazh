import { parseWorkflowGraph } from './graph';
import {
  createSnapshot,
  ensureSnapshotLimit,
  normalizeProjectAstText,
  buildDefaultProjectLibrary,
} from './project-factories';
import {
  PROJECT_BOARD_KIND,
  PROJECT_LIBRARY_KIND,
  PROJECT_LIBRARY_STORAGE_KEY,
  PROJECT_SCHEMA_VERSION,
  cloneJson,
  deepMergeJson,
  isRecord,
  nowIso,
  type ConnectionDefinition,
  type JsonValue,
  type ProjectEnvironment,
  type ProjectEnvironmentDiff,
  type ProjectLibraryState,
  type ProjectRecord,
  type WorkflowGraph,
} from './project-types';
import {
  ensureUniqueProjectId,
  normalizeProjectRecord,
} from './project-json';
import {
  buildProjectBoardFileName,
  serializeProjectRecordToBoardFile,
} from './project-export';

// Re-export all public types and constants
export {
  CURRENT_USER_NAME,
  PROJECT_LIBRARY_STORAGE_KEY,
  PROJECT_PACKAGE_KIND,
  PROJECT_LIBRARY_KIND,
  PROJECT_BOARD_KIND,
  PROJECT_SCHEMA_VERSION,
  MAX_PROJECT_SNAPSHOTS,
  PROJECT_BOARD_FILE_SUFFIX,
  type ProjectEnvironmentDiff,
  type ProjectEnvironment,
  type ProjectSnapshotReason,
  type ProjectSnapshot,
  type ProjectRecord,
  type ProjectLibraryState,
  type ProjectPackage,
  type ProjectBoardFileText,
  type ProjectBoardSnapshotFile,
  type ProjectBoardFile,
  type ImportProjectsResult,
} from './project-types';

export {
  buildDefaultConnectionDefinitions,
  buildDefaultProjectLibrary,
  createNewProjectRecord,
} from './project-factories';

export {
  serializeProjectRecordToBoardFile,
  buildProjectBoardFileName,
  serializeProjectLibraryToBoardFiles,
  parseProjectBoardFileText,
  parseProjectBoardFiles,
  importProjectsFromText,
  prepareProjectExport,
  mergeImportedProjects,
} from './project-export';

// ---- 以下为直接定义的公共函数 ----

export function formatRelativeTimestamp(timestamp: string): string {
  const target = new Date(timestamp).getTime();
  if (Number.isNaN(target)) {
    return '未知时间';
  }

  const diff = Date.now() - target;
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;

  if (diff < minute) {
    return '刚刚';
  }

  if (diff < hour) {
    return `${Math.max(1, Math.floor(diff / minute))} 分钟前`;
  }

  if (diff < day) {
    return `${Math.max(1, Math.floor(diff / hour))} 小时前`;
  }

  if (diff < 7 * day) {
    return `${Math.max(1, Math.floor(diff / day))} 天前`;
  }

  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(target);
}

export function parseProjectNodeCount(astText: string): number {
  const parsed = parseWorkflowGraph(astText);
  return parsed.graph ? Object.keys(parsed.graph.nodes).length : 0;
}

export function loadProjectLibrary(): ProjectLibraryState {
  if (typeof window === 'undefined') {
    return buildDefaultProjectLibrary();
  }

  try {
    const raw = window.localStorage.getItem(PROJECT_LIBRARY_STORAGE_KEY);
    if (!raw) {
      return buildDefaultProjectLibrary();
    }
    return parseProjectLibraryText(raw);
  } catch {
    return buildDefaultProjectLibrary();
  }
}

export function parseProjectLibraryText(raw: string): ProjectLibraryState {
  const parsed = JSON.parse(raw) as unknown;
  if (!isRecord(parsed) || !Array.isArray(parsed.projects)) {
    throw new Error('工程库文件格式无效。');
  }

  const projects = parsed.projects.map((item, index) =>
    normalizeProjectRecord(item, `工程 ${index + 1}`),
  );

  return {
    kind: PROJECT_LIBRARY_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    projects,
  };
}

export function persistProjectLibrary(library: ProjectLibraryState) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(PROJECT_LIBRARY_STORAGE_KEY, JSON.stringify(library));
  } catch {
    // 忽略存储失败。
  }
}

export function getActiveEnvironment(project: ProjectRecord): ProjectEnvironment | null {
  return (
    project.environments.find((environment) => environment.id === project.activeEnvironmentId) ??
    project.environments[0] ??
    null
  );
}

export function applyEnvironmentToGraph(
  graph: WorkflowGraph,
  environment: ProjectEnvironment | null,
): WorkflowGraph {
  if (!environment) {
    return cloneJson(graph);
  }

  const nextGraph = cloneJson(graph);
  const nodeDiffs = environment.diff.nodeConfigs ?? {};

  Object.entries(nodeDiffs).forEach(([nodeId, override]) => {
    const targetNode = nextGraph.nodes[nodeId];
    if (!targetNode) {
      return;
    }

    targetNode.config = deepMergeJson(targetNode.config ?? {}, override);
  });

  return nextGraph;
}

export function applyEnvironmentToConnectionDefinitions(
  definitions: ConnectionDefinition[],
  environment: ProjectEnvironment | null,
): ConnectionDefinition[] {
  if (!environment) {
    return cloneJson(definitions);
  }

  const connectionDiffs = environment.diff.connections ?? {};
  return definitions.map((definition) => {
    const override = connectionDiffs[definition.id];
    if (!override) {
      return cloneJson(definition);
    }

    return {
      ...cloneJson(definition),
      metadata: deepMergeJson(definition.metadata, override),
    };
  });
}

export function renameProjectRecord(
  project: ProjectRecord,
  patch: Partial<Pick<ProjectRecord, 'name' | 'description' | 'astText' | 'payloadText'>>,
): ProjectRecord {
  const normalizedAstText =
    typeof patch.astText === 'string' ? normalizeProjectAstText(patch.astText) : patch.astText;

  return {
    ...project,
    ...patch,
    ...(normalizedAstText === undefined ? {} : { astText: normalizedAstText }),
    updatedAt: nowIso(),
  };
}

export function createProjectSnapshot(
  project: ProjectRecord,
  label?: string,
  description?: string,
): ProjectRecord {
  return {
    ...project,
    updatedAt: nowIso(),
    snapshots: ensureSnapshotLimit([
      createSnapshot(project, 'manual', label, description),
      ...project.snapshots,
    ]),
  };
}

export function deleteProjectSnapshot(
  project: ProjectRecord,
  snapshotId: string,
): ProjectRecord {
  return {
    ...project,
    updatedAt: nowIso(),
    snapshots: project.snapshots.filter((snapshot) => snapshot.id !== snapshotId),
  };
}

export function rollbackProjectToSnapshot(
  project: ProjectRecord,
  snapshotId: string,
): ProjectRecord {
  const target = project.snapshots.find((snapshot) => snapshot.id === snapshotId);
  if (!target) {
    return project;
  }

  const rollbackProtection = createSnapshot(
    project,
    'rollback',
    `回滚前 · ${project.name}`,
    `回滚到 ${target.label} 之前自动保留的版本。`,
  );

  return {
    ...project,
    astText: target.astText,
    payloadText: target.payloadText,
    activeEnvironmentId: target.activeEnvironmentId,
    environments: cloneJson(target.environments),
    updatedAt: nowIso(),
    snapshots: ensureSnapshotLimit([rollbackProtection, ...project.snapshots]),
  };
}

export { upsertProjectRecord } from './project-export';
