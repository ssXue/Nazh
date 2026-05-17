import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

import { formatWorkflowGraph, toNazhWorkflowGraph } from './flowgram';
import { parseWorkflowGraph } from './graph';
import { buildProjectFlowgramWorkflow, ensureUniqueProjectId, isFlowgramWorkflowJsonLike, isWorkflowGraphLike, normalizeProjectRecord, normalizeProjectRecordFromBoardFile } from './project-json';
import {
  createEmptyWorkflowGraph,
  createEnvironment,
  createSnapshot,
} from './project-factories';
import {
  PROJECT_BOARD_FILE_SUFFIX,
  PROJECT_BOARD_KIND,
  PROJECT_LIBRARY_KIND,
  PROJECT_PACKAGE_KIND,
  PROJECT_SCHEMA_VERSION,
  cloneJson,
  isRecord,
  normalizeString,
  nowIso,
  slugify,
  type ImportProjectsResult,
  type ProjectBoardFile,
  type ProjectBoardFileText,
  type ProjectBoardSnapshotFile,
  type ProjectLibraryState,
  type ProjectRecord,
  type ProjectSnapshot,
  type WorkflowGraph,
} from './project-types';

export function buildImportedProjectFromGraph(graph: WorkflowGraph, name?: string): ProjectRecord {
  const projectName = name?.trim() || graph.name?.trim() || '导入工程';
  const environments = [createEnvironment('生产环境', '导入后生成的默认环境。')];
  const createdAt = nowIso();
  const project: ProjectRecord = {
    id: slugify(projectName),
    name: projectName,
    description: '从裸工作流 AST 导入并迁移得到。',
    createdAt,
    updatedAt: createdAt,
    astText: formatWorkflowGraph(graph),
    payloadText: JSON.stringify({ imported: true }, null, 2),
    activeEnvironmentId: environments[0].id,
    environments,
    snapshots: [],
    migrationNotes: ['已从裸工作流 AST 迁移为 Nazh 工程包。'],
  };

  return {
    ...project,
    snapshots: [
      createSnapshot(project, 'import', '导入版本', '从裸工作流导入后创建的首个版本。'),
    ],
  };
}

export function buildImportedProjectFromFlowgramJson(
  workflow: FlowgramWorkflowJSON,
  name?: string,
): ProjectRecord {
  const projectName = name?.trim() || 'Flowgram 导入工程';
  const graph = toNazhWorkflowGraph(workflow, createEmptyWorkflowGraph(projectName));

  return {
    ...buildImportedProjectFromGraph(graph, projectName),
    description: '从 Flowgram 导出 JSON 导入并迁移得到。',
    migrationNotes: ['已从 Flowgram 导出 JSON 迁移为 Nazh 工程包。'],
  };
}

export function serializeProjectSnapshotToBoardFile(
  snapshot: ProjectSnapshot,
  boardName: string,
): ProjectBoardSnapshotFile {
  const flowgramWorkflow = buildProjectFlowgramWorkflow(snapshot.astText, boardName);

  return {
    id: snapshot.id,
    label: snapshot.label,
    description: snapshot.description,
    createdAt: snapshot.createdAt,
    reason: snapshot.reason,
    nodes: cloneJson(flowgramWorkflow.nodes),
    edges: cloneJson(flowgramWorkflow.edges),
    payloadText: snapshot.payloadText,
    activeEnvironmentId: snapshot.activeEnvironmentId,
    environments: cloneJson(snapshot.environments),
  };
}

export function serializeProjectRecordToBoardFile(project: ProjectRecord): ProjectBoardFile {
  const flowgramWorkflow = buildProjectFlowgramWorkflow(project.astText, project.name);

  return {
    kind: PROJECT_BOARD_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    id: project.id,
    name: project.name,
    description: project.description,
    createdAt: project.createdAt,
    updatedAt: project.updatedAt,
    nodes: cloneJson(flowgramWorkflow.nodes),
    edges: cloneJson(flowgramWorkflow.edges),
    payloadText: project.payloadText,
    activeEnvironmentId: project.activeEnvironmentId,
    environments: cloneJson(project.environments),
    snapshots: project.snapshots.map((snapshot) =>
      serializeProjectSnapshotToBoardFile(snapshot, project.name),
    ),
    migrationNotes: cloneJson(project.migrationNotes),
  };
}

export function buildProjectBoardFileName(project: Pick<ProjectRecord, 'id' | 'name'>): string {
  return `${slugify(project.name)}--${slugify(project.id)}${PROJECT_BOARD_FILE_SUFFIX}`;
}

export function serializeProjectLibraryToBoardFiles(
  library: Pick<ProjectLibraryState, 'projects'>,
): ProjectBoardFileText[] {
  return library.projects.map((project) => ({
    fileName: buildProjectBoardFileName(project),
    text: JSON.stringify(serializeProjectRecordToBoardFile(project), null, 2),
  }));
}

