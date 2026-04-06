import {
  EditorRenderer,
  FreeLayoutEditorProvider,
  FlowNodeBaseType,
  FlowNodeEntity,
  WorkflowContentChangeType,
  WorkflowLineEntity,
  type WorkflowJSON as FlowgramWorkflowJSON,
  type WorkflowContentChangeEvent,
  WorkflowNodeEntity,
  WorkflowNodeRenderer,
  useClientContext,
  usePlaygroundTools,
  useService,
  type FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
import { NodeIntoContainerService } from '@flowgram.ai/free-container-plugin';
import { WorkflowGroupService } from '@flowgram.ai/free-group-plugin';
import { PanelManager } from '@flowgram.ai/panel-manager-plugin';
import { usePanelManager } from '@flowgram.ai/panel-manager-plugin';
import { type CSSProperties, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { MinimapRender } from '@flowgram.ai/minimap-plugin';

import { FlowgramNodeAddPanel } from './flowgram/FlowgramNodeAddPanel';
import { FLOWGRAM_NODE_SETTINGS_PANEL_KEY } from './flowgram/FlowgramNodeSettingsPanel';
import { FLOWGRAM_RUNTIME_PANEL_KEY } from './flowgram/FlowgramRuntimePanel';
import {
  resolveNodeData,
  type NodeSeed,
} from './flowgram/flowgram-node-library';
import { handleFlowgramDragLineEnd } from './flowgram/flowgram-line-panel';
import { useFlowgramEditorProps } from './flowgram/useFlowgramEditorProps';
import {
  formatWorkflowGraph,
  toFlowgramWorkflowJson,
  toNazhWorkflowGraph,
} from '../lib/flowgram';
import type { WorkflowGraph, WorkflowRuntimeState, WorkflowWindowStatus } from '../types';

interface FlowgramCanvasProps {
  graph: WorkflowGraph | null;
  reloadVersion: number;
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  onGraphChange: (nextAstText: string) => void;
}

interface FlowgramNodeMaterialProps {
  node: FlowNodeEntity;
  activated?: boolean;
  runtimeStatus?: RuntimeNodeStatus;
}

type RuntimeNodeStatus = 'idle' | 'running' | 'completed' | 'failed' | 'output';

interface FlowgramToolbarProps {
  canDeleteSelection: boolean;
  onDeleteSelection: () => void;
}

const FLOWGRAM_BUTTON_STYLE: CSSProperties = {
  border: '1px solid var(--toolbar-border)',
  borderRadius: '4px',
  cursor: 'pointer',
  padding: '4px 8px',
  minHeight: 26,
  height: 26,
  lineHeight: 1,
  fontSize: 12,
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  whiteSpace: 'nowrap',
  color: 'var(--toolbar-text)',
  background: 'var(--toolbar-bg)',
  boxShadow: 'none',
  transform: 'none',
  transition: 'none',
};

const FLOWGRAM_TOOLS_STYLE: CSSProperties = {
  position: 'absolute',
  zIndex: 10,
  left: 16,
  bottom: 16,
  display: 'flex',
  alignItems: 'center',
  flexWrap: 'wrap',
  gap: 8,
  maxWidth: 'calc(100% - 164px)',
};

const FLOWGRAM_ZOOM_STYLE: CSSProperties = {
  ...FLOWGRAM_BUTTON_STYLE,
  cursor: 'default',
  width: 40,
};

const FLOWGRAM_MINIMAP_CANVAS_WIDTH = 110;
const FLOWGRAM_MINIMAP_CANVAS_HEIGHT = 76;
const FLOWGRAM_MINIMAP_PANEL_PADDING = 4;

const FLOWGRAM_MINIMAP_WRAPPER_STYLE: CSSProperties = {
  position: 'absolute',
  right: 16,
  bottom: 72,
  zIndex: 10,
  width: 118,
  height: FLOWGRAM_MINIMAP_CANVAS_HEIGHT + FLOWGRAM_MINIMAP_PANEL_PADDING * 2,
};

const FLOWGRAM_MINIMAP_CONTAINER_STYLE: CSSProperties = {
  pointerEvents: 'auto',
  position: 'relative',
  top: 'unset',
  right: 'unset',
  bottom: 'unset',
  left: 'unset',
};

const FLOWGRAM_MINIMAP_PANEL_STYLE: CSSProperties = {
  width: 118,
  height: FLOWGRAM_MINIMAP_CANVAS_HEIGHT + FLOWGRAM_MINIMAP_PANEL_PADDING * 2,
  padding: FLOWGRAM_MINIMAP_PANEL_PADDING,
  boxSizing: 'border-box',
};

const FLOWGRAM_MINIMAP_INACTIVE_STYLE = {
  opacity: 1,
  scale: 1,
  translateX: 0,
  translateY: 0,
} as const;

const BEZIER_LINE_TYPE = 0;

function getCanvasWorkflowStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'preview':
      return '预览';
    case 'idle':
      return '未运行';
    case 'deployed':
      return '已部署';
    case 'running':
      return '运行中';
    case 'completed':
      return '已完成';
    case 'failed':
      return '失败';
  }
}

