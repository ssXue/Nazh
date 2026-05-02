import {
  EditorRenderer,
  FreeLayoutEditorProvider,
  FlowNodeBaseType,
  FlowNodeEntity,
  WorkflowContentChangeType,
  WorkflowLineEntity,
  type WorkflowLinesManager,
  type WorkflowJSON as FlowgramWorkflowJSON,
  type WorkflowContentChangeEvent,
  type FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
import { PanelManager } from '@flowgram.ai/panel-manager-plugin';
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';

import { FlowgramNodeAddPanel } from './flowgram/FlowgramNodeAddPanel';
import type { ThemeMode } from './app/types';
import { FLOWGRAM_NODE_SETTINGS_PANEL_KEY } from './flowgram/FlowgramNodeSettingsPanel';
import {
  getLogicNodeBranchDefinitions,
  isKnownEditorNodeType,
  normalizeNodeKind,
  resolveNodeData,
  type FlowgramConnectionDefaults,
  type NodeSeed,
} from './flowgram/flowgram-node-library';
import { handleFlowgramDragLineEnd } from './flowgram/flowgram-line-panel';
import { useFlowgramEditorProps } from './flowgram/useFlowgramEditorProps';
import {
  formatWorkflowGraph,
  toFlowgramWorkflowJson,
  toNazhWorkflowGraph,
} from '../lib/flowgram';
import { FlowDownloadFormat, FlowDownloadService } from '@flowgram.ai/export-plugin';
import {
  configToRecord,
  invalidateNodePinSchema,
  refreshNodePinSchema,
} from '../lib/pin-schema-cache';
import {
  type ConnectionRejection,
  checkConnection,
  formatRejection,
} from '../lib/pin-validator';
import { hasTauriRuntime, saveFlowgramExportFile } from '../lib/tauri';
import { refreshCapabilitiesCache } from '../lib/node-capabilities-cache';
import { allocateNodeId } from '../lib/workflow-node-id';
import type {
  AiGenerationParams,
  AiProviderView,
  ConnectionDefinition,
  WorkflowGraph,
  WorkflowRuntimeState,
  WorkflowWindowStatus,
} from '../types';

// 从拆分模块导入
import {
  isSerialConnectionType,
  isModbusConnectionType,
  isMqttConnectionType,
  isHttpConnectionType,
  isBarkConnectionType,
  buildFlowgramExportFileName,
  buildFlowgramGraphSignature,
  getCanvasWorkflowStatusLabel,
  describeFlowgramError,
} from './flowgram/flowgram-canvas-utils';
import {
  type RuntimeNodeStatus,
  type FlowgramNodeMaterialProps,
  FlowgramContainerCard,
  FlowgramNodeCard,
} from './flowgram/FlowgramNodeCards';
import { FlowgramToolbar } from './flowgram/FlowgramToolbar';

export interface FlowgramCanvasResources {
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
}

export interface FlowgramCanvasRuntime {
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  canTestRun?: boolean;
}

export interface FlowgramCanvasAppearance {
  accentHex: string;
  themeMode: ThemeMode;
  nodeCodeColor: string;
}

export interface FlowgramCanvasExportTarget {
  workspacePath?: string;
  workflowName?: string | null;
}

export interface FlowgramCanvasActions {
  onRunRequested?: () => void;
  onStopRequested?: () => void;
  onTestRunRequested?: () => void;
  onGraphChange: (nextAstText: string) => void;
  onError?: (title: string, detail?: string | null) => void;
  onStatusMessage?: (message: string) => void;
}

interface FlowgramCanvasProps {
  graph: WorkflowGraph | null;
  resources: FlowgramCanvasResources;
  runtime: FlowgramCanvasRuntime;
  appearance: FlowgramCanvasAppearance;
  exportTarget?: FlowgramCanvasExportTarget;
  actions: FlowgramCanvasActions;
}

export interface FlowgramCanvasHandle {
  isReady: () => boolean;
  getCurrentWorkflowGraph: () => WorkflowGraph | null;
  loadWorkflowGraph: (graph: WorkflowGraph) => void;
}

interface InternalFlowExportImageService {
  export: (options: { format: FlowDownloadFormat; watermarkSVG?: string }) => Promise<string | undefined>;
}

interface InternalFlowDownloadService {
  download: (params: { format: FlowDownloadFormat }) => Promise<void>;
  document: {
    toJSON: () => unknown;
  };
  exportImageService: InternalFlowExportImageService;
  options?: {
    watermarkSVG?: string;
  };
  formatDataContent: (
    json: unknown,
    format: FlowDownloadFormat,
  ) => Promise<{
    content: string;
    mimeType: string;
  }>;
  setDownloading: (value: boolean) => void;
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

/** 拒收一条连接时给用户的视觉 + 诊断反馈。
 *
 * `toPort.hasError = true` 让 FlowGram 默认 `isErrorPort` 渲染红色，
 * 1.5s 后自动复位。`canAddLine` 返回 false 后边其实不会真创建线，
 * 这里的红色仅作"刚才被拒"的瞬时反馈。 */
function applyConnectionRejectionFeedback(
  toPort: { hasError?: boolean },
  rejection: ConnectionRejection,
): void {
  toPort.hasError = true;
  window.setTimeout(() => {
    toPort.hasError = false;
  }, 1500);
  console.warn(`[pin-validator] ${formatRejection(rejection)}`);
}

function getDefaultOutputPortId(node: FlowNodeEntity | null): string | undefined {
  if (!node) {
    return undefined;
  }

  const rawData = (node.getExtInfo() ?? {}) as {
    nodeType?: string;
    config?: unknown;
  };
  const nodeType = normalizeNodeKind(rawData.nodeType ?? node.flowNodeType);
  return getLogicNodeBranchDefinitions(nodeType, rawData.config)[0]?.key;
}

// FlowgramContainerCard / FlowgramNodeCard / FlowgramToolButton / FlowgramToolbar
// 已拆分到 flowgram/FlowgramNodeCards.tsx 和 flowgram/FlowgramToolbar.tsx。

export const FlowgramCanvas = forwardRef<FlowgramCanvasHandle, FlowgramCanvasProps>(function FlowgramCanvas({
  graph,
  resources,
  runtime,
  appearance,
  exportTarget,
  actions,
}, ref) {
  const { connections, aiProviders, activeAiProviderId, copilotParams } = resources;
  const { runtimeState, workflowStatus, canTestRun = false } = runtime;
  const { accentHex, themeMode, nodeCodeColor } = appearance;
  const { workspacePath, workflowName } = exportTarget ?? {};
  const {
    onRunRequested,
    onStopRequested,
    onTestRunRequested,
    onGraphChange,
    onError,
    onStatusMessage,
  } = actions;
  const [lastChange, setLastChange] = useState<string | null>(null);
  const [editorCtx, setEditorCtx] = useState<FreeLayoutPluginContext | null>(null);
  const [hasSelection, setHasSelection] = useState(false);
  const [minimapVisible, setMinimapVisible] = useState(false);
  const [isReadonlyMode, setIsReadonlyMode] = useState(false);
  const syncTimerRef = useRef<number | null>(null);
  const selectedNodeRef = useRef<FlowNodeEntity | null>(null);
  const latestGraphRef = useRef<WorkflowGraph | null>(graph);
  const applyingExternalGraphRef = useRef(false);
  const initialFlowgramDataRef = useRef<FlowgramWorkflowJSON | null>(null);
  const pendingFitViewRef = useRef(true);
  const flowgramData = useMemo(() => {
    if (!graph) {
      return null;
    }

    return toFlowgramWorkflowJson(graph);
  }, [graph]);
  const flowgramDataSignature = useMemo(
    () => (flowgramData ? buildFlowgramGraphSignature(flowgramData) : null),
    [flowgramData],
  );
  const latestFlowgramDataRef = useRef<FlowgramWorkflowJSON | null>(flowgramData);

  const reportFlowgramError = useCallback(
    (title: string, error: unknown) => {
      const detail = describeFlowgramError(error);
      console.error(title, error);
      onError?.(title, detail);
    },
    [onError],
  );

  useEffect(() => {
    if (graph) {
      latestGraphRef.current = graph;
    }
  }, [graph]);

  useEffect(() => {
    if (flowgramData) {
      latestFlowgramDataRef.current = flowgramData;
    }
  }, [flowgramData]);

  const resolvedGraph = graph ?? latestGraphRef.current;
  const resolvedFlowgramData = flowgramData ?? latestFlowgramDataRef.current;
  const connectionOptions = connections;
  const connectionDefaults = useMemo<FlowgramConnectionDefaults>(() => {
    const anyConnectionId = connections[0]?.id ?? null;
    const modbusConnectionId =
      connections.find((connection) => isModbusConnectionType(connection.type))?.id ?? null;
    const serialConnectionId =
      connections.find((connection) => isSerialConnectionType(connection.type))?.id ?? null;
    const mqttConnectionId =
      connections.find((connection) => isMqttConnectionType(connection.type))?.id ?? null;
    const httpConnectionId =
      connections.find((connection) => isHttpConnectionType(connection.type))?.id ?? null;
    const barkConnectionId =
      connections.find((connection) => isBarkConnectionType(connection.type))?.id ?? null;

    return {
      any: anyConnectionId,
      modbus: modbusConnectionId,
      serial: serialConnectionId,
      mqtt: mqttConnectionId,
      http: httpConnectionId,
      bark: barkConnectionId,
    };
  }, [connections]);

  useEffect(() => {
    if (!editorCtx && resolvedFlowgramData) {
      initialFlowgramDataRef.current = resolvedFlowgramData;
    }
  }, [editorCtx, resolvedFlowgramData]);

  if (!initialFlowgramDataRef.current && resolvedFlowgramData) {
    initialFlowgramDataRef.current = resolvedFlowgramData;
  }

  const activeNodeIds = useMemo(() => new Set(runtimeState.activeNodeIds), [runtimeState.activeNodeIds]);
  const completedNodeIds = useMemo(
    () => new Set(runtimeState.completedNodeIds),
    [runtimeState.completedNodeIds],
  );
  const failedNodeIds = useMemo(() => new Set(runtimeState.failedNodeIds), [runtimeState.failedNodeIds]);
  const outputNodeIds = useMemo(() => new Set(runtimeState.outputNodeIds), [runtimeState.outputNodeIds]);
  const isWorkflowRuntimeMapped =
    workflowStatus === 'running' || workflowStatus === 'completed' || workflowStatus === 'failed';
  const isWorkflowActive =
    workflowStatus === 'deployed' ||
    workflowStatus === 'running' ||
    workflowStatus === 'completed' ||
    workflowStatus === 'failed';
  const workflowStatusLabel = getCanvasWorkflowStatusLabel(workflowStatus);
  const runtimeContextLabel = useMemo(() => {
    if (!isWorkflowActive) {
      return '运行上下文: 未绑定';
    }

    if (runtimeState.traceId) {
      return `运行上下文: ${runtimeState.lastEventType ?? 'deployed'} @ ${runtimeState.lastNodeId ?? '--'}`;
    }

    if (workflowStatus === 'deployed') {
      return '运行上下文: 已绑定，等待触发';
    }

    return `运行上下文: ${workflowStatusLabel}`;
  }, [
    isWorkflowActive,
    runtimeState.lastEventType,
    runtimeState.lastNodeId,
    runtimeState.traceId,
    workflowStatus,
    workflowStatusLabel,
  ]);

  const resolveNodeRuntimeStatus = useCallback(
    (nodeId: string): RuntimeNodeStatus => {
      if (!isWorkflowRuntimeMapped) {
        return 'idle';
      }

      if (failedNodeIds.has(nodeId)) {
        return 'failed';
      }

      if (activeNodeIds.has(nodeId)) {
        return 'running';
      }

      if (outputNodeIds.has(nodeId)) {
        return 'output';
      }

      if (completedNodeIds.has(nodeId)) {
        return 'completed';
      }

      return 'idle';
    },
    [activeNodeIds, completedNodeIds, failedNodeIds, isWorkflowRuntimeMapped, outputNodeIds],
  );

  const resolveLineRuntimeStatus = useCallback(
    (line: WorkflowLineEntity): RuntimeNodeStatus => {
      if (!isWorkflowRuntimeMapped) {
        return 'idle';
      }

      const fromId = line.from?.id;
      const toId = line.to?.id;
      if (!fromId) {
        return 'idle';
      }

      if ((toId && failedNodeIds.has(toId)) || failedNodeIds.has(fromId)) {
        return 'failed';
      }

      if (activeNodeIds.has(fromId) || (toId && activeNodeIds.has(toId))) {
        return 'running';
      }

      if ((toId && outputNodeIds.has(toId)) || outputNodeIds.has(fromId)) {
        return 'output';
      }

      if ((toId && completedNodeIds.has(toId)) || completedNodeIds.has(fromId)) {
        return 'completed';
      }

      return 'idle';
    },
    [activeNodeIds, completedNodeIds, failedNodeIds, isWorkflowRuntimeMapped, outputNodeIds],
  );

  const isFlowingLine = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) =>
      resolveLineRuntimeStatus(line) === 'running',
    [resolveLineRuntimeStatus],
  );

  const isErrorLine = useCallback(
    (_ctx: FreeLayoutPluginContext, fromPort: { node: FlowNodeEntity }, toPort?: { node: FlowNodeEntity }) =>
      isWorkflowRuntimeMapped &&
      Boolean(
        (fromPort?.node?.id && failedNodeIds.has(fromPort.node.id)) ||
          (toPort?.node?.id && failedNodeIds.has(toPort.node.id)),
      ),
    [failedNodeIds, isWorkflowRuntimeMapped],
  );

  const setLineClassName = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) => {
      const lineStatus = resolveLineRuntimeStatus(line);
      return lineStatus === 'idle' ? 'flowgram-line' : `flowgram-line flowgram-line--${lineStatus}`;
    },
    [resolveLineRuntimeStatus],
  );

  // 连接期 pin 类型校验：用户拖边瞬间被调用——pin schema 缓存命中且
  // 类型不兼容时返回 false 拒收，否则放行（缓存未命中也放行，部署期
  // pin_validator 作为 backstop 兜底）。
  const canAddLine = useCallback(
    (
      _ctx: FreeLayoutPluginContext,
      fromPort: { node: { id: string }; portID: string | number },
      toPort: { node: { id: string }; portID: string | number; hasError?: boolean },
      _lines: WorkflowLinesManager,
      silent?: boolean,
    ): boolean => {
      const result = checkConnection(
        fromPort.node.id,
        fromPort.portID,
        toPort.node.id,
        toPort.portID,
      );

      if (!result.allow && result.rejection && !silent) {
        applyConnectionRejectionFeedback(toPort, result.rejection);
      }

      return result.allow;
    },
    [],
  );

  const renderNodeCard = useCallback(
    (props: FlowgramNodeMaterialProps) => {
      const rawType = normalizeNodeKind(
        ((props.node.getExtInfo() ?? {}) as { nodeType?: string }).nodeType ?? props.node.flowNodeType,
      );
      if (rawType === 'subgraph' || rawType === 'loop') {
        return (
          <FlowgramContainerCard
            {...props}
            runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
            accentHex={accentHex}
            nodeCodeColor={nodeCodeColor}
          />
        );
      }
      return (
        <FlowgramNodeCard
          {...props}
          runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
          accentHex={accentHex}
          nodeCodeColor={nodeCodeColor}
        />
      );
    },
    [accentHex, nodeCodeColor, resolveNodeRuntimeStatus],
  );
  const materials = useMemo(
    () => ({
      renderDefaultNode: renderNodeCard,
    }),
    [renderNodeCard],
  );
  const buildCurrentWorkflowGraph = useCallback(
    (ctx: FreeLayoutPluginContext) => {
      if (!latestGraphRef.current) {
        return null;
      }

      const nextFlowgramGraph = ctx.document.toJSON();
      return toNazhWorkflowGraph(nextFlowgramGraph, latestGraphRef.current);
    },
    [],
  );

  const emitCurrentGraphChange = useCallback(
    (ctx: FreeLayoutPluginContext) => {
      try {
        const nextGraph = buildCurrentWorkflowGraph(ctx);
        if (!nextGraph) {
          return null;
        }

        const nextAstText = formatWorkflowGraph(nextGraph);
        onGraphChange(nextAstText);
        return nextAstText;
      } catch (error) {
        reportFlowgramError('FlowGram 当前工作流序列化失败', error);
        return null;
      }
    },
    [buildCurrentWorkflowGraph, onGraphChange, reportFlowgramError],
  );

  const syncSelectionState = useCallback(
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

        // 从选中节点切换到无选中时（关闭设置面板），延迟同步图变更以避免渲染期 setState
        if (hadPreviousSelection && !nextBusinessNode) {
          setTimeout(() => emitCurrentGraphChange(ctx), 0);
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
        return;
      }
    },
    [activeAiProviderId, aiProviders, connectionOptions, copilotParams, emitCurrentGraphChange, reportFlowgramError],
  );

  const applyExternalFlowgramGraph = useCallback(
    (ctx: FreeLayoutPluginContext, nextGraph: FlowgramWorkflowJSON) => {
      try {
        applyingExternalGraphRef.current = true;
        const operationContext = ctx as FreeLayoutPluginContext & {
          operation?: {
            fromJSON: (graph: FlowgramWorkflowJSON) => void;
          };
        };

        if (operationContext.operation) {
          operationContext.operation.fromJSON(nextGraph);
        } else {
          ctx.document.fromJSON(nextGraph);
        }

        syncSelectionState(ctx);
      } catch (error) {
        reportFlowgramError('FlowGram 外部数据载入失败', error);
      } finally {
        applyingExternalGraphRef.current = false;
      }
    },
    [reportFlowgramError, syncSelectionState],
  );

  const loadWorkflowGraph = useCallback(
    (nextGraph: WorkflowGraph) => {
      try {
        latestGraphRef.current = nextGraph;
        const nextFlowgramGraph = toFlowgramWorkflowJson(nextGraph);
        latestFlowgramDataRef.current = nextFlowgramGraph;

        if (syncTimerRef.current !== null) {
          window.clearTimeout(syncTimerRef.current);
          syncTimerRef.current = null;
        }

        if (!editorCtx) {
          initialFlowgramDataRef.current = nextFlowgramGraph;
          return;
        }

        pendingFitViewRef.current = true;
        applyExternalFlowgramGraph(editorCtx, nextFlowgramGraph);
      } catch (error) {
        reportFlowgramError('FlowGram 工作流导入失败', error);
      }
    },
    [applyExternalFlowgramGraph, editorCtx, reportFlowgramError],
  );

  useImperativeHandle(
    ref,
    () => ({
      isReady: () => Boolean(editorCtx),
      getCurrentWorkflowGraph: () =>
        editorCtx ? buildCurrentWorkflowGraph(editorCtx) : latestGraphRef.current,
      loadWorkflowGraph,
    }),
    [buildCurrentWorkflowGraph, editorCtx, loadWorkflowGraph],
  );

  const handleSaveCurrentGraph = useCallback(() => {
    if (!editorCtx) {
      return;
    }

    emitCurrentGraphChange(editorCtx);
  }, [editorCtx, emitCurrentGraphChange]);

  const handleDownloadCurrentGraph = useCallback(
    async (format: FlowDownloadFormat) => {
      if (!editorCtx) {
        return;
      }

      try {
        const downloadService = (editorCtx as FreeLayoutPluginContext & {
          get?: <T>(token: unknown) => T;
        }).get?.<FlowDownloadService>(FlowDownloadService) as unknown as
          | InternalFlowDownloadService
          | undefined;
        if (!downloadService) {
          return;
        }

        if (hasTauriRuntime()) {
          downloadService.setDownloading(true);

          try {
            const fileName = buildFlowgramExportFileName(workflowName, format);

            if (format === FlowDownloadFormat.JSON) {
              const json = downloadService.document.toJSON();
              const { content } = await downloadService.formatDataContent(json, format);
              const saved = await saveFlowgramExportFile(workspacePath ?? '', fileName, {
                text: content,
              });
              onStatusMessage?.(`已导出到 ${saved.filePath}`);
              return;
            }

            const imageUrl = await downloadService.exportImageService.export({
              format,
              watermarkSVG: downloadService.options?.watermarkSVG,
            });
            if (!imageUrl) {
              throw new Error('未能生成导出内容。');
            }

            const response = await fetch(imageUrl);
            if (!response.ok) {
              throw new Error(`导出内容读取失败: ${response.status}`);
            }

            const buffer = await response.arrayBuffer();
            const saved = await saveFlowgramExportFile(workspacePath ?? '', fileName, {
              bytes: Array.from(new Uint8Array(buffer)),
            });
            onStatusMessage?.(`已导出到 ${saved.filePath}`);
            return;
          } finally {
            downloadService.setDownloading(false);
          }
        }

        await downloadService.download({ format });
      } catch (error) {
        reportFlowgramError('FlowGram 导出失败', error);
      }
    },
    [editorCtx, onStatusMessage, reportFlowgramError, workflowName, workspacePath],
  );

  const handleEditorRef = useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      setEditorCtx(ctx);
    },
    [],
  );

  const handleAllLayersRendered = useCallback((ctx: FreeLayoutPluginContext) => {
    if (!pendingFitViewRef.current) {
      return;
    }

    pendingFitViewRef.current = false;
    void ctx.tools.fitView(true);
  }, []);
  const handleDragLineEnd = useCallback(
    async (ctx: FreeLayoutPluginContext, params: Parameters<typeof handleFlowgramDragLineEnd>[1]) => {
      try {
        await handleFlowgramDragLineEnd(ctx, params);
        syncSelectionState(ctx);
      } catch (error) {
        reportFlowgramError('FlowGram 连线处理失败', error);
      }
    },
    [reportFlowgramError, syncSelectionState],
  );

  useEffect(() => {
    void refreshCapabilitiesCache();
    return () => {
      if (syncTimerRef.current !== null) {
        window.clearTimeout(syncTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!editorCtx) {
      setIsReadonlyMode(false);
      return;
    }

    const selectionDisposable = editorCtx.document.selectServices.onSelectionChanged(() => {
      syncSelectionState(editorCtx);
    });

    syncSelectionState(editorCtx);

    return () => {
      selectionDisposable.dispose();
    };
  }, [editorCtx, syncSelectionState]);

  useEffect(() => {
    if (!editorCtx) {
      return;
    }

    setIsReadonlyMode(editorCtx.playground.config.readonly);
    const disposable = editorCtx.playground.config.onReadonlyOrDisabledChange(({ readonly }) => {
      setIsReadonlyMode(readonly);
      if (readonly) {
        syncSelectionState(editorCtx);
      }
    });

    return () => {
      disposable.dispose();
    };
  }, [editorCtx, syncSelectionState]);

  useEffect(() => {
    const nextFlowgramData = latestFlowgramDataRef.current;

    if (!editorCtx || !graph || !nextFlowgramData || !flowgramDataSignature) {
      return;
    }

    const currentGraphSignature = buildFlowgramGraphSignature(editorCtx.document.toJSON());
    if (currentGraphSignature === flowgramDataSignature) {
      return;
    }

    applyExternalFlowgramGraph(editorCtx, nextFlowgramData);
  }, [applyExternalFlowgramGraph, editorCtx, flowgramDataSignature, graph]);

  const handleContentChange = useCallback(
    (ctx: FreeLayoutPluginContext, event: WorkflowContentChangeEvent) => {
      try {
        if (applyingExternalGraphRef.current) {
          return;
        }

        if (event.type === WorkflowContentChangeType.META_CHANGE) {
          return;
        }

        // 节点生命周期事件触发 pin schema 缓存维护。refresh / invalidate
        // 都是 fire-and-forget——失败时缓存自动写 fallback Any/Any，部署期
        // 校验作为 backstop 兜底。
        if (
          event.type === WorkflowContentChangeType.ADD_NODE ||
          event.type === WorkflowContentChangeType.NODE_DATA_CHANGE
        ) {
          const entity = event.entity as { id?: string; getExtInfo?: () => unknown } | undefined;
          if (entity?.id && entity.getExtInfo) {
            const ext = (entity.getExtInfo() ?? {}) as {
              nodeType?: string;
              config?: unknown;
            };
            if (ext.nodeType) {
              void refreshNodePinSchema(
                entity.id,
                ext.nodeType,
                configToRecord(ext.config as never),
              );
            }
          }
        } else if (event.type === WorkflowContentChangeType.DELETE_NODE) {
          const entity = event.entity as { id?: string } | undefined;
          if (entity?.id) {
            invalidateNodePinSchema(entity.id);
          }
        }

        if (
          event.type === WorkflowContentChangeType.DELETE_NODE ||
          event.type === WorkflowContentChangeType.DELETE_LINE
        ) {
          ctx.playground.flush();
        }

        // 延迟状态更新，避免 FlowGram 在渲染期触发回调导致 setState 警告
        setTimeout(() => {
          setLastChange(event.type);
          syncSelectionState(ctx);

          if (!latestGraphRef.current) {
            return;
          }

          if (syncTimerRef.current !== null) {
            window.clearTimeout(syncTimerRef.current);
            syncTimerRef.current = null;
          }

          syncTimerRef.current = window.setTimeout(() => {
            emitCurrentGraphChange(ctx);
          }, 120);
        }, 0);
      } catch (error) {
        reportFlowgramError('FlowGram 内容同步失败', error);
      }
    },
    [syncSelectionState, emitCurrentGraphChange, reportFlowgramError],
  );

  function nextNodeId(prefix: string): string {
    const currentIds = new Set(
      editorCtx?.document.getAllNodes().map((node) => node.id) ?? Object.keys(resolvedGraph?.nodes ?? {}),
    );

    return allocateNodeId(prefix, currentIds);
  }

  function buildInsertionPosition(anchorNode: FlowNodeEntity | null) {
    if (!anchorNode) {
      return undefined;
    }

    const anchorPosition = anchorNode.getNodeMeta().position;
    if (!anchorPosition) {
      return undefined;
    }

    const branchOffset = anchorNode.lines.outputNodes.length * 168;

    return {
      x: anchorPosition.x + 320,
      y: anchorPosition.y + branchOffset,
    };
  }

  async function handleInsertNode(seed: NodeSeed, mode: 'standalone' | 'downstream') {
    if (!editorCtx || editorCtx.playground.config.readonly) {
      return;
    }

    try {
      const anchorNode = mode === 'downstream' ? selectedNodeRef.current : null;
      const nextId = nextNodeId(seed.idPrefix);
      const node = editorCtx.document.createWorkflowNodeByType(
        seed.kind,
        buildInsertionPosition(anchorNode),
        {
          id: nextId,
          type: seed.kind,
          data: resolveNodeData(seed, nextId, connectionDefaults),
        },
      );

      if (anchorNode) {
        const fromPort = getDefaultOutputPortId(anchorNode);
        editorCtx.document.linesManager.createLine({
          from: anchorNode.id,
          to: node.id,
          ...(fromPort ? { fromPort } : {}),
        });
      }

      editorCtx.document.selectServices.selectNode(node);
      await editorCtx.document.selectServices.selectNodeAndScrollToView(node, false);
      syncSelectionState(editorCtx);
    } catch (error) {
      reportFlowgramError('FlowGram 节点插入失败', error);
    }
  }

  const editorProps = useFlowgramEditorProps({
    initialData: initialFlowgramDataRef.current ??
      resolvedFlowgramData ?? {
        nodes: [],
        edges: [],
      },
    accentColor: accentHex,
    themeMode,
    connectionDefaults,
    materials,
    isFlowingLine,
    isErrorLine,
    setLineClassName,
    canAddLine,
    onContentChange: handleContentChange,
    onAllLayersRendered: handleAllLayersRendered,
    onDragLineEnd: handleDragLineEnd,
  });

  return (
    <section className="canvas-shell">
      {!resolvedGraph || !resolvedFlowgramData ? (
        <div className="canvas-empty">无有效流程</div>
      ) : (
        <FreeLayoutEditorProvider ref={handleEditorRef} {...editorProps}>
          <div className="flowgram-host">
            <div className="flowgram-workspace">
              <FlowgramNodeAddPanel
                connectionDefaults={connectionDefaults}
                hasSelection={hasSelection}
                disabled={isReadonlyMode}
                onInsertSeed={handleInsertNode}
              />

              <div className="flowgram-stage">
                <EditorRenderer className="flowgram-editor" />
                <FlowgramToolbar
                  canRun={Boolean(onRunRequested)}
                  canTestRun={canTestRun}
                  isWorkflowActive={isWorkflowActive}
                  minimapVisible={minimapVisible}
                  onToggleMinimap={() => setMinimapVisible((visible) => !visible)}
                  onRun={onRunRequested}
                  onStop={onStopRequested}
                  onTestRun={onTestRunRequested}
                  onDownload={handleDownloadCurrentGraph}
                />
                <div className="flowgram-overlay">
                  <span>{`工作流状态: ${workflowStatusLabel}`}</span>
                  <span>{lastChange ? `最近变更: ${lastChange}` : '未变更'}</span>
                  <span>{`${resolvedFlowgramData.nodes.length} nodes / ${resolvedFlowgramData.edges.length} edges`}</span>
                  <span>{runtimeContextLabel}</span>
                </div>
              </div>
            </div>
          </div>
        </FreeLayoutEditorProvider>
      )}
    </section>
  );
});
