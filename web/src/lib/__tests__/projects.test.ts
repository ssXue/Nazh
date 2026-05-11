// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest';

import {
  importProjectsFromText,
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
