// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest';

import {
  applyEnvironmentToGraph,
  buildDefaultProjectLibrary,
  importProjectsFromText,
  loadProjectLibrary,
  persistProjectLibrary,
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
});

describe('applyEnvironmentToGraph', () => {
  it('会将环境差异合并到连接 metadata 与节点 config', () => {
    const library = buildDefaultProjectLibrary();
    const targetProject = library.projects[0];
    const targetEnvironment = targetProject.environments[1];
    const graph = JSON.parse(targetProject.astText);

    const nextGraph = applyEnvironmentToGraph(graph, targetEnvironment);
    const modbusConnection = nextGraph.connections?.find((connection) => connection.id === 'plc-main');
    const sqlWriterConfig = nextGraph.nodes.sql_writer?.config as Record<string, unknown>;

    expect(modbusConnection?.metadata).toMatchObject({
      host: '192.168.10.99',
      port: 1502,
    });
    expect(sqlWriterConfig).toMatchObject({
      database_path: './data/test-edge-runtime.sqlite3',
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
});
