import { afterEach, describe, expect, it, vi } from 'vitest';
import { FlowNodeBaseType, type WorkflowNodeEntity } from '@flowgram.ai/free-layout-editor';

import {
  buildFlowgramShortcutHandlers,
  getFlowgramShortcutSelectableNodes,
} from '../flowgram-shortcuts';

function setNavigatorUserAgent(userAgent: string | undefined) {
  if (userAgent === undefined) {
    Reflect.deleteProperty(globalThis, 'navigator');
    return;
  }

  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { userAgent },
  });
}

function createNode(
  id: string,
  flowNodeType: string,
  disposed = false,
): WorkflowNodeEntity {
  return {
    id,
    flowNodeType,
    disposed,
  } as unknown as WorkflowNodeEntity;
}

function createContext(nodes: WorkflowNodeEntity[]) {
  const selectionState = {
    selection: [] as WorkflowNodeEntity[],
    clear() {
      this.selection = [];
    },
  };
  const focus = vi.fn();
  const fitView = vi.fn();
  const redo = vi.fn();

  return {
    ctx: {
      document: {
        getAllNodes: () => nodes,
        selectServices: selectionState,
      },
      playground: {
        node: {
          focus,
        },
      },
      tools: {
        fitView,
      },
      history: {
        undoRedoService: {},
        canRedo: () => true,
        redo,
      },
    },
    focus,
    fitView,
    redo,
    selectionState,
  };
}

afterEach(() => {
  Reflect.deleteProperty(globalThis, 'navigator');
});

describe('getFlowgramShortcutSelectableNodes', () => {
  it('会过滤 root 和已销毁节点', () => {
    const nodes = [
      createNode('root', FlowNodeBaseType.ROOT),
      createNode('timer_1', 'timer'),
      createNode('code_1', 'code'),
      createNode('gone_1', 'code', true),
    ];

    expect(getFlowgramShortcutSelectableNodes(nodes).map((node) => node.id)).toEqual([
      'timer_1',
      'code_1',
    ]);
  });
});

describe('buildFlowgramShortcutHandlers', () => {
  it('Ctrl/Cmd+A 会全选当前画布节点并聚焦画布', () => {
    setNavigatorUserAgent('Windows NT');
    const nodes = [
      createNode('root', FlowNodeBaseType.ROOT),
      createNode('timer_1', 'timer'),
      createNode('code_1', 'code'),
    ];
    const { ctx, focus, selectionState } = createContext(nodes);
    const handlers = buildFlowgramShortcutHandlers(ctx as never);
    const selectAllHandler = handlers.find((handler) => handler.shortcuts.includes('ctrl a'));

    expect(selectAllHandler).toBeDefined();

    selectAllHandler?.execute();

    expect(selectionState.selection.map((node) => node.id)).toEqual(['timer_1', 'code_1']);
    expect(focus).toHaveBeenCalledTimes(1);
  });

  it('Escape 会清空选择', () => {
    const nodes = [createNode('timer_1', 'timer')];
    const { ctx, selectionState } = createContext(nodes);
    selectionState.selection = [...nodes];
    const handlers = buildFlowgramShortcutHandlers(ctx as never);
    const clearHandler = handlers.find((handler) => handler.shortcuts.includes('esc'));

    clearHandler?.execute();

    expect(selectionState.selection).toEqual([]);
  });

  it('Ctrl/Cmd+0 会触发适配视图', () => {
    const nodes = [createNode('timer_1', 'timer')];
    const { ctx, fitView } = createContext(nodes);
    const handlers = buildFlowgramShortcutHandlers(ctx as never);
    const fitViewHandler = handlers.find((handler) => handler.shortcuts.includes('ctrl 0'));

    fitViewHandler?.execute();

    expect(fitView).toHaveBeenCalledWith(true);
  });

  it('Windows/Linux 下启用 Ctrl+Y 重做', () => {
    setNavigatorUserAgent('Windows NT');
    const nodes = [createNode('timer_1', 'timer')];
    const { ctx, redo } = createContext(nodes);
    const handlers = buildFlowgramShortcutHandlers(ctx as never);
    const redoHandler = handlers.find((handler) => handler.shortcuts.includes('ctrl y'));

    expect(redoHandler?.isEnabled?.()).toBe(true);

    redoHandler?.execute();

    expect(redo).toHaveBeenCalledTimes(1);
  });

  it('macOS 下不启用 Ctrl+Y，继续沿用 Cmd+Shift+Z', () => {
    setNavigatorUserAgent('Macintosh');
    const nodes = [createNode('timer_1', 'timer')];
    const { ctx } = createContext(nodes);
    const handlers = buildFlowgramShortcutHandlers(ctx as never);
    const redoHandler = handlers.find((handler) => handler.shortcuts.includes('ctrl y'));

    expect(redoHandler?.isEnabled?.()).toBe(false);
  });
});