function getCanvasWorkflowStatusPillClass(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'running':
      return 'runtime-pill--running';
    case 'failed':
      return 'runtime-pill--failed';
    case 'completed':
    case 'deployed':
      return 'runtime-pill--ready';
    case 'idle':
    case 'preview':
      return 'runtime-pill--idle';
  }
}

function normalizeCanvasCoordinate(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return null;
  }

  return Math.round(value * 100) / 100;
}

function normalizeFlowgramEdge(
  edge: FlowgramWorkflowJSON['edges'][number],
) {
  return {
    sourceNodeID: edge.sourceNodeID,
    targetNodeID: edge.targetNodeID,
    sourcePortID: edge.sourcePortID ?? null,
    targetPortID: edge.targetPortID ?? null,
  };
}

interface NormalizedFlowgramNode {
  id: string;
  type: string | number;
  meta: unknown;
  data: unknown;
  blocks: NormalizedFlowgramNode[];
  edges: ReturnType<typeof normalizeFlowgramEdge>[];
}

function normalizeFlowgramValue(value: unknown): unknown {
  if (typeof value === 'number') {
    return normalizeCanvasCoordinate(value);
  }

  if (Array.isArray(value)) {
    return value.map(normalizeFlowgramValue);
  }

  if (!value || typeof value !== 'object') {
    return value ?? null;
  }

  return Object.keys(value)
    .sort((left, right) => left.localeCompare(right))
    .reduce<Record<string, unknown>>((acc, key) => {
      acc[key] = normalizeFlowgramValue((value as Record<string, unknown>)[key]);
      return acc;
    }, {});
}

function normalizeFlowgramNode(
  node: FlowgramWorkflowJSON['nodes'][number],
): NormalizedFlowgramNode {
  return {
    id: node.id,
    type: node.type,
    meta: normalizeFlowgramValue(node.meta ?? {}),
    data: normalizeFlowgramValue(node.data ?? {}),
    blocks: (node.blocks ?? [])
      .map(normalizeFlowgramNode)
      .sort((left, right) => left.id.localeCompare(right.id)),
    edges: (node.edges ?? [])
      .map(normalizeFlowgramEdge)
      .sort((left, right) => {
        const sourceCompare = left.sourceNodeID.localeCompare(right.sourceNodeID);
        if (sourceCompare !== 0) {
          return sourceCompare;
        }

        return left.targetNodeID.localeCompare(right.targetNodeID);
      }),
  };
}

function buildFlowgramGraphSignature(graph: FlowgramWorkflowJSON): string {
  const normalizedNodes = graph.nodes
    .map(normalizeFlowgramNode)
    .sort((left, right) => left.id.localeCompare(right.id));
  const normalizedEdges = [...graph.edges]
    .map(normalizeFlowgramEdge)
    .sort((left, right) => {
      const sourceCompare = left.sourceNodeID.localeCompare(right.sourceNodeID);
      return sourceCompare !== 0 ? sourceCompare : left.targetNodeID.localeCompare(right.targetNodeID);
    });

  return JSON.stringify({
    nodes: normalizedNodes,
    edges: normalizedEdges,
  });
}

