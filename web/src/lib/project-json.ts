import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

import { formatWorkflowGraph, toFlowgramWorkflowJson, toNazhWorkflowGraph } from './flowgram';
import { parseWorkflowGraph } from './graph';
import {
  createEmptyWorkflowGraph,
  createEnvironment,
  createSnapshot,
  ensureSnapshotLimit,
  normalizeProjectAstText,
  stripGraphConnectionDefinitions,
  buildStarterWorkflow,
} from './project-factories';
import {
  PROJECT_BOARD_KIND,
  asJsonObject,
  cloneJson,
  createId,
  isRecord,
  normalizeString,
  nowIso,
  slugify,
  type ProjectEnvironment,
  type ProjectRecord,
  type ProjectSnapshot,
  type ProjectSnapshotReason,
  type WorkflowGraph,
} from './project-types';

export function isFlowgramWorkflowJsonLike(value: unknown): value is FlowgramWorkflowJSON {
  return isRecord(value) && Array.isArray(value.nodes) && Array.isArray(value.edges);
}

export function isWorkflowGraphLike(value: unknown): value is WorkflowGraph {
  return isRecord(value) && isRecord(value.nodes) && Array.isArray(value.edges);
}

export function normalizeFlowgramWorkflowJson(
  value: unknown,
  fallbackName: string,
): FlowgramWorkflowJSON {
  if (isFlowgramWorkflowJsonLike(value)) {
    return {
      nodes: cloneJson(value.nodes),
      edges: cloneJson(value.edges),
    };
  }

  if (isRecord(value) && isFlowgramWorkflowJsonLike(value.workflow)) {
    return {
      nodes: cloneJson(value.workflow.nodes),
      edges: cloneJson(value.workflow.edges),
    };
  }

  const astTextCandidate =
    isRecord(value) && typeof value.astText === 'string'
      ? value.astText
      : isRecord(value) && typeof value.workflowAst === 'string'
        ? value.workflowAst
        : null;

  if (astTextCandidate) {
    const parsed = parseWorkflowGraph(astTextCandidate);
    if (parsed.graph) {
      return toFlowgramWorkflowJson(parsed.graph);
    }
  }

  return toFlowgramWorkflowJson(buildStarterWorkflow(fallbackName));
}

export function formatFlowgramWorkflowAstText(
  flowgramWorkflow: FlowgramWorkflowJSON,
  boardName: string,
): string {
  const graph = toNazhWorkflowGraph(flowgramWorkflow, createEmptyWorkflowGraph(boardName));
  return normalizeProjectAstText(formatWorkflowGraph(graph));
}

export function buildProjectFlowgramWorkflow(
  astText: string,
  fallbackName: string,
): FlowgramWorkflowJSON {
  const parsed = parseWorkflowGraph(astText);
  if (parsed.graph) {
    return toFlowgramWorkflowJson(parsed.graph);
  }

  return toFlowgramWorkflowJson(buildStarterWorkflow(fallbackName));
}

export function normalizeEnvironmentDiff(value: unknown) {
  if (!isRecord(value)) {
    return {};
  }

  return {
    connections: isRecord(value.connections) ? asJsonObject(value.connections) : {},
    nodeConfigs: isRecord(value.nodeConfigs)
      ? asJsonObject(value.nodeConfigs)
      : isRecord(value.node_configs)
        ? asJsonObject(value.node_configs)
        : {},
  };
}

export function normalizeEnvironment(value: unknown, index: number): ProjectEnvironment {
  const source = isRecord(value) ? value : {};

  return {
    id: normalizeString(source.id, `env-${index + 1}`),
    name: normalizeString(source.name, `环境 ${index + 1}`),
    description: typeof source.description === 'string' ? source.description : '',
    updatedAt: normalizeString(source.updatedAt, nowIso()),
    diff: normalizeEnvironmentDiff(source.diff),
  };
}

export function normalizeProjectSnapshotReason(value: unknown): ProjectSnapshotReason {
  return value === 'seed' ||
    value === 'manual' ||
    value === 'import' ||
    value === 'migration' ||
    value === 'rollback'
    ? value
    : 'manual';
}

export function normalizeBoardSnapshot(
  value: unknown,
  project: ProjectRecord,
  index: number,
): ProjectSnapshot {
  const source = isRecord(value) ? value : {};
  const environments = Array.isArray(source.environments)
    ? source.environments.map((item, itemIndex) => normalizeEnvironment(item, itemIndex))
    : cloneJson(project.environments);
  const activeEnvironmentId = normalizeString(
    source.activeEnvironmentId,
    environments[0]?.id ?? project.activeEnvironmentId,
  );
  const flowgramWorkflow = normalizeFlowgramWorkflowJson(source, project.name);

  return {
    id: normalizeString(source.id, `snapshot-${index + 1}`),
    label: normalizeString(source.label, `快照 ${index + 1}`),
    description: typeof source.description === 'string' ? source.description : project.description,
    createdAt: normalizeString(source.createdAt, nowIso()),
    reason: normalizeProjectSnapshotReason(source.reason),
    astText: formatFlowgramWorkflowAstText(flowgramWorkflow, project.name),
    payloadText: normalizeString(source.payloadText, project.payloadText),
    activeEnvironmentId,
    environments,
  };
}

