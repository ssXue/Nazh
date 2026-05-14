import {
  Command,
  FlowNodeBaseType,
  ShortcutsRegistry,
  createPlaygroundPlugin,
  type FreeLayoutPluginContext,
  type ShortcutsHandler,
  type WorkflowNodeEntity,
} from '@flowgram.ai/free-layout-editor';

const FLOWGRAM_SELECT_ALL_COMMAND = 'nazh.flowgram.selectAll';
const FLOWGRAM_CLEAR_SELECTION_COMMAND = 'nazh.flowgram.clearSelection';
const FLOWGRAM_FIT_VIEW_COMMAND = 'nazh.flowgram.fitView';
const FLOWGRAM_REDO_WINDOWS_COMMAND = 'nazh.flowgram.redoWindows';
const FLOWGRAM_DELETE_SELECTED_COMMAND = 'nazh.flowgram.deleteSelected';
const FLOWGRAM_DUPLICATE_SELECTED_COMMAND = 'nazh.flowgram.duplicateSelected';
const FLOWGRAM_UNDO_WINDOWS_COMMAND = 'nazh.flowgram.undoWindows';

export function isMacLikeShortcutPlatform(): boolean {
  if (typeof navigator === 'undefined') {
    return false;
  }

  return /(Macintosh|MacIntel|MacPPC|Mac68K|iPad)/.test(navigator.userAgent);
}

export function getFlowgramShortcutSelectableNodes(
  nodes: WorkflowNodeEntity[],
): WorkflowNodeEntity[] {
  return nodes.filter(
    (node) => node.flowNodeType !== FlowNodeBaseType.ROOT && !node.disposed,
  );
}

export function buildFlowgramShortcutHandlers(
  ctx: FreeLayoutPluginContext,
): ShortcutsHandler[] {
  const selectService = ctx.document.selectServices;
  const handlers: ShortcutsHandler[] = [
    {
      commandId: FLOWGRAM_SELECT_ALL_COMMAND,
      commandDetail: {
        label: 'Select all nodes',
      },
      shortcuts: ['meta a', 'ctrl a'],
      isEnabled: () => getFlowgramShortcutSelectableNodes(ctx.document.getAllNodes()).length > 0,
      execute: () => {
        selectService.selection = getFlowgramShortcutSelectableNodes(ctx.document.getAllNodes());
        ctx.playground.node?.focus?.();
      },
    },
    {
      commandId: FLOWGRAM_CLEAR_SELECTION_COMMAND,
      commandDetail: {
        label: 'Clear selection',
      },
      shortcuts: ['esc'],
      isEnabled: () => selectService.selection.length > 0,
      execute: () => {
        selectService.clear();
        ctx.playground.node?.focus?.();
      },
    },
    {
      commandId: FLOWGRAM_FIT_VIEW_COMMAND,
      commandDetail: {
        label: 'Fit view',
      },
      shortcuts: ['meta 0', 'ctrl 0'],
      isEnabled: () => getFlowgramShortcutSelectableNodes(ctx.document.getAllNodes()).length > 0,
      execute: () => {
        void ctx.tools.fitView(true);
      },
    },
  ];

  if (ctx.history?.undoRedoService) {
    handlers.push({
      commandId: FLOWGRAM_REDO_WINDOWS_COMMAND,
      commandDetail: {
        label: 'Redo',
      },
      shortcuts: ['ctrl y'],
      // Keep macOS on Cmd+Shift+Z; Ctrl+Y is primarily for Windows/Linux muscle memory.
      isEnabled: () => !isMacLikeShortcutPlatform() && ctx.history.canRedo(),
      execute: () => {
        void ctx.history.redo();
      },
    });
    handlers.push({
      commandId: FLOWGRAM_UNDO_WINDOWS_COMMAND,
      commandDetail: {
        label: 'Undo',
      },
      shortcuts: ['ctrl z'],
      isEnabled: () => !isMacLikeShortcutPlatform() && ctx.history.canUndo(),
      execute: () => {
        void ctx.history.undo();
      },
    });
  }

  // 删除选中节点
  handlers.push({
    commandId: FLOWGRAM_DELETE_SELECTED_COMMAND,
    commandDetail: {
      label: 'Delete selected nodes',
    },
    shortcuts: ['backspace', 'delete'],
    isEnabled: () => {
      const sel = selectService.selection;
      return sel.length > 0 && sel.some((n) => (n as WorkflowNodeEntity).flowNodeType !== FlowNodeBaseType.ROOT);
    },
    execute: () => {
      const selectable = selectService.selection.filter(
        (n) => (n as WorkflowNodeEntity).flowNodeType !== FlowNodeBaseType.ROOT && !n.disposed,
      );
      for (const node of selectable) {
        node.dispose();
      }
      selectService.clear();
    },
  });

  // 复制选中节点
  handlers.push({
    commandId: FLOWGRAM_DUPLICATE_SELECTED_COMMAND,
    commandDetail: {
      label: 'Duplicate selected nodes',
    },
    shortcuts: ['meta d', 'ctrl d'],
    isEnabled: () => {
      const sel = selectService.selection;
      return sel.length > 0 && sel.some((n) => (n as WorkflowNodeEntity).flowNodeType !== FlowNodeBaseType.ROOT);
    },
    execute: () => {
      const selectable = selectService.selection.filter(
        (n) => (n as WorkflowNodeEntity).flowNodeType !== FlowNodeBaseType.ROOT && !n.disposed,
      );
      const copied: WorkflowNodeEntity[] = [];
      for (const node of selectable) {
        const newNode = ctx.document.copyNode(node as WorkflowNodeEntity);
        if (newNode) {
          copied.push(newNode);
        }
      }
      if (copied.length > 0) {
        selectService.selection = copied;
      }
    },
  });

  return handlers;
}

export function registerFlowgramShortcuts(
  ctx: FreeLayoutPluginContext,
  registry: ShortcutsRegistry,
) {
  registry.addHandlers(...buildFlowgramShortcutHandlers(ctx));
}

export const flowgramShortcutsPlugin = createPlaygroundPlugin<FreeLayoutPluginContext>({
  onInit: (ctx) => {
    const registry = ctx.get<ShortcutsRegistry>(ShortcutsRegistry);
    registerFlowgramShortcuts(ctx, registry);
  },
});
