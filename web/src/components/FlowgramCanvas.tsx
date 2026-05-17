import {
  EditorRenderer,
  FreeLayoutEditorProvider,
  FlowNodeBaseType,
  type FlowNodeEntity,
  type WorkflowJSON as FlowgramWorkflowJSON,
  type FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
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
import { FlowgramContextMenu, type ContextMenuState } from './flowgram/FlowgramContextMenu';
import type { FlowgramConnectionDefaults } from './flowgram/flowgram-node-library';
import { handleFlowgramDragLineEnd } from './flowgram/flowgram-line-panel';
import { useFlowgramEditorProps } from './flowgram/useFlowgramEditorProps';
import {
  formatWorkflowGraph,
  toFlowgramWorkflowJson,
  toNazhWorkflowGraph,
} from '../lib/flowgram';
import { refreshCapabilitiesCache } from '../lib/node-capabilities-cache';
import type { WorkflowGraph } from '../types';

// 从拆分模块导入
import {
  isSerialConnectionType,
  isCanConnectionType,
  isEthercatConnectionType,
  isModbusConnectionType,
  isMqttConnectionType,
  isHttpConnectionType,
  isBarkConnectionType,
  buildFlowgramGraphSignature,
  getCanvasWorkflowStatusLabel,
  describeFlowgramError,
} from './flowgram/flowgram-canvas-utils';
import { FlowgramToolbar } from './flowgram/FlowgramToolbar';
import { useFlowgramCanvasOps } from './flowgram/useFlowgramCanvasOps';
import { useFlowgramConnectionValidation } from './flowgram/useFlowgramConnectionValidation';
import { useFlowgramContentSync } from './flowgram/useFlowgramContentSync';
import { useFlowgramExport } from './flowgram/useFlowgramExport';
import { useFlowgramRuntimeDecorations } from './flowgram/useFlowgramRuntimeDecorations';
import { useFlowgramSelectionSync } from './flowgram/useFlowgramSelectionSync';
import type {
  FlowgramCanvasHandle,
  FlowgramCanvasProps,
} from './flowgram/FlowgramCanvas.types';

// 公开类型继续从本入口转出，保持调用方导入路径不变。
export type {
  CanvasEdgeOp,
  CanvasNodeOp,
  CanvasNodePatch,
  CanvasOps,
  FlowgramCanvasActions,
  FlowgramCanvasAppearance,
  FlowgramCanvasExportTarget,
  FlowgramCanvasHandle,
  FlowgramCanvasProps,
  FlowgramCanvasResources,
  FlowgramCanvasRuntime,
} from './flowgram/FlowgramCanvas.types';

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
  /// Copilot 增量添加节点时，追踪已创建节点的最大 Y 坐标，
  /// 避免逐个 add_node 事件导致节点重叠。
  const lastCopilotYRef = useRef(0);
  const initialFlowgramDataRef = useRef<FlowgramWorkflowJSON | null>(null);
  const pendingFitViewRef = useRef(true);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
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
    const canConnectionId =
      connections.find((connection) => isCanConnectionType(connection.type))?.id ?? null;
    const ethercatConnectionId =
      connections.find((connection) => isEthercatConnectionType(connection.type))?.id ?? null;

    return {
      any: anyConnectionId,
      modbus: modbusConnectionId,
      serial: serialConnectionId,
      mqtt: mqttConnectionId,
      http: httpConnectionId,
      bark: barkConnectionId,
      can: canConnectionId,
      ethercat: ethercatConnectionId,
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
  const {
    materials,
    isFlowingLine,
    isErrorLine,
    setLineClassName,
  } = useFlowgramRuntimeDecorations({
    editorCtx,
    runtimeState,
    workflowStatus,
    getEdgeHeatmap: runtime.getEdgeHeatmap,
    registerEdgeHeatUpdate: runtime.registerEdgeHeatUpdate,
    accentHex,
    nodeCodeColor,
  });
  const canAddLine = useFlowgramConnectionValidation();
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

  const syncSelectionState = useFlowgramSelectionSync({
    selectedNodeRef,
    setHasSelection,
    connectionOptions,
    aiProviders,
    activeAiProviderId,
    copilotParams,
    emitCurrentGraphChange,
    reportFlowgramError,
  });

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
        // 加载新图时重置 copilot 增量添加偏移
        lastCopilotYRef.current = 0;
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

  const {
    addCanvasOps,
    autoLayout: handleAutoLayout,
    getSelectedNode: getSelectedNodeSummary,
    updateCanvasNode,
    deleteCanvasNode,
    deleteCanvasEdge,
    insertNode: handleInsertNode,
  } = useFlowgramCanvasOps({
    editorCtx,
    selectedNodeRef,
    lastCopilotYRef,
    applyingExternalGraphRef,
    connectionDefaults,
    syncSelectionState,
    reportFlowgramError,
  });

  useImperativeHandle(
    ref,
    () => ({
      isReady: () => Boolean(editorCtx),
      getCurrentWorkflowGraph: () =>
        editorCtx ? buildCurrentWorkflowGraph(editorCtx) : latestGraphRef.current,
      loadWorkflowGraph,
      addCanvasOps,
      autoLayout: handleAutoLayout,
      getSelectedNode: getSelectedNodeSummary,
      updateCanvasNode,
      deleteCanvasNode,
      deleteCanvasEdge,
    }),
    [addCanvasOps, buildCurrentWorkflowGraph, deleteCanvasEdge, deleteCanvasNode, editorCtx, getSelectedNodeSummary, handleAutoLayout, loadWorkflowGraph, updateCanvasNode],
  );

  const handleSaveCurrentGraph = useCallback(() => {
    if (!editorCtx) {
      return;
    }

    emitCurrentGraphChange(editorCtx);
  }, [editorCtx, emitCurrentGraphChange]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      if (!editorCtx) return;

      const nodeEl = (e.target as HTMLElement).closest('[data-node-id]');
      const nodeId = nodeEl?.getAttribute('data-node-id');
      const entity = nodeId
        ? (editorCtx.document.getNode(nodeId) as FlowNodeEntity | undefined)
        : undefined;

      if (entity && entity.flowNodeType !== FlowNodeBaseType.ROOT) {
        editorCtx.document.selectServices.selectNode(entity);
        const ext = (entity.getExtInfo() ?? {}) as { connection_id?: string };
        setContextMenu({ x: e.clientX, y: e.clientY, target: 'node', connectionId: ext.connection_id });
      } else {
        setContextMenu({ x: e.clientX, y: e.clientY, target: 'canvas' });
      }
    },
    [editorCtx],
  );

  const handleDownloadCurrentGraph = useFlowgramExport({
    editorCtx,
    workflowName,
    workspacePath,
    onStatusMessage,
    reportFlowgramError,
  });

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

  const handleContentChange = useFlowgramContentSync({
    applyingExternalGraphRef,
    latestGraphRef,
    syncTimerRef,
    setLastChange,
    syncSelectionState,
    emitCurrentGraphChange,
    reportFlowgramError,
  });

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
    <section className="canvas-shell" onContextMenu={handleContextMenu}>
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
      {contextMenu ? (
        <FlowgramContextMenu
          state={contextMenu}
          editorCtx={editorCtx}
          onClose={() => setContextMenu(null)}
        />
      ) : null}
    </section>
  );
});
