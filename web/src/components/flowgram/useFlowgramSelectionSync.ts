import {
  FlowNodeBaseType,
  type FlowNodeEntity,
  type FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
import { PanelManager } from '@flowgram.ai/panel-manager-plugin';
import {
  useCallback,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';

import type {
  AiGenerationParams,
  AiProviderView,
  ConnectionDefinition,
} from '../../types';
import { FLOWGRAM_NODE_SETTINGS_PANEL_KEY } from './FlowgramNodeSettingsPanel';
import { isKnownEditorNodeType } from './flowgram-node-library';

interface UseFlowgramSelectionSyncOptions {
  selectedNodeRef: MutableRefObject<FlowNodeEntity | null>;
  setHasSelection: Dispatch<SetStateAction<boolean>>;
  connectionOptions: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
  emitCurrentGraphChange: (ctx: FreeLayoutPluginContext) => string | null;
  reportFlowgramError: (title: string, error: unknown) => void;
}

function isBusinessFlowNode(node: FlowNodeEntity | null): node is FlowNodeEntity {
  if (!node || node.flowNodeType === FlowNodeBaseType.GROUP) {
    return false;
  }

  const rawData = (node.getExtInfo() ?? {}) as {
    nodeType?: string;
  };
  const explicitNodeType =
    typeof rawData.nodeType === 'string' && rawData.nodeType.trim()
      ? rawData.nodeType.trim()
      : null;

  return explicitNodeType !== null || isKnownEditorNodeType(node.flowNodeType);
}

export function useFlowgramSelectionSync({
  selectedNodeRef,
  setHasSelection,
  connectionOptions,
  aiProviders,
  activeAiProviderId,
  copilotParams,
  emitCurrentGraphChange,
  reportFlowgramError,
}: UseFlowgramSelectionSyncOptions) {
  return useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      try {
        if (!ctx) {
          selectedNodeRef.current = null;
          setHasSelection(false);
          return;
        }

        const selectionService = ctx.document.selectServices;
        const selectedNodes = selectionService.selectedNodes;
        const nextSelectedNode = selectedNodes.length === 1 ? selectedNodes[0] : null;
        const nextBusinessNode = isBusinessFlowNode(nextSelectedNode) ? nextSelectedNode : null;
        const hadPreviousSelection = Boolean(selectedNodeRef.current);

        selectedNodeRef.current = nextBusinessNode;
        setHasSelection(Boolean(nextBusinessNode));

        if (hadPreviousSelection && !nextBusinessNode) {
          window.setTimeout(() => emitCurrentGraphChange(ctx), 0);
        }

        const panelManager = (ctx as FreeLayoutPluginContext & {
          get?: <T>(token: unknown) => T;
        }).get?.<PanelManager>(PanelManager);

        if (!panelManager) {
          return;
        }

        if (ctx.playground.config.readonly) {
          panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY);
          return;
        }

        if (nextBusinessNode) {
          panelManager.open(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right', {
            props: {
              nodeId: nextBusinessNode.id,
              connections: connectionOptions,
              aiProviders,
              activeAiProviderId,
              copilotParams,
            },
          });
          return;
        }

        panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY);
      } catch (error) {
        reportFlowgramError('FlowGram 选择状态同步失败', error);
      }
    },
    [
      activeAiProviderId,
      aiProviders,
      connectionOptions,
      copilotParams,
      emitCurrentGraphChange,
      reportFlowgramError,
      selectedNodeRef,
      setHasSelection,
    ],
  );
}
