import {
  type FlowNodeEntity,
  type FreeLayoutPluginContext,
  type WorkflowLineEntity,
} from '@flowgram.ai/free-layout-editor';
import { useCallback, useEffect, useMemo, useRef } from 'react';

import {
  edgeHeatLevel,
  findEdgeHeatEntry,
  type EdgeHeatMap,
} from '../../hooks/use-edge-heatmap';
import type { WorkflowRuntimeState, WorkflowWindowStatus } from '../../types';
import {
  FlowgramContainerCard,
  FlowgramNodeCard,
  type FlowgramNodeMaterialProps,
  type RuntimeNodeStatus,
} from './FlowgramNodeCards';
import { normalizeNodeKind } from './flowgram-node-library';

interface FlowgramLineRuntimeSnapshot {
  isWorkflowRuntimeMapped: boolean;
  activeNodeIds: Set<string>;
  completedNodeIds: Set<string>;
  failedNodeIds: Set<string>;
  outputNodeIds: Set<string>;
  getEdgeHeatmap: (() => EdgeHeatMap) | null;
}

interface UseFlowgramRuntimeDecorationsOptions {
  editorCtx: FreeLayoutPluginContext | null;
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  getEdgeHeatmap?: () => EdgeHeatMap;
  registerEdgeHeatUpdate?: (callback: (() => void) | null) => void;
  accentHex: string;
  nodeCodeColor: string;
}

function resolveLineRuntimeStatusFromSnapshot(
  line: WorkflowLineEntity,
  snapshot: FlowgramLineRuntimeSnapshot,
): RuntimeNodeStatus {
  if (!snapshot.isWorkflowRuntimeMapped) {
    return 'idle';
  }

  const fromId = line.info.from || line.from?.id;
  const toId = line.info.to || line.to?.id;
  if (!fromId) {
    return 'idle';
  }

  if ((toId && snapshot.failedNodeIds.has(toId)) || snapshot.failedNodeIds.has(fromId)) {
    return 'failed';
  }

  if (snapshot.activeNodeIds.has(fromId) || (toId && snapshot.activeNodeIds.has(toId))) {
    return 'running';
  }

  if ((toId && snapshot.outputNodeIds.has(toId)) || snapshot.outputNodeIds.has(fromId)) {
    return 'output';
  }

  if ((toId && snapshot.completedNodeIds.has(toId)) || snapshot.completedNodeIds.has(fromId)) {
    return 'completed';
  }

  return 'idle';
}

export function useFlowgramRuntimeDecorations({
  editorCtx,
  runtimeState,
  workflowStatus,
  getEdgeHeatmap,
  registerEdgeHeatUpdate,
  accentHex,
  nodeCodeColor,
}: UseFlowgramRuntimeDecorationsOptions) {
  const activeNodeIds = useMemo(() => new Set(runtimeState.activeNodeIds), [runtimeState.activeNodeIds]);
  const completedNodeIds = useMemo(
    () => new Set(runtimeState.completedNodeIds),
    [runtimeState.completedNodeIds],
  );
  const failedNodeIds = useMemo(() => new Set(runtimeState.failedNodeIds), [runtimeState.failedNodeIds]);
  const outputNodeIds = useMemo(() => new Set(runtimeState.outputNodeIds), [runtimeState.outputNodeIds]);
  const isWorkflowRuntimeMapped =
    workflowStatus === 'running' || workflowStatus === 'completed' || workflowStatus === 'failed';

  const lineRuntimeRef = useRef<FlowgramLineRuntimeSnapshot>({
    isWorkflowRuntimeMapped: false,
    activeNodeIds: new Set(),
    completedNodeIds: new Set(),
    failedNodeIds: new Set(),
    outputNodeIds: new Set(),
    getEdgeHeatmap: null,
  });
  lineRuntimeRef.current = {
    isWorkflowRuntimeMapped,
    activeNodeIds,
    completedNodeIds,
    failedNodeIds,
    outputNodeIds,
    getEdgeHeatmap: getEdgeHeatmap ?? null,
  };

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

  const isFlowingLine = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) =>
      resolveLineRuntimeStatusFromSnapshot(line, lineRuntimeRef.current) === 'running',
    [],
  );

  const isErrorLine = useCallback(
    (_ctx: FreeLayoutPluginContext, fromPort: { node: FlowNodeEntity }, toPort?: { node: FlowNodeEntity }) =>
      lineRuntimeRef.current.isWorkflowRuntimeMapped &&
      Boolean(
        (fromPort?.node?.id && lineRuntimeRef.current.failedNodeIds.has(fromPort.node.id)) ||
          (toPort?.node?.id && lineRuntimeRef.current.failedNodeIds.has(toPort.node.id)),
      ),
    [],
  );

  const setLineClassName = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) => {
      const snapshot = lineRuntimeRef.current;
      const lineStatus = resolveLineRuntimeStatusFromSnapshot(line, snapshot);
      const base = lineStatus === 'idle' ? 'flowgram-line' : `flowgram-line flowgram-line--${lineStatus}`;

      // ADR-0016：边热力图叠加（运行态下根据传输统计着色）。
      if (snapshot.getEdgeHeatmap) {
        const heatmap = snapshot.getEdgeHeatmap();
        const fromId = line.info.from || line.from?.id;
        const toId = line.info.to || line.to?.id;
        if (fromId && toId && heatmap.size > 0) {
          const entry = findEdgeHeatEntry(
            heatmap,
            fromId,
            line.info.fromPort,
            toId,
            line.info.toPort,
          );
          if (entry?.backpressure) {
            return `${base} flowgram-line--backpressure`;
          }
          if (entry && entry.transmitCount > 0) {
            return `${base} flowgram-line--heat-${edgeHeatLevel(entry.transmitCount)}`;
          }
        }
      }

      return base;
    },
    [],
  );

  useEffect(() => {
    if (!editorCtx || !registerEdgeHeatUpdate) {
      return;
    }
    const forceUpdateLines = () => {
      editorCtx.document.linesManager.forceUpdate();
    };
    registerEdgeHeatUpdate(forceUpdateLines);
    return () => {
      registerEdgeHeatUpdate(null);
    };
  }, [editorCtx, registerEdgeHeatUpdate]);

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
      const debugOutput = rawType === 'debugConsole'
        ? runtimeState.debugOutputs[props.node.id]
        : undefined;
      return (
        <FlowgramNodeCard
          {...props}
          runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
          debugOutput={debugOutput}
          accentHex={accentHex}
          nodeCodeColor={nodeCodeColor}
        />
      );
    },
    [accentHex, nodeCodeColor, resolveNodeRuntimeStatus, runtimeState.debugOutputs],
  );

  const materials = useMemo(
    () => ({
      renderDefaultNode: renderNodeCard,
    }),
    [renderNodeCard],
  );

  return {
    isWorkflowRuntimeMapped,
    isFlowingLine,
    isErrorLine,
    setLineClassName,
    materials,
  };
}
