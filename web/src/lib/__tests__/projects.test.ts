// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest';

import {
  applyEnvironmentToConnectionDefinitions,
  applyEnvironmentToGraph,
  buildDefaultProjectLibrary,
  deleteProjectSnapshot,
  importProjectsFromText,
  loadProjectLibrary,
  parseProjectBoardFileText,
  persistProjectLibrary,
  prepareProjectExport,
  PROJECT_BOARD_KIND,
  rollbackProjectToSnapshot,
} from '../projects';

afterEach(() => {
  localStorage.clear();
});

describe('importProjectsFromText', () => {
  it('支持从裸工作流 AST 迁移为项目包', () => {
    const source = JSON.stringify({
      name: '裸工作流',
      connections: [],
      nodes: {
        timer_trigger: {
          type: 'timer',
          config: {
            interval_ms: 1000,
          },
        },
      },
      edges: [],
    });

    const result = importProjectsFromText(source);

    expect(result.importedProjects).toHaveLength(1);
    expect(result.importedProjects[0].name).toBe('裸工作流');
    expect(result.importedProjects[0].migrationNotes[0]).toContain('裸工作流 AST');
    expect(result.importedProjects[0].snapshots).toHaveLength(1);
  });

  it('支持从 Flowgram 导出 JSON 迁移为项目包', () => {
    const source = JSON.stringify({
      nodes: [
        {
          id: 'timer_trigger',
          type: 'timer',
          meta: {
            position: {
              x: 48,
              y: 88,
            },
          },
          data: {
            nodeType: 'timer',
            config: {
              interval_ms: 1000,
              immediate: true,
            },
            connectionId: null,
            timeoutMs: null,
          },
        },
      ],
      edges: [],
    });

    const result = importProjectsFromText(source);

    expect(result.importedProjects).toHaveLength(1);
    expect(result.importedProjects[0].name).toBe('Flowgram 导入工程');
    expect(result.importedProjects[0].migrationNotes[0]).toContain('Flowgram');
    expect(result.importedProjects[0].snapshots).toHaveLength(1);
  });
});

describe('project board file serialization', () => {
  it('导出单看板文件时以 Flowgram nodes/edges 作为顶层结构', () => {
    const project = buildDefaultProjectLibrary().projects[0];
    const exported = prepareProjectExport(project);
    const parsed = JSON.parse(exported.text) as Record<string, unknown>;

    expect(exported.fileName).toMatch(/\.nazh-board\.json$/);
    expect(parsed.kind).toBe(PROJECT_BOARD_KIND);
    expect(Array.isArray(parsed.nodes)).toBe(true);
    expect(Array.isArray(parsed.edges)).toBe(true);
    expect(parsed.name).toBe(project.name);
  });

  it('可以从单看板文件恢复项目记录与快照', () => {
    const project = buildDefaultProjectLibrary().projects[0];
    const exported = prepareProjectExport(project);
    const hydrated = parseProjectBoardFileText(exported.text, project.name);

    expect(hydrated.id).toBe(project.id);
    expect(hydrated.name).toBe(project.name);
    expect(hydrated.snapshots).toHaveLength(project.snapshots.length);
    expect(JSON.parse(hydrated.astText)).toHaveProperty('editor_graph');
  });
});

describe('applyEnvironmentToGraph', () => {
  it('会将环境差异合并到节点 config', () => {
    const library = buildDefaultProjectLibrary();
    const targetProject = library.projects[0];
    const targetEnvironment = targetProject.environments[1];
    const graph = JSON.parse(targetProject.astText);

    const nextGraph = applyEnvironmentToGraph(graph, targetEnvironment);
    const sqlWriterConfig = nextGraph.nodes.sql_writer?.config as Record<string, unknown>;

    expect(sqlWriterConfig).toMatchObject({
      database_path: './data/test-edge-runtime.sqlite3',
    });
  });
});

describe('applyEnvironmentToConnectionDefinitions', () => {
  it('会将环境差异合并到全局连接 metadata', () => {
    const library = buildDefaultProjectLibrary();
    const targetProject = library.projects[0];
    const targetEnvironment = targetProject.environments[1];
    const graph = JSON.parse(targetProject.astText);

    const nextConnections = applyEnvironmentToConnectionDefinitions(
      graph.connections ?? [],
      targetEnvironment,
    );
    const modbusConnection = nextConnections.find((connection) => connection.id === 'plc-main');

    expect(modbusConnection?.metadata).toMatchObject({
      host: '192.168.10.99',
      port: 1502,
    });
  });
});

describe('project library persistence and rollback', () => {
  it('会持久化项目库并在回滚时恢复旧版本', () => {
    const library = buildDefaultProjectLibrary();
    persistProjectLibrary(library);

    const hydrated = loadProjectLibrary();
    expect(hydrated.projects).toHaveLength(library.projects.length);

    const project = hydrated.projects[0];
    const initialSnapshot = project.snapshots[0];
    const modifiedProject = {
      ...project,
      astText: project.astText.replace('temperature_audit', 'temperature_archive'),
      payloadText: JSON.stringify({ changed: true }, null, 2),
    };

    const rolledBack = rollbackProjectToSnapshot(modifiedProject, initialSnapshot.id);

    expect(rolledBack.astText).toBe(initialSnapshot.astText);
    expect(rolledBack.payloadText).toBe(initialSnapshot.payloadText);
    expect(rolledBack.snapshots[0].reason).toBe('rollback');
  });

  it('支持删除单个快照', () => {
    const library = buildDefaultProjectLibrary();
    const project = library.projects[0];
    const snapshotToDelete = project.snapshots[0];

    const nextProject = deleteProjectSnapshot(project, snapshotToDelete.id);

    expect(nextProject.snapshots).toHaveLength(project.snapshots.length - 1);
    expect(nextProject.snapshots.some((snapshot) => snapshot.id === snapshotToDelete.id)).toBe(false);
  });
});
