import type { RuntimeDockProps } from '../types';

export type { RuntimeDockProps };

export type RuntimeDockPanel = 'events' | 'results' | 'payload' | 'connections' | 'variables';

export const runtimeDockTabs: Array<{ id: RuntimeDockPanel; label: string; title: string }> = [
  { id: 'events', label: '事件', title: '执行事件流' },
  { id: 'results', label: '结果', title: '结果载荷' },
  { id: 'payload', label: '载荷', title: '测试载荷' },
  { id: 'connections', label: '连接', title: '连接资源' },
  { id: 'variables', label: '变量', title: '运行时变量' },
];

export interface DockColumn {
  id: string;
  panel: RuntimeDockPanel;
}

let nextColumnId = 1;
export function createColumnId(): string {
  return `c${nextColumnId++}`;
}

export function getPanelCount(panel: RuntimeDockPanel, counts: Record<RuntimeDockPanel, number>): number {
  return counts[panel];
}

export function normalizeResultPayload(payload: unknown): Record<string, unknown> | unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }

  if (payload && typeof payload === 'object') {
    return payload as Record<string, unknown>;
  }

  return { value: payload };
}
