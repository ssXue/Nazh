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
  type FlowgramConnectionDefaults,
  getDefaultFlowgramNodeRegistry,
  normalizeFlowgramNodeJson,
} from './flowgram-node-library';
import { flowgramShortcutsPlugin } from './flowgram-shortcuts';
import { FlowgramGroupNodeRender } from './FlowgramGroupNodeRender';
import { flowgramNodeSettingsPanelFactory } from './FlowgramNodeSettingsPanel';
import { createFlowgramQuickNodePanel } from './FlowgramQuickNodePanel';
import { flowgramRuntimePanelFactory } from './FlowgramRuntimePanel';

const FLOWGRAM_MINIMAP_CANVAS_WIDTH = 110;
const FLOWGRAM_MINIMAP_CANVAS_HEIGHT = 76;
const FLOWGRAM_BACKGROUND_OPTIONS = {
  gridSize: 24,
  dotSize: 1,
  dotColor: 'var(--flowgram-canvas-grid)',
  dotFillColor: 'var(--flowgram-canvas-grid)',
  dotOpacity: 1,
  backgroundColor: 'var(--flowgram-canvas-bg)',
};

type FlowgramThemeMode = 'light' | 'dark';

function buildMinimapCanvasStyle(themeMode: FlowgramThemeMode) {
  const isDark = themeMode === 'dark';

  return {
    canvasWidth: FLOWGRAM_MINIMAP_CANVAS_WIDTH,
    canvasHeight: FLOWGRAM_MINIMAP_CANVAS_HEIGHT,
    canvasPadding: 18,
    canvasBackground: isDark ? 'rgba(21, 23, 28, 1)' : 'rgba(245, 246, 248, 1)',
    canvasBorderRadius: 8,
    viewportBackground: isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(255, 255, 255, 0.78)',
    viewportBorderColor: isDark ? 'rgba(255, 255, 255, 0.2)' : 'rgba(60, 60, 67, 0.14)',
    nodeColor: isDark ? 'rgba(255, 255, 255, 0.18)' : 'rgba(60, 60, 67, 0.16)',
    nodeBorderColor: isDark ? 'rgba(255, 255, 255, 0.16)' : 'rgba(60, 60, 67, 0.12)',
    overlayColor: isDark ? 'rgba(0, 0, 0, 0.32)' : 'rgba(255, 255, 255, 0.48)',
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

interface UseFlowgramEditorPropsParams {
  initialData: NonNullable<FreeLayoutProps['initialData']>;
  accentColor: string;
  themeMode: FlowgramThemeMode;
  connectionDefaults: FlowgramConnectionDefaults;
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
  themeMode,
  connectionDefaults,
  materials,
  isFlowingLine,
  isErrorLine,
  setLineClassName,
  onContentChange,
  onAllLayersRendered,
  onDragLineEnd,
}: UseFlowgramEditorPropsParams): FreeLayoutProps {
  const nodeRegistries = useMemo(
    () => createFlowgramNodeRegistries(connectionDefaults),
    [connectionDefaults],
  );
  const nodePanelRenderer = useMemo(
    () => createFlowgramQuickNodePanel(connectionDefaults),
    [connectionDefaults],
  );
  const minimapCanvasStyle = useMemo(() => buildMinimapCanvasStyle(themeMode), [themeMode]);

  return useMemo(
    () => ({
      background: FLOWGRAM_BACKGROUND_OPTIONS,
      readonly: false,
      initialData,
      nodeRegistries,
      getNodeDefaultRegistry(type: FlowNodeType) {
        return getDefaultFlowgramNodeRegistry(String(type));
      },
      fromNodeJSON(node, json) {
        const normalized = normalizeFlowgramNodeJson(
          {
            ...json,
            type: json.type ?? 'native',
          },
          connectionDefaults,
        );
        // 将归一化后的 data 同步写入 extInfo，
        // 否则 getExtInfo() 返回空值，节点卡片和设置面板无法读取 config。
        if (isRecord(normalized.data)) {
          node.updateExtInfo(normalized.data);
        }
        return normalized;
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
          connectionDefaults,
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
            ...minimapCanvasStyle,
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
        flowgramShortcutsPlugin,
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
      minimapCanvasStyle,
      nodeRegistries,
      nodePanelRenderer,
      onAllLayersRendered,
      onContentChange,
      onDragLineEnd,
      connectionDefaults,
      setLineClassName,
    ],
  );
}