export function parseProjectBoardFileText(sourceText: string, fallbackName = '导入工程'): ProjectRecord {
  const parsed = JSON.parse(sourceText) as unknown;

  if (!isRecord(parsed) || parsed.kind !== PROJECT_BOARD_KIND) {
    throw new Error('看板文件格式无效。');
  }

  const schemaVersion =
    typeof parsed.schemaVersion === 'number' ? parsed.schemaVersion : PROJECT_SCHEMA_VERSION;
  const migrationNotes =
    schemaVersion === PROJECT_SCHEMA_VERSION
      ? []
      : [`已从看板文件 schema v${schemaVersion} 迁移到 v${PROJECT_SCHEMA_VERSION}。`];

  return normalizeProjectRecordFromBoardFile(parsed, fallbackName, migrationNotes);
}

export function parseProjectBoardFiles(boardFiles: ProjectBoardFileText[]): ProjectLibraryState {
  const projects = boardFiles.map((boardFile, index) =>
    parseProjectBoardFileText(boardFile.text, boardFile.fileName || `工程 ${index + 1}`),
  );

  return {
    kind: PROJECT_LIBRARY_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    projects,
  };
}

export function importProjectsFromText(sourceText: string): ImportProjectsResult {
  const parsed = JSON.parse(sourceText) as unknown;

  if (isRecord(parsed) && parsed.kind === PROJECT_BOARD_KIND) {
    const project = parseProjectBoardFileText(
      sourceText,
      normalizeString(parsed.name, '导入工程'),
    );

    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  if (isFlowgramWorkflowJsonLike(parsed)) {
    const project = buildImportedProjectFromFlowgramJson(parsed);
    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  if (isWorkflowGraphLike(parsed)) {
    const project = buildImportedProjectFromGraph(parsed);
    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  if (!isRecord(parsed)) {
    throw new Error('导入文件不是有效的项目包。');
  }

  if (parsed.kind === PROJECT_PACKAGE_KIND) {
    const sourceProject = isRecord(parsed.project) ? parsed.project : parsed;
    const schemaVersion =
      typeof parsed.schemaVersion === 'number' ? parsed.schemaVersion : PROJECT_SCHEMA_VERSION;
    const migrationNotes =
      schemaVersion === PROJECT_SCHEMA_VERSION
        ? []
        : [`已从 schema v${schemaVersion} 迁移到 v${PROJECT_SCHEMA_VERSION}。`];
    const project = normalizeProjectRecord(
      sourceProject,
      normalizeString(sourceProject.name, '导入工程'),
      migrationNotes,
    );

    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  if (parsed.kind === PROJECT_LIBRARY_KIND && Array.isArray(parsed.projects)) {
    const schemaVersion =
      typeof parsed.schemaVersion === 'number' ? parsed.schemaVersion : PROJECT_SCHEMA_VERSION;
    const migrationNotes =
      schemaVersion === PROJECT_SCHEMA_VERSION
        ? []
        : [`已从工程库 schema v${schemaVersion} 迁移到 v${PROJECT_SCHEMA_VERSION}。`];
    const projects = parsed.projects.map((item, index) =>
      normalizeProjectRecord(item, `导入工程 ${index + 1}`, migrationNotes),
    );

    return {
      importedProjects: projects,
      migrationNotes,
    };
  }

  if (typeof parsed.workflowAst === 'string') {
    const parsedGraph = parseWorkflowGraph(parsed.workflowAst);
    if (!parsedGraph.graph) {
      throw new Error(parsedGraph.error ?? '导入的工作流 AST 无法解析。');
    }

    const project = normalizeProjectRecord(
      {
        ...parsed,
        astText: formatWorkflowGraph(parsedGraph.graph),
      },
      normalizeString(parsed.name, '导入工程'),
      ['已从旧版工程包结构迁移到当前版本。'],
    );

    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  throw new Error('暂不支持该导入文件格式。');
}

export function upsertProjectRecord(
  projects: ProjectRecord[],
  project: ProjectRecord,
): ProjectRecord[] {
  const existingIndex = projects.findIndex((item) => item.id === project.id);
  if (existingIndex === -1) {
    return [project, ...projects];
  }

  const nextProjects = projects.slice();
  nextProjects[existingIndex] = project;
  return nextProjects;
}

export function prepareProjectExport(project: ProjectRecord): {
  fileName: string;
  text: string;
} {
  return {
    fileName: buildProjectBoardFileName(project),
    text: JSON.stringify(serializeProjectRecordToBoardFile(project), null, 2),
  };
}

export function mergeImportedProjects(
  existingProjects: ProjectRecord[],
  importedProjects: ProjectRecord[],
): ProjectRecord[] {
  let nextProjects = existingProjects.slice();

  importedProjects.forEach((project) => {
    const nextId = ensureUniqueProjectId(nextProjects, project.id || project.name);
    nextProjects = upsertProjectRecord(nextProjects, {
      ...project,
      id: nextId,
      updatedAt: nowIso(),
    });
  });

  return nextProjects.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
}
