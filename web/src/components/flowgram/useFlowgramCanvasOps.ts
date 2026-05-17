import {
  FlowNodeBaseType,
  type FlowNodeEntity,
  type FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
import { useCallback, type MutableRefObject } from 'react';

import { allocateNodeId } from '../../lib/workflow-node-id';
import { usesDynamicPorts } from './nodes/settings-shared';
import {
  getLogicNodeBranchDefinitions,
  normalizeNodeKind,
  resolveNodeData,
  type FlowgramConnectionDefaults,
  type NodeSeed,
} from './flowgram-node-library';
import type {
  CanvasNodeOp,
  CanvasNodePatch,
  CanvasOps,
} from './FlowgramCanvas.types';

interface UseFlowgramCanvasOpsOptions {
  editorCtx: FreeLayoutPluginContext | null;
  selectedNodeRef: MutableRefObject<FlowNodeEntity | null>;
  lastCopilotYRef: MutableRefObject<number>;
  applyingExternalGraphRef: MutableRefObject<boolean>;
  connectionDefaults: FlowgramConnectionDefaults;
  syncSelectionState: (ctx: FreeLayoutPluginContext | null) => void;
  reportFlowgramError: (title: string, error: unknown) => void;
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

export function useFlowgramCanvasOps({
  editorCtx,
  selectedNodeRef,
  lastCopilotYRef,
  applyingExternalGraphRef,
  connectionDefaults,
  syncSelectionState,
  reportFlowgramError,
}: UseFlowgramCanvasOpsOptions) {
  const addCanvasOps = useCallback(
    (ops: CanvasOps) => {
      if (!editorCtx || editorCtx.playground.config.readonly) {
        return;
      }

      try {
        applyingExternalGraphRef.current = true;

        // 简易拓扑排序：按边关系确定层级。
        const depthMap = new Map<string, number>();
        for (const node of ops.nodes) {
          depthMap.set(node.id, 0);
        }
        for (const edge of ops.edges) {
          const fromDepth = depthMap.get(edge.from) ?? 0;
          const toDepth = depthMap.get(edge.to);
          if (toDepth !== undefined) {
            depthMap.set(edge.to, Math.max(toDepth, fromDepth + 1));
          }
        }

        const byDepth = new Map<number, CanvasNodeOp[]>();
        for (const node of ops.nodes) {
          const depth = depthMap.get(node.id) ?? 0;
          let group = byDepth.get(depth);
          if (!group) {
            group = [];
            byDepth.set(depth, group);
          }
          group.push(node);
        }

        const anchorNode = selectedNodeRef.current;
        const baseX = anchorNode?.getNodeMeta().position?.x ?? 200;
        const baseY = lastCopilotYRef.current > 0
          ? lastCopilotYRef.current
          : (anchorNode?.getNodeMeta().position?.y ?? 300);

        const idToEntity = new Map<string, FlowNodeEntity>();
        const sortedDepths = [...byDepth.keys()].sort((a, b) => a - b);
        for (const depth of sortedDepths) {
          const group = byDepth.get(depth) ?? [];
          for (let i = 0; i < group.length; i++) {
            const op = group[i];
            const x = baseX + depth * 320;
            const y = baseY + i * 168;

            const seed: NodeSeed = {
              idPrefix: op.type,
              kind: normalizeNodeKind(op.type),
              label: op.label ?? '',
              connectionId: op.connection_id ?? null,
              timeoutMs: null,
              config: (op.config ?? {}) as NodeSeed['config'],
            };

            const node = editorCtx.document.createWorkflowNodeByType(
              seed.kind,
              { x, y },
              {
                id: op.id,
                type: seed.kind,
                data: resolveNodeData(seed, op.id, connectionDefaults),
              },
            );

            idToEntity.set(op.id, node);
          }
        }

        for (const edge of ops.edges) {
          editorCtx.document.linesManager.createLine({
            from: edge.from,
            to: edge.to,
            ...(edge.source_port_id ? { fromPort: edge.source_port_id } : {}),
            ...(edge.target_port_id ? { toPort: edge.target_port_id } : {}),
          });
        }

        if (ops.nodes.length > 0 && ops.edges.length === 0) {
          lastCopilotYRef.current = baseY + ops.nodes.length * 168;
        } else if (ops.nodes.length > 0) {
          const maxDepth = Math.max(...sortedDepths);
          const maxGroup = byDepth.get(maxDepth) ?? [];
          lastCopilotYRef.current = baseY + maxGroup.length * 168;
        }

        syncSelectionState(editorCtx);

        const lastNode = ops.nodes[ops.nodes.length - 1];
        if (lastNode) {
          const entity = idToEntity.get(lastNode.id);
          if (entity) {
            void editorCtx.document.selectServices.selectNodeAndScrollToView(entity, false);
          }
        }
      } catch (error) {
        reportFlowgramError('Copilot 画布节点添加失败', error);
      } finally {
        applyingExternalGraphRef.current = false;
      }
    },
    [
      applyingExternalGraphRef,
      connectionDefaults,
      editorCtx,
      lastCopilotYRef,
      reportFlowgramError,
      selectedNodeRef,
      syncSelectionState,
    ],
  );

  const autoLayout = useCallback(() => {
    if (editorCtx) {
      void editorCtx.tools.autoLayout();
    }
  }, [editorCtx]);

  const getSelectedNode = useCallback((): { id: string; type: string; label?: string } | null => {
    const node = selectedNodeRef.current;
    if (!node) {
      return null;
    }
    const rawData = (node.getExtInfo() ?? {}) as { nodeType?: string; label?: string };
    return {
      id: node.id,
      type: rawData.nodeType ?? String(node.flowNodeType),
      label: rawData.label || undefined,
    };
  }, [selectedNodeRef]);

  const updateCanvasNode = useCallback(
    (nodeId: string, patch: CanvasNodePatch): boolean => {
      if (!editorCtx) {
        return false;
      }
      const node = editorCtx.document.getNode(nodeId);
      if (!node || node.disposed || node.flowNodeType === FlowNodeBaseType.ROOT) {
        return false;
      }

      const current = (node.getExtInfo() ?? {}) as Record<string, unknown>;
      const currentConfig = (current.config as Record<string, unknown>) ?? {};

      const nextExtInfo: Record<string, unknown> = {
        ...current,
        ...(patch.label !== undefined ? { label: patch.label } : {}),
        ...(patch.config ? { config: { ...currentConfig, ...patch.config } } : {}),
        ...(patch.connectionId !== undefined ? { connectionId: patch.connectionId || null } : {}),
      };

      node.updateExtInfo(nextExtInfo);

      const nodeType = (nextExtInfo.nodeType as string) ?? String(node.flowNodeType);
      if (usesDynamicPorts(nodeType)) {
        window.requestAnimationFrame(() => {
          node.ports.updateDynamicPorts();
        });
      }
      return true;
    },
    [editorCtx],
  );

  const deleteCanvasNode = useCallback(
    (nodeId: string): boolean => {
      if (!editorCtx) {
        return false;
      }
      const node = editorCtx.document.getNode(nodeId);
      if (!node || node.disposed || node.flowNodeType === FlowNodeBaseType.ROOT) {
        return false;
      }
      node.dispose();
      return true;
    },
    [editorCtx],
  );

  const deleteCanvasEdge = useCallback(
    (from: string, to: string): boolean => {
      if (!editorCtx) {
        return false;
      }
      const lines = editorCtx.document.linesManager.getAllLines();
      for (const line of lines) {
        const lineFrom = line.info.from || (line.from as { id?: string } | undefined)?.id;
        const lineTo = line.info.to || (line.to as { id?: string } | undefined)?.id;
        if (lineFrom === from && lineTo === to) {
          line.dispose();
          return true;
        }
      }
      return false;
    },
    [editorCtx],
  );

  const insertNode = useCallback(
    async (seed: NodeSeed, mode: 'standalone' | 'downstream') => {
      if (!editorCtx || editorCtx.playground.config.readonly) {
        return;
      }

      try {
        const anchorNode = mode === 'downstream' ? selectedNodeRef.current : null;
        const nextId = allocateNodeId();
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
    },
    [connectionDefaults, editorCtx, reportFlowgramError, selectedNodeRef, syncSelectionState],
  );

  return {
    addCanvasOps,
    autoLayout,
    getSelectedNode,
    updateCanvasNode,
    deleteCanvasNode,
    deleteCanvasEdge,
    insertNode,
  };
}