function isBusinessFlowNode(node: FlowNodeEntity | null): node is FlowNodeEntity {
  if (!node || node.flowNodeType === FlowNodeBaseType.GROUP) {
    return false;
  }

  const rawData = (node.getExtInfo() ?? {}) as {
    nodeType?: string;
  };

  return rawData.nodeType === 'native' || rawData.nodeType === 'rhai' || node.flowNodeType === 'native' || node.flowNodeType === 'rhai';
}

function FlowgramNodeCard(props: FlowgramNodeMaterialProps) {
  const rawData = props.node.getExtInfo() as
    | {
        label?: string;
        nodeType?: string;
        connectionId?: string | null;
        aiDescription?: string | null;
        timeoutMs?: number | null;
        config?: {
          message?: string;
          script?: string;
        };
      }
    | undefined;

  const nodeType = rawData?.nodeType ?? props.node.flowNodeType;
  const runtimeStatus = props.runtimeStatus ?? 'idle';
  const preview =
    nodeType === 'native'
      ? rawData?.config?.message ?? 'Native I/O passthrough'
      : rawData?.config?.script ?? 'Rhai business script';

  return (
    <WorkflowNodeRenderer
      node={props.node}
      className={`flowgram-card flowgram-card--${nodeType} flowgram-card--${runtimeStatus} ${props.activated ? 'is-activated' : ''}`}
      portClassName="flowgram-card__port"
      portBackgroundColor="#08111d"
      portPrimaryColor={nodeType === 'native' ? '#ff8f3a' : '#67b4ff'}
      portSecondaryColor="rgba(236, 244, 255, 0.4)"
      portErrorColor="#ff6d73"
    >
      <div data-flow-editor-selectable="false" className="flowgram-card__body" draggable={false}>
        <div className="flowgram-card__topline">
          <span className="flowgram-card__type">{nodeType}</span>
          {runtimeStatus !== 'idle' ? (
            <span className={`flowgram-card__runtime flowgram-card__runtime--${runtimeStatus}`}>
              {runtimeStatus}
            </span>
          ) : null}
        </div>
        <strong>{rawData?.label ?? props.node.id}</strong>
        <p className="flowgram-card__preview">{preview}</p>
        <div className="flowgram-card__meta">
          <span>{rawData?.connectionId ? `conn: ${rawData.connectionId}` : 'logic-only node'}</span>
          <span>{rawData?.timeoutMs ? `${rawData.timeoutMs} ms timeout` : 'no timeout'}</span>
        </div>
        {rawData?.aiDescription ? <p className="flowgram-card__hint">{rawData.aiDescription}</p> : null}
      </div>
    </WorkflowNodeRenderer>
  );
}

