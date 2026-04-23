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
  }

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
