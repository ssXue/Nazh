import { useMemo } from 'react';

import { createContainerNodePlugin } from '@flowgram.ai/free-container-plugin';
import { createFreeGroupPlugin } from '@flowgram.ai/free-group-plugin';
import { createFreeNodePanelPlugin } from '@flowgram.ai/free-node-panel-plugin';
import { createFreeSnapPlugin } from '@flowgram.ai/free-snap-plugin';
import { createMinimapPlugin } from '@flowgram.ai/minimap-plugin';
import { createPanelManagerPlugin } from '@flowgram.ai/panel-manager-plugin';
import type {
  FreeLayoutProps,
  FlowNodeType,
  onDragLineEndParams,
  FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
import { createDownloadPlugin } from '@flowgram.ai/export-plugin';

import {
  createFlowgramNodeRegistries,
  getDefaultFlowgramNodeRegistry,
  normalizeFlowgramNodeJson,
} from './flowgram-node-library';
import { FlowgramGroupNodeRender } from './FlowgramGroupNodeRender';
import { flowgramNodeSettingsPanelFactory } from './FlowgramNodeSettingsPanel';
import { createFlowgramQuickNodePanel } from './FlowgramQuickNodePanel';
import { flowgramRuntimePanelFactory } from './FlowgramRuntimePanel';

const FLOWGRAM_MINIMAP_CANVAS_WIDTH = 110;
const FLOWGRAM_MINIMAP_CANVAS_HEIGHT = 76;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

interface UseFlowgramEditorPropsParams {
  initialData: NonNullable<FreeLayoutProps['initialData']>;
  accentColor: string;
  primaryConnectionId: string | null;
  materials: NonNullable<FreeLayoutProps['materials']>;
  isFlowingLine: NonNullable<FreeLayoutProps['isFlowingLine']>;
  isErrorLine: NonNullable<FreeLayoutProps['isErrorLine']>;
  setLineClassName: NonNullable<FreeLayoutProps['setLineClassName']>;
  onContentChange: NonNullable<FreeLayoutProps['onContentChange']>;
  onAllLayersRendered: NonNullable<FreeLayoutProps['onAllLayersRendered']>;
  onDragLineEnd?: (ctx: FreeLayoutPluginContext, params: onDragLineEndParams) => Promise<void>;
}

export function useFlowgramEditorProps({
  initialData,
  accentColor,
  primaryConnectionId,
  materials,
  isFlowingLine,
  isErrorLine,
  setLineClassName,
  onContentChange,
  onAllLayersRendered,
  onDragLineEnd,
}: UseFlowgramEditorPropsParams): FreeLayoutProps {
  const nodeRegistries = useMemo(
    () => createFlowgramNodeRegistries(primaryConnectionId),
    [primaryConnectionId],
  );
  const nodePanelRenderer = useMemo(
    () => createFlowgramQuickNodePanel(primaryConnectionId),
    [primaryConnectionId],
  );

  return useMemo(
    () => ({
      background: true,
      readonly: false,
      initialData,
      nodeRegistries,
      getNodeDefaultRegistry(type: FlowNodeType) {
        return getDefaultFlowgramNodeRegistry(String(type));
      },
      fromNodeJSON(node, json) {
        return normalizeFlowgramNodeJson(
          {
            ...json,
            type: json.type ?? 'native',
          },
          primaryConnectionId,
        );
      },
      toNodeJSON(node, json) {
        const liveExtInfo = isRecord(node.getExtInfo()) ? node.getExtInfo() : {};
        const jsonData = isRecord(json.data) ? json.data : {};

        return normalizeFlowgramNodeJson(
          {
            ...json,
            type: json.type ?? node.flowNodeType ?? 'native',
            data: {
              ...jsonData,
              ...liveExtInfo,
            },
          },
          primaryConnectionId,
        );
      },
      playground: {
        autoFocus: false,
        autoResize: true,
        preventGlobalGesture: true,
      },
      scroll: {
        enableScrollLimit: false,
      },
      materials,
      nodeEngine: {
        enable: true,
      },
      history: {
        enable: true,
        enableChangeNode: true,
      },
      isFlowingLine,
      isErrorLine,
      setLineClassName,
      onContentChange,
      onAllLayersRendered,
      onDragLineEnd,
      plugins: () => [
        createMinimapPlugin({
          disableLayer: true,
          canvasStyle: {
            canvasWidth: FLOWGRAM_MINIMAP_CANVAS_WIDTH,
            canvasHeight: FLOWGRAM_MINIMAP_CANVAS_HEIGHT,
            canvasPadding: 18,
            canvasBorderRadius: 8,
          },
        }),
        createFreeSnapPlugin({
          edgeColor: accentColor,
          alignColor: accentColor,
          edgeLineWidth: 1,
          alignLineWidth: 1,
          alignCrossWidth: 8,
        }),
        createFreeNodePanelPlugin({
          renderer: nodePanelRenderer,
        }),
        createContainerNodePlugin({}),
        createFreeGroupPlugin({
          groupNodeRender: FlowgramGroupNodeRender,
        }),
        createDownloadPlugin({}),
        createPanelManagerPlugin({
          factories: [flowgramNodeSettingsPanelFactory, flowgramRuntimePanelFactory],
          right: {
            max: 1,
          },
          bottom: {
            max: 1,
          },
        }),
      ],
    }),
    [
      initialData,
      accentColor,
      isErrorLine,
      isFlowingLine,
      materials,
      nodeRegistries,
      nodePanelRenderer,
      onAllLayersRendered,
      onContentChange,
      onDragLineEnd,
      primaryConnectionId,
      setLineClassName,
    ],
  );
}