function FlowgramToolbar({ canDeleteSelection, onDeleteSelection }: FlowgramToolbarProps) {
  const { document, history } = useClientContext();
  const tools = usePlaygroundTools({
    minZoom: 0.24,
    maxZoom: 2,
  });
  const panelManager = usePanelManager();
  const groupService = useService(WorkflowGroupService);
  const nodeIntoContainerService = useService(NodeIntoContainerService);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [canGroupSelection, setCanGroupSelection] = useState(false);
  const [selectedGroupNode, setSelectedGroupNode] = useState<FlowNodeEntity | null>(null);
  const [selectedMovableNode, setSelectedMovableNode] = useState<FlowNodeEntity | null>(null);

  useEffect(() => {
    if (!history?.undoRedoService) {
      setCanUndo(false);
      setCanRedo(false);
      return;
    }

    const syncHistoryState = () => {
      setCanUndo(history.canUndo());
      setCanRedo(history.canRedo());
    };

    syncHistoryState();

    const disposable = history.undoRedoService.onChange(syncHistoryState);
    return () => disposable.dispose();
  }, [history]);

  useEffect(() => {
    const syncSelectionState = () => {
      const selectedNodes = document.selectServices.selectedNodes;
      const nextSelectedGroup =
        selectedNodes.length === 1 && selectedNodes[0].flowNodeType === FlowNodeBaseType.GROUP
          ? selectedNodes[0]
          : null;
      const nextSelectedMovableNode =
        selectedNodes.length === 1 &&
        selectedNodes[0].parent?.flowNodeType === FlowNodeBaseType.GROUP &&
        nodeIntoContainerService.canMoveOutContainer(selectedNodes[0])
          ? selectedNodes[0]
          : null;

      setCanGroupSelection(
        selectedNodes.length > 1 && selectedNodes.every((node) => node.flowNodeType !== FlowNodeBaseType.GROUP),
      );
      setSelectedGroupNode(nextSelectedGroup);
      setSelectedMovableNode(nextSelectedMovableNode);
    };

    syncSelectionState();

    const dispose = document.selectServices.onSelectionChanged(syncSelectionState);
    return () => dispose.dispose();
  }, [document, nodeIntoContainerService]);

  async function handleGroupSelection() {
    const selectedNodes = document.selectServices.selectedNodes.filter(
      (node) => node.flowNodeType !== FlowNodeBaseType.GROUP,
    );

    if (selectedNodes.length < 2) {
      return;
    }

    const groupNode = groupService.createGroup(selectedNodes);

    if (!groupNode) {
      return;
    }

    document.selectServices.selectNode(groupNode);
    await document.selectServices.selectNodeAndScrollToView(groupNode, false);
  }

  function handleUngroupSelection() {
    if (!selectedGroupNode) {
      return;
    }

    groupService.ungroup(selectedGroupNode);
  }

  async function handleMoveOutContainer() {
    if (!selectedMovableNode) {
      return;
    }

    await nodeIntoContainerService.moveOutContainer({
      node: selectedMovableNode,
    });
  }

  return (
    <div style={FLOWGRAM_TOOLS_STYLE} data-flow-editor-selectable="false">
      <button type="button" style={FLOWGRAM_BUTTON_STYLE} onClick={() => tools.zoomin()}>
        放大
      </button>
      <button type="button" style={FLOWGRAM_BUTTON_STYLE} onClick={() => tools.zoomout()}>
        缩小
      </button>
      <span style={FLOWGRAM_ZOOM_STYLE}>{Math.floor(tools.zoom * 100)}%</span>
      <button type="button" style={FLOWGRAM_BUTTON_STYLE} onClick={() => tools.fitView()}>
        适配
      </button>
      <button type="button" style={FLOWGRAM_BUTTON_STYLE} onClick={() => void tools.autoLayout()}>
        布局
      </button>
      <button type="button" style={FLOWGRAM_BUTTON_STYLE} onClick={() => tools.switchLineType()}>
        {tools.lineType === BEZIER_LINE_TYPE ? '贝塞尔' : '折线'}
      </button>
      <button
        type="button"
        style={{
          ...FLOWGRAM_BUTTON_STYLE,
          cursor: canUndo ? 'pointer' : 'not-allowed',
          color: canUndo ? 'var(--toolbar-text)' : 'var(--toolbar-disabled)',
        }}
        onClick={() => void history.undo()}
        disabled={!canUndo}
      >
        撤销
      </button>
      <button
        type="button"
        style={{
          ...FLOWGRAM_BUTTON_STYLE,
          cursor: canRedo ? 'pointer' : 'not-allowed',
          color: canRedo ? 'var(--toolbar-text)' : 'var(--toolbar-disabled)',
        }}
        onClick={() => void history.redo()}
        disabled={!canRedo}
      >
        重做
      </button>
      <button
        type="button"
        style={{
          ...FLOWGRAM_BUTTON_STYLE,
          cursor: canGroupSelection ? 'pointer' : 'not-allowed',
          color: canGroupSelection ? 'var(--toolbar-text)' : 'var(--toolbar-disabled)',
        }}
        onClick={() => void handleGroupSelection()}
        disabled={!canGroupSelection}
      >
        分组
      </button>
      <button
        type="button"
        style={{
          ...FLOWGRAM_BUTTON_STYLE,
          cursor: selectedGroupNode ? 'pointer' : 'not-allowed',
          color: selectedGroupNode ? 'var(--toolbar-text)' : 'var(--toolbar-disabled)',
        }}
        onClick={handleUngroupSelection}
        disabled={!selectedGroupNode}
      >
        解组
      </button>
      <button
        type="button"
        style={{
          ...FLOWGRAM_BUTTON_STYLE,
          cursor: selectedMovableNode ? 'pointer' : 'not-allowed',
          color: selectedMovableNode ? 'var(--toolbar-text)' : 'var(--toolbar-disabled)',
        }}
        onClick={() => void handleMoveOutContainer()}
        disabled={!selectedMovableNode}
      >
        移出容器
      </button>
      <button
        type="button"
        style={FLOWGRAM_BUTTON_STYLE}
        onClick={() => panelManager.open(FLOWGRAM_RUNTIME_PANEL_KEY, 'bottom')}
      >
        预检
      </button>
      <button
        type="button"
        style={{
          ...FLOWGRAM_BUTTON_STYLE,
          cursor: canDeleteSelection ? 'pointer' : 'not-allowed',
          color: canDeleteSelection ? 'var(--toolbar-text)' : 'var(--toolbar-disabled)',
        }}
        onClick={onDeleteSelection}
        disabled={!canDeleteSelection}
      >
        删除
      </button>
    </div>
  );
}

