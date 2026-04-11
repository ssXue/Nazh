import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

import { layoutGraph } from './graph';
import type { WorkflowGraph, WorkflowNodeDefinition } from '../types';

interface FlowgramNodeData {
  label: string;
  nodeType: string;
  displayType?: string;
  connectionId: string | null;
  aiDescription: string | null;
  timeoutMs: number | null;
  config: unknown;
  parentID?: string;
  blockIDs?: string[];
}

const FLOWGRAM_BUSINESS_NODE_TYPES = new Set([
  'native',
  'rhai',
  'code',
  'timer',
  'serialTrigger',
  'modbusRead',
  'if',
  'switch',
  'tryCatch',
  'loop',
  'httpClient',
  'sqlWriter',
  'debugConsole',
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwnKey<T extends object>(value: T, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function isBusinessNodeType(type: unknown): boolean {
  return typeof type === 'string' && FLOWGRAM_BUSINESS_NODE_TYPES.has(type);
}

function isBusinessNode(node: FlowgramWorkflowJSON['nodes'][number]): boolean {
  if (isBusinessNodeType(node.type)) {
    return true;
  }

  const rawData = isRecord(node.data) ? node.data : {};
  return isBusinessNodeType(rawData.nodeType);
}

function buildEdgeKey(edge: FlowgramWorkflowJSON['edges'][number]): string {
  return `${edge.sourceNodeID}:${String(edge.sourcePortID ?? '')}->${edge.targetNodeID}:${String(edge.targetPortID ?? '')}`;
}

function buildBaseFlowgramWorkflowJson(graph: WorkflowGraph): FlowgramWorkflowJSON {
  const positionedNodes = layoutGraph(graph);

  return {
    nodes: positionedNodes.map((positionedNode) => {
      const definition = graph.nodes[positionedNode.id];
      const position = definition?.meta?.position ?? {
        x: 44 + positionedNode.layer * 320,
        y: 40 + positionedNode.row * 196,
      };
      const data: FlowgramNodeData = {
        label: positionedNode.id,
        nodeType: definition?.type ?? 'unknown',
        displayType: definition?.type ?? 'unknown',
        connectionId: definition?.connection_id ?? null,
        aiDescription: definition?.ai_description ?? null,
        timeoutMs: definition?.timeout_ms ?? null,
        config: definition?.config ?? {},
      };

      return {
        id: positionedNode.id,
        type: positionedNode.type,
        meta: {
          position,
        },
        data,
      };
    }),
    edges: (graph.edges ?? []).map((edge) => ({
      sourceNodeID: edge.from,
      targetNodeID: edge.to,
      sourcePortID: edge.source_port_id,
      targetPortID: edge.target_port_id,
    })),
  };
}

function sanitizeEditorNodes(
  nodes: FlowgramWorkflowJSON['nodes'],
): FlowgramWorkflowJSON['nodes'] {
  const nodeIds = new Set(nodes.map((node) => node.id));

  return nodes.map((node) => {
    if (!isRecord(node.data)) {
      return node;
    }

    const nextData = {
      ...node.data,
    };

    if (Array.isArray(nextData.blockIDs)) {
      nextData.blockIDs = nextData.blockIDs.filter(
        (blockId): blockId is string => typeof blockId === 'string' && nodeIds.has(blockId),
      );
    }

    if (typeof nextData.parentID === 'string' && nextData.parentID !== 'root' && !nodeIds.has(nextData.parentID)) {
      nextData.parentID = 'root';
    }

    return {
      ...node,
      data: nextData,
    };
  });
}

export function toFlowgramWorkflowJson(graph: WorkflowGraph): FlowgramWorkflowJSON {
  const baseGraph = buildBaseFlowgramWorkflowJson(graph);
  const editorGraph = graph.editor_graph;

  if (!editorGraph) {
    return baseGraph;
  }

  const editorNodeMap = new Map(editorGraph.nodes.map((node) => [node.id, node] as const));
  const businessNodeIds = new Set(baseGraph.nodes.map((node) => node.id));
  const mergedBusinessNodes = baseGraph.nodes.map((node) => {
    const persistedNode = editorNodeMap.get(node.id);

    if (!persistedNode) {
      return node;
    }

    return {
      ...node,
      meta: persistedNode.meta ?? node.meta,
      data: {
        ...(isRecord(persistedNode.data) ? persistedNode.data : {}),
        ...(isRecord(node.data) ? node.data : {}),
      },
      blocks: persistedNode.blocks,
      edges: persistedNode.edges,
    };
  });
  const editorOnlyNodes = editorGraph.nodes.filter(
    (node) => !businessNodeIds.has(node.id) && !isBusinessNode(node),
  );
  const nodes = sanitizeEditorNodes([...mergedBusinessNodes, ...editorOnlyNodes]);
  const nodeIds = new Set(nodes.map((node) => node.id));
  const businessEdgeKeys = new Set(baseGraph.edges.map(buildEdgeKey));
  const editorOnlyEdges = editorGraph.edges.filter((edge) => {
    const edgeKey = buildEdgeKey(edge);

    if (businessEdgeKeys.has(edgeKey)) {
      return false;
    }

    if (!nodeIds.has(edge.sourceNodeID) || !nodeIds.has(edge.targetNodeID)) {
      return false;
    }

    return !businessNodeIds.has(edge.sourceNodeID) || !businessNodeIds.has(edge.targetNodeID);
  });

  return {
    nodes,
    edges: [...baseGraph.edges, ...editorOnlyEdges],
  };
}

export function toNazhWorkflowGraph(
  flowgramGraph: FlowgramWorkflowJSON,
  previousGraph: WorkflowGraph,
): WorkflowGraph {
  const businessNodes = flowgramGraph.nodes.filter(isBusinessNode);
  const businessNodeIds = new Set(businessNodes.map((node) => node.id));
  const nodes = businessNodes.reduce<Record<string, WorkflowNodeDefinition>>((acc, node) => {
    const previousNode = previousGraph.nodes[node.id];
    const rawData = (node.data ?? {}) as Partial<FlowgramNodeData>;
    const position = node.meta?.position;
    const hasNodeType = hasOwnKey(rawData, 'nodeType');
    const hasConnectionId = hasOwnKey(rawData, 'connectionId');
    const hasConfig = hasOwnKey(rawData, 'config');
    const hasAiDescription = hasOwnKey(rawData, 'aiDescription');
    const hasTimeoutMs = hasOwnKey(rawData, 'timeoutMs');

    acc[node.id] = {
      id: node.id,
      type: String(
        (hasNodeType ? rawData.nodeType : undefined) ?? previousNode?.type ?? node.type,
      ),
      connection_id: hasConnectionId
        ? rawData.connectionId ?? undefined
        : previousNode?.connection_id,
      config: hasConfig
        ? ((rawData.config as WorkflowNodeDefinition['config']) ?? {})
        : previousNode?.config ?? {},
      ai_description: hasAiDescription
        ? rawData.aiDescription ?? undefined
        : previousNode?.ai_description,
      timeout_ms: hasTimeoutMs
        ? typeof rawData.timeoutMs === 'number'
          ? rawData.timeoutMs
          : undefined
        : previousNode?.timeout_ms,
      buffer: previousNode?.buffer,
      meta: position
        ? {
            position: {
              x: position.x,
              y: position.y,
            },
          }
        : previousNode?.meta,
    };

    return acc;
  }, {});

  return {
    name: previousGraph.name,
    connections: [],
    editor_graph: flowgramGraph,
    nodes,
    edges: flowgramGraph.edges
      .filter(
        (edge) => businessNodeIds.has(edge.sourceNodeID) && businessNodeIds.has(edge.targetNodeID),
      )
      .map((edge) => ({
        from: edge.sourceNodeID,
        to: edge.targetNodeID,
        source_port_id:
          typeof edge.sourcePortID === 'string' ? edge.sourcePortID : undefined,
        target_port_id:
          typeof edge.targetPortID === 'string' ? edge.targetPortID : undefined,
      })),
  };
}

export function formatWorkflowGraph(graph: WorkflowGraph): string {
  return JSON.stringify(graph, null, 2);
}
