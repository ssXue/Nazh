import { formatWorkflowGraph } from './flowgram';
import { parseWorkflowGraph } from './graph';
import {
  CURRENT_USER_NAME,
  MAX_PROJECT_SNAPSHOTS,
  PROJECT_LIBRARY_KIND,
  PROJECT_SCHEMA_VERSION,
  cloneJson,
  createId,
  nowIso,
  slugify,
  type ProjectEnvironment,
  type ProjectEnvironmentDiff,
  type ProjectLibraryState,
  type ProjectRecord,
  type ProjectSnapshot,
  type ProjectSnapshotReason,
  type WorkflowGraph,
  type WorkflowNodeDefinition,
  type JsonValue,
} from './project-types';

export function formatSnapshotReason(reason: ProjectSnapshotReason): string {
  switch (reason) {
    case 'seed':
      return '模板初始化';
    case 'manual':
      return '手动快照';
    case 'import':
      return '导入';
    case 'migration':
      return '迁移';
    case 'rollback':
      return '回滚前保护';
  }
}

export function createEmptyWorkflowGraph(name: string): WorkflowGraph {
  return {
    name,
    connections: [],
    nodes: {},
    edges: [],
  };
}

export function createNode(
  id: string,
  type: string,
  x: number,
  y: number,
  config: JsonValue,
  extras: Partial<WorkflowNodeDefinition> = {},
): WorkflowNodeDefinition {
  return {
    id,
    type,
    config,
    meta: {
      position: { x, y },
    },
    ...extras,
  };
}

export function buildStarterWorkflow(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [],
    nodes: {
      timer_trigger: createNode('timer_trigger', 'timer', 64, 116, {
        interval_ms: 3000,
        immediate: true,
      }),
      debug_console: createNode(
        'debug_console',
        'debugConsole',
        368,
        116,
        {
          label: 'starter',
          pretty: true,
        },
      ),
    },
    edges: [{ from: 'timer_trigger', to: 'debug_console' }],
  };
}

export function createEnvironment(
  name: string,
  description: string,
  diff: ProjectEnvironmentDiff = {},
): ProjectEnvironment {
  return {
    id: createId('env'),
    name,
    description,
    updatedAt: nowIso(),
    diff: cloneJson(diff),
  };
}

export function stripGraphConnectionDefinitions(graph: WorkflowGraph): WorkflowGraph {
  return {
    ...cloneJson(graph),
    connections: [],
  };
}

export function normalizeProjectAstText(astText: string): string {
  const parsed = parseWorkflowGraph(astText);
  return parsed.graph
    ? formatWorkflowGraph(stripGraphConnectionDefinitions(parsed.graph))
    : astText;
}

export function createSnapshot(
  project: Pick<
    ProjectRecord,
    'name' | 'description' | 'astText' | 'payloadText' | 'activeEnvironmentId' | 'environments'
  >,
  reason: ProjectSnapshotReason,
  label?: string,
  description?: string,
): ProjectSnapshot {
  const createdAt = nowIso();

  return {
    id: createId('snapshot'),
    label: label ?? `${formatSnapshotReason(reason)} · ${project.name}`,
    description: description ?? project.description,
    createdAt,
    reason,
    astText: normalizeProjectAstText(project.astText),
    payloadText: project.payloadText,
    activeEnvironmentId: project.activeEnvironmentId,
    environments: cloneJson(project.environments),
  };
}

export function ensureSnapshotLimit(snapshots: ProjectSnapshot[]): ProjectSnapshot[] {
  return snapshots
    .slice()
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
    .slice(0, MAX_PROJECT_SNAPSHOTS);
}

function buildSeedProjects(): ProjectRecord[] {
  return [];
}

export function buildDefaultConnectionDefinitions() {
  return [];
}

export function buildDefaultProjectLibrary(): ProjectLibraryState {
  return {
    kind: PROJECT_LIBRARY_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    projects: buildSeedProjects(),
  };
}

export function createNewProjectRecord(name: string, description?: string, empty?: boolean): ProjectRecord {
  const projectName = name.trim() || '未命名工程';
  const graph = empty ? { name: projectName, connections: [], nodes: {}, edges: [] } : buildStarterWorkflow(projectName);
  const astText = formatWorkflowGraph(stripGraphConnectionDefinitions(graph));
  const createdAt = nowIso();
  const environments = [createEnvironment('生产环境', '默认环境。')];
  const project: ProjectRecord = {
    id: slugify(projectName),
    name: projectName,
    description: description?.trim() || '新的工作流工程',
    createdAt,
    updatedAt: createdAt,
    astText,
    payloadText: JSON.stringify(
      {
        manual: true,
        created_by: CURRENT_USER_NAME,
      },
      null,
      2,
    ),
    activeEnvironmentId: environments[0].id,
    environments,
    snapshots: [],
    migrationNotes: [],
  };

  return {
    ...project,
    snapshots: [createSnapshot(project, 'seed', '初始版本', '创建工程时生成的首个版本。')],
  };
}