function FlowgramMinimap() {
  return (
    <div style={FLOWGRAM_MINIMAP_WRAPPER_STYLE} data-flow-editor-selectable="false">
      <MinimapRender
        containerStyles={FLOWGRAM_MINIMAP_CONTAINER_STYLE}
        panelStyles={FLOWGRAM_MINIMAP_PANEL_STYLE}
        inactiveStyle={FLOWGRAM_MINIMAP_INACTIVE_STYLE}
      />
    </div>
  );
}

export function FlowgramCanvas({
  graph,
  reloadVersion,
  runtimeState,
  workflowStatus,
  onGraphChange,
}: FlowgramCanvasProps) {
  const [lastChange, setLastChange] = useState<string | null>(null);
  const [editorCtx, setEditorCtx] = useState<FreeLayoutPluginContext | null>(null);
  const [hasSelection, setHasSelection] = useState(false);
  const syncTimerRef = useRef<number | null>(null);
  const selectedNodeRef = useRef<FlowNodeEntity | null>(null);
  const latestGraphRef = useRef<WorkflowGraph | null>(graph);
  const applyingExternalGraphRef = useRef(false);
  const initialFlowgramDataRef = useRef<FlowgramWorkflowJSON | null>(null);
  const pendingFitViewRef = useRef(true);
  const lastReloadVersionRef = useRef(reloadVersion);
  const connectionOptions = graph?.connections ?? [];
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
  const primaryConnectionId = graph?.connections?.[0]?.id ?? null;

  useEffect(() => {
    latestGraphRef.current = graph;
  }, [graph]);

  useEffect(() => {
    latestFlowgramDataRef.current = flowgramData;
  }, [flowgramData]);

  useEffect(() => {
    if (reloadVersion !== lastReloadVersionRef.current) {
      lastReloadVersionRef.current = reloadVersion;
      pendingFitViewRef.current = true;
    }
  }, [reloadVersion]);

  useEffect(() => {
    if (!editorCtx) {
      initialFlowgramDataRef.current = flowgramData;
    }
  }, [editorCtx, flowgramData]);

  if (!initialFlowgramDataRef.current && flowgramData) {
    initialFlowgramDataRef.current = flowgramData;
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
  const workflowStatusLabel = getCanvasWorkflowStatusLabel(workflowStatus);
  const workflowStatusPillClass = getCanvasWorkflowStatusPillClass(workflowStatus);

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

      const fromId = line.from.id;
      const toId = line.to?.id;

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
      (failedNodeIds.has(fromPort.node.id) || Boolean(toPort && failedNodeIds.has(toPort.node.id))),
    [failedNodeIds, isWorkflowRuntimeMapped],
  );

  const setLineClassName = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) => {
      const lineStatus = resolveLineRuntimeStatus(line);
      return lineStatus === 'idle' ? 'flowgram-line' : `flowgram-line flowgram-line--${lineStatus}`;
    },
    [resolveLineRuntimeStatus],
  );

  const renderNodeCard = useCallback(
    (props: FlowgramNodeMaterialProps) => (
      <FlowgramNodeCard
        {...props}
        runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
      />
    ),
    [resolveNodeRuntimeStatus],
  );
  const materials = useMemo(
    () => ({
      renderDefaultNode: renderNodeCard,
    }),
    [renderNodeCard],
  );
  const canDeleteSelection = Boolean(editorCtx && editorCtx.selection.selection.length > 0);
  const syncSelectionState = useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      if (!ctx) {
        selectedNodeRef.current = null;
        setHasSelection(false);
        return;
      }

      const selectionService = ctx.document.selectServices;
      const selectedNodes = selectionService.selectedNodes;
      const nextSelectedNode = selectedNodes.length === 1 ? selectedNodes[0] : null;
      const nextBusinessNode = isBusinessFlowNode(nextSelectedNode) ? nextSelectedNode : null;

      selectedNodeRef.current = nextBusinessNode;
      setHasSelection(Boolean(nextBusinessNode));

      const panelManager = (ctx as FreeLayoutPluginContext & {
        get?: <T>(token: unknown) => T;
      }).get?.<PanelManager>(PanelManager);

      if (!panelManager) {
        return;
      }

      if (nextBusinessNode) {
        panelManager.open(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right', {
          props: {
            nodeId: nextBusinessNode.id,
            connections: connectionOptions,
          },
        });
        return;
      }

      panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right');
    },
    [connectionOptions],
  );

  const handleEditorRef = useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      setEditorCtx(ctx);
    },
    [],
  );

  const applyExternalFlowgramGraph = useCallback(
    (ctx: FreeLayoutPluginContext, nextGraph: FlowgramWorkflowJSON) => {
      applyingExternalGraphRef.current = true;

      try {
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
      } finally {
        applyingExternalGraphRef.current = false;
      }
    },
    [syncSelectionState],
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
      await handleFlowgramDragLineEnd(ctx, params);
      syncSelectionState(ctx);
    },
    [syncSelectionState],
  );

  useEffect(() => {
    return () => {
      if (syncTimerRef.current !== null) {
        window.clearTimeout(syncTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!editorCtx) {
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
    const nextFlowgramData = latestFlowgramDataRef.current;

    if (!editorCtx || !nextFlowgramData || !flowgramDataSignature) {
      return;
    }

    const currentGraphSignature = buildFlowgramGraphSignature(editorCtx.document.toJSON());
    if (currentGraphSignature === flowgramDataSignature) {
      return;
    }

    applyExternalFlowgramGraph(editorCtx, nextFlowgramData);
  }, [applyExternalFlowgramGraph, editorCtx, flowgramDataSignature, reloadVersion]);

  function handleContentChange(
    ctx: FreeLayoutPluginContext,
    event: WorkflowContentChangeEvent,
  ) {
    if (applyingExternalGraphRef.current) {
      return;
    }

    if (event.type === WorkflowContentChangeType.META_CHANGE) {
      return;
    }

    if (
      event.type === WorkflowContentChangeType.DELETE_NODE ||
      event.type === WorkflowContentChangeType.DELETE_LINE
    ) {
      ctx.playground.flush();
    }

    setLastChange(event.type);
    syncSelectionState(ctx);

    if (!latestGraphRef.current) {
      return;
    }

    if (syncTimerRef.current !== null) {
      window.clearTimeout(syncTimerRef.current);
    }

    syncTimerRef.current = window.setTimeout(() => {
      const nextFlowgramGraph = ctx.document.toJSON();
      const nextGraph = toNazhWorkflowGraph(nextFlowgramGraph, latestGraphRef.current as WorkflowGraph);
      const nextAstText = formatWorkflowGraph(nextGraph);
      onGraphChange(nextAstText);
    }, 120);
  }

  function nextNodeId(prefix: string): string {
    const currentIds = new Set(
      editorCtx?.document.getAllNodes().map((node) => node.id) ?? Object.keys(graph?.nodes ?? {}),
    );

    let index = 1;
    while (currentIds.has(`${prefix}_${index}`)) {
      index += 1;
    }

    return `${prefix}_${index}`;
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
    if (!editorCtx) {
      return;
    }

    const anchorNode = mode === 'downstream' ? selectedNodeRef.current : null;
    const nextId = nextNodeId(seed.idPrefix);
    const node = editorCtx.document.createWorkflowNodeByType(
      seed.kind,
      buildInsertionPosition(anchorNode),
      {
      id: nextId,
      type: seed.kind,
      data: resolveNodeData(seed, nextId, primaryConnectionId),
      },
    );

    if (anchorNode) {
      editorCtx.document.linesManager.createLine({
        from: anchorNode.id,
        to: node.id,
      });
    }

    editorCtx.document.selectServices.selectNode(node);
    await editorCtx.document.selectServices.selectNodeAndScrollToView(node, false);
    syncSelectionState(editorCtx);
  }

  async function handleAutoLayout() {
    if (!editorCtx) {
      return;
    }

    await editorCtx.tools.autoLayout();
    await editorCtx.tools.fitView(true);
  }

  async function handleFitView() {
    if (!editorCtx) {
      return;
    }

    await editorCtx.tools.fitView(true);
  }

  function handleDeleteSelection() {
    if (!editorCtx) {
      return;
    }

    editorCtx.selection.selection.forEach((entity) => {
      if (entity instanceof WorkflowNodeEntity) {
        if (!editorCtx.document.canRemove(entity)) {
          return;
        }

        const nodeMeta = entity.getNodeMeta();
        const subCanvas = nodeMeta.subCanvas?.(entity);
        if (subCanvas?.isCanvas) {
          subCanvas.parentNode.dispose();
          return;
        }

        entity.dispose();
        return;
      }

      if (entity instanceof WorkflowLineEntity) {
        if (!editorCtx.document.linesManager.canRemove(entity)) {
          return;
        }

        entity.dispose();
      }
    });

    editorCtx.selection.selection = editorCtx.selection.selection.filter((entity) => !entity.disposed);
    editorCtx.playground.flush();
    syncSelectionState(editorCtx);
  }

  const editorProps = useFlowgramEditorProps({
    initialData: initialFlowgramDataRef.current ??
      flowgramData ?? {
        nodes: [],
        edges: [],
      },
    primaryConnectionId,
    materials,
    isFlowingLine,
    isErrorLine,
    setLineClassName,
    onContentChange: handleContentChange,
    onAllLayersRendered: handleAllLayersRendered,
    onDragLineEnd: handleDragLineEnd,
  });

  return (
    <section className="canvas-shell">
      <div className="canvas-header">
        <div>
          <p className="eyebrow">FlowGram Editor</p>
          <h2>工业画布编辑器</h2>
        </div>
        <div className={`runtime-pill ${workflowStatusPillClass}`}>{workflowStatusLabel}</div>
      </div>

      {!graph || !flowgramData ? (
        <div className="canvas-empty">无有效流程</div>
      ) : (
        <FreeLayoutEditorProvider ref={handleEditorRef} {...editorProps}>
          <div className="flowgram-workspace">
            <FlowgramNodeAddPanel
              primaryConnectionId={primaryConnectionId}
              hasSelection={hasSelection}
              onInsertSeed={handleInsertNode}
            />

            <div className="flowgram-host">
              <EditorRenderer className="flowgram-editor" />
              <FlowgramToolbar
                canDeleteSelection={canDeleteSelection}
                onDeleteSelection={handleDeleteSelection}
              />
              <FlowgramMinimap />
              <div className="flowgram-overlay">
                <span>{`工作流状态: ${workflowStatusLabel}`}</span>
                <span>{lastChange ? `最近变更: ${lastChange}` : '未变更'}</span>
                <span>{`${flowgramData.nodes.length} nodes / ${flowgramData.edges.length} edges`}</span>
                <span>
                  {isWorkflowRuntimeMapped && runtimeState.traceId
                    ? `运行态: ${runtimeState.lastEventType ?? 'idle'} @ ${runtimeState.lastNodeId ?? '--'}`
                    : '运行态: non-runtime'}
                </span>
              </div>
            </div>
          </div>
        </FreeLayoutEditorProvider>
      )}
    </section>
  );
}