export function normalizeSnapshot(value: unknown, project: ProjectRecord, index: number): ProjectSnapshot {
  const source = isRecord(value) ? value : {};
  const environments = Array.isArray(source.environments)
    ? source.environments.map((item, itemIndex) => normalizeEnvironment(item, itemIndex))
    : cloneJson(project.environments);
  const activeEnvironmentId = normalizeString(
    source.activeEnvironmentId,
    environments[0]?.id ?? project.activeEnvironmentId,
  );

  return {
    id: normalizeString(source.id, `snapshot-${index + 1}`),
    label: normalizeString(source.label, `快照 ${index + 1}`),
    description: typeof source.description === 'string' ? source.description : project.description,
    createdAt: normalizeString(source.createdAt, nowIso()),
    reason: normalizeProjectSnapshotReason(source.reason),
    astText: normalizeProjectAstText(normalizeString(source.astText, project.astText)),
    payloadText: normalizeString(source.payloadText, project.payloadText),
    activeEnvironmentId,
    environments,
  };
}

export function normalizeProjectRecordFromBoardFile(
  value: unknown,
  fallbackName: string,
  migrationNotes: string[] = [],
): ProjectRecord {
  const source = isRecord(value) ? value : {};
  const projectName = normalizeString(source.name, fallbackName);
  const flowgramWorkflow = normalizeFlowgramWorkflowJson(source, projectName);
  const createdAt = normalizeString(source.createdAt, nowIso());
  const updatedAt = normalizeString(source.updatedAt, createdAt);
  const environments = Array.isArray(source.environments)
    ? source.environments.map((item, index) => normalizeEnvironment(item, index))
    : [createEnvironment('生产环境', '默认环境。')];
  const activeEnvironmentId = normalizeString(
    source.activeEnvironmentId,
    environments[0]?.id ?? '',
  );

  const project: ProjectRecord = {
    id: normalizeString(source.id, createId('project')),
    name: projectName,
    description:
      typeof source.description === 'string' ? source.description : '从看板文件结构迁移而来。',
    createdAt,
    updatedAt,
    astText: formatFlowgramWorkflowAstText(flowgramWorkflow, projectName),
    payloadText:
      typeof source.payloadText === 'string'
        ? source.payloadText
        : JSON.stringify({ manual: true }, null, 2),
    activeEnvironmentId,
    environments,
    snapshots: [],
    migrationNotes: [
      ...migrationNotes,
      ...(Array.isArray(source.migrationNotes)
        ? source.migrationNotes.filter((item): item is string => typeof item === 'string')
        : []),
    ],
  };

  const snapshots = Array.isArray(source.snapshots)
    ? source.snapshots.map((item, index) => normalizeBoardSnapshot(item, project, index))
    : [];

  return {
    ...project,
    snapshots:
      snapshots.length > 0
        ? ensureSnapshotLimit(snapshots)
        : [createSnapshot(project, project.migrationNotes.length > 0 ? 'migration' : 'seed')],
  };
}

export function normalizeProjectRecord(
  value: unknown,
  fallbackName: string,
  migrationNotes: string[] = [],
): ProjectRecord {
  const source = isRecord(value) ? value : {};
  const astTextCandidate =
    typeof source.astText === 'string'
      ? source.astText
      : typeof source.workflowAst === 'string'
        ? source.workflowAst
        : formatWorkflowGraph(buildStarterWorkflow(fallbackName));
  const parsedGraph = parseWorkflowGraph(astTextCandidate);
  const astText = parsedGraph.graph
    ? formatWorkflowGraph(stripGraphConnectionDefinitions(parsedGraph.graph))
    : formatWorkflowGraph(buildStarterWorkflow(fallbackName));
  const createdAt = normalizeString(source.createdAt, nowIso());
  const updatedAt = normalizeString(source.updatedAt, createdAt);
  const environments = Array.isArray(source.environments)
    ? source.environments.map((item, index) => normalizeEnvironment(item, index))
    : [createEnvironment('生产环境', '默认环境。')];
  const activeEnvironmentId = normalizeString(
    source.activeEnvironmentId,
    environments[0]?.id ?? '',
  );

  const project: ProjectRecord = {
    id: normalizeString(source.id, createId('project')),
    name: normalizeString(source.name, fallbackName),
    description:
      typeof source.description === 'string' ? source.description : '从旧版工程结构迁移而来。',
    createdAt,
    updatedAt,
    astText,
    payloadText:
      typeof source.payloadText === 'string'
        ? source.payloadText
        : JSON.stringify({ manual: true }, null, 2),
    activeEnvironmentId,
    environments,
    snapshots: [],
    migrationNotes: [
      ...migrationNotes,
      ...(Array.isArray(source.migrationNotes)
        ? source.migrationNotes.filter((item): item is string => typeof item === 'string')
        : []),
    ],
  };

  const snapshots = Array.isArray(source.snapshots)
    ? source.snapshots.map((item, index) => normalizeSnapshot(item, project, index))
    : [];

  return {
    ...project,
    snapshots:
      snapshots.length > 0
        ? ensureSnapshotLimit(snapshots)
        : [createSnapshot(project, project.migrationNotes.length > 0 ? 'migration' : 'seed')],
  };
}

export function ensureUniqueProjectId(projects: ProjectRecord[], preferredId: string): string {
  const normalizedId = slugify(preferredId);
  const existingIds = new Set(projects.map((project) => project.id));
  if (!existingIds.has(normalizedId)) {
    return normalizedId;
  }

  let suffix = 2;
  while (existingIds.has(`${normalizedId}-${suffix}`)) {
    suffix += 1;
  }

  return `${normalizedId}-${suffix}`;
}
