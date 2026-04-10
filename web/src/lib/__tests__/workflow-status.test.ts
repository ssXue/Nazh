// workflow-status 单元测试
import { describe, expect, it } from 'vitest';
import type { DeployResponse, WorkflowWindowStatus } from '../../types';
import { EMPTY_RUNTIME_STATE } from '../workflow-events';
import {
  deriveWorkflowStatus,
  getWorkflowStatusLabel,
  getWorkflowStatusPillClass,
} from '../workflow-status';

/** 最小 DeployResponse 存根，仅满足类型要求。 */
const STUB_DEPLOY: DeployResponse = { nodeCount: 2, edgeCount: 1, rootNodes: ['node-a'] };

describe('deriveWorkflowStatus', () => {
  it('非 Tauri 环境 → preview', () => {
    const status = deriveWorkflowStatus(false, true, STUB_DEPLOY, EMPTY_RUNTIME_STATE);
    expect(status).toBe('preview');
  });

  it('无活跃面板 → idle', () => {
    const status = deriveWorkflowStatus(true, false, STUB_DEPLOY, EMPTY_RUNTIME_STATE);
    expect(status).toBe('idle');
  });

  it('未部署（deployInfo 为 null）→ idle', () => {
    const status = deriveWorkflowStatus(true, true, null, EMPTY_RUNTIME_STATE);
    expect(status).toBe('idle');
  });

  it('已部署但无事件 → deployed', () => {
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, EMPTY_RUNTIME_STATE);
    expect(status).toBe('deployed');
  });

  it('存在活跃节点 → running', () => {
    const runtimeState = { ...EMPTY_RUNTIME_STATE, activeNodeIds: ['node-a'] };
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, runtimeState);
    expect(status).toBe('running');
  });

  it('lastEventType 为 started → running', () => {
    const runtimeState = { ...EMPTY_RUNTIME_STATE, lastEventType: 'started' as const };
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, runtimeState);
    expect(status).toBe('running');
  });

  it('存在失败节点 → failed', () => {
    const runtimeState = { ...EMPTY_RUNTIME_STATE, failedNodeIds: ['node-b'] };
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, runtimeState);
    expect(status).toBe('failed');
  });

  it('lastEventType 为 failed → failed', () => {
    const runtimeState = { ...EMPTY_RUNTIME_STATE, lastEventType: 'failed' as const };
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, runtimeState);
    expect(status).toBe('failed');
  });

  it('有输出节点且有 traceId → completed', () => {
    const runtimeState = {
      ...EMPTY_RUNTIME_STATE,
      traceId: 'trace-1',
      outputNodeIds: ['node-c'],
    };
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, runtimeState);
    expect(status).toBe('completed');
  });

  it('completed 事件且无活跃节点 → completed', () => {
    const runtimeState = {
      ...EMPTY_RUNTIME_STATE,
      traceId: 'trace-1',
      lastEventType: 'completed' as const,
      completedNodeIds: ['node-d'],
      activeNodeIds: [],
    };
    const status = deriveWorkflowStatus(true, true, STUB_DEPLOY, runtimeState);
    expect(status).toBe('completed');
  });
});

describe('getWorkflowStatusLabel', () => {
  const cases: Array<[WorkflowWindowStatus, string]> = [
    ['preview', '浏览器预览'],
    ['idle', '未部署'],
    ['deployed', '已部署待运行'],
    ['running', '运行中'],
    ['completed', '执行完成'],
    ['failed', '执行失败'],
  ];

  for (const [status, expected] of cases) {
    it(`${status} → "${expected}"`, () => {
      expect(getWorkflowStatusLabel(status)).toBe(expected);
    });
  }
});

describe('getWorkflowStatusPillClass', () => {
  it('running → runtime-pill--running', () => {
    expect(getWorkflowStatusPillClass('running')).toBe('runtime-pill--running');
  });

  it('failed → runtime-pill--failed', () => {
    expect(getWorkflowStatusPillClass('failed')).toBe('runtime-pill--failed');
  });

  it('completed → runtime-pill--ready', () => {
    expect(getWorkflowStatusPillClass('completed')).toBe('runtime-pill--ready');
  });

  it('deployed → runtime-pill--ready', () => {
    expect(getWorkflowStatusPillClass('deployed')).toBe('runtime-pill--ready');
  });

  it('idle → runtime-pill--idle', () => {
    expect(getWorkflowStatusPillClass('idle')).toBe('runtime-pill--idle');
  });

  it('preview → runtime-pill--idle', () => {
    expect(getWorkflowStatusPillClass('preview')).toBe('runtime-pill--idle');
  });
});
