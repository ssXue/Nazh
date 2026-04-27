import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

import { layoutGraph } from './graph';
import { stripNodeLocalAiConfig } from './workflow-ai';
import type { WorkflowGraph, WorkflowNodeDefinition } from '../types';

interface FlowgramNodeData {
  label: string;
  nodeType: string;
  displayType?: string;
  connectionId: string | null;
  timeoutMs: number | null;
  config: unknown;
  parentID?: string;
  blockIDs?: string[];
}

const FLOWGRAM_BUSINESS_NODE_TYPES = new Set([
  'native',
  'code',
  'timer',
  'serialTrigger',
  'modbusRead',
  'if',
  'switch',
  'tryCatch',
  'loop',
  'httpClient',
  'barkPush',
  'sqlWriter',
  'debugConsole',
  'subgraphInput',
  'subgraphOutput',
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
      const nodeType = definition?.type ?? 'unknown';
      const config = definition?.config ?? {};
      const data: FlowgramNodeData = {
        label: positionedNode.id,
        nodeType,
        displayType: nodeType,
        connectionId: definition?.connection_id ?? null,
        timeoutMs: definition?.timeout_ms ?? null,
        config,
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

/**
 * 深度遍历 JSON value，对所有 string 值做 `{{paramName}}` 替换。
 * 未绑定的参数保留原值。
 */
function applyParameterBindings(
  value: unknown,
  params: Record<string, string | number | boolean>,
): unknown {
  if (typeof value === 'string') {
    return value.replace(/\{\{(\w+)\}\}/g, (match, key: string) => {
      if (key in params) {
        return String(params[key]);
      }
      return match;
    });
  }
  if (Array.isArray(value)) {
    return value.map((item) => applyParameterBindings(item, params));
  }
  if (isRecord(value)) {
    const result: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      result[k] = applyParameterBindings(v, params);
    }
    return result;
  }
  return value;
}

function isContainerNode(node: FlowgramWorkflowJSON['nodes'][number]): boolean {
  return node.type === 'subgraph';
}

interface BridgeNodes {
  inputNodes: FlowgramWorkflowJSON['nodes'];
  outputNodes: FlowgramWorkflowJSON['nodes'];
}

function findBridgeNodes(blocks: FlowgramWorkflowJSON['nodes']): BridgeNodes {
  const inputNodes = blocks.filter((n) => n.type === 'subgraphInput');
  const outputNodes = blocks.filter((n) => n.type === 'subgraphOutput');
  return { inputNodes, outputNodes };
}

interface FlatGraph {
  nodes: FlowgramWorkflowJSON['nodes'];
  edges: FlowgramWorkflowJSON['edges'];
}

const MAX_SUBGRAPH_DEPTH = 8;

/**
 * 递归展平子图容器节点，返回纯平的 nodes + edges。
 *
 * 规则：
 * 1. 容器节点被移除，内部 blocks 递归展平后加入
 * 2. 内部节点 ID 加前缀 `<subgraph-id>/`
 * 3. 外部边重写：容器 input port → sg-in，容器 output port → sg-out
 * 4. `{{paramName}}` 被参数绑定替换
 * 5. 嵌套深度超过 8 层报错
 * 6. 检测循环引用（同一路径下 ID 重复）
 */
export function flattenSubgraphs(
  flowgramGraph: FlowgramWorkflowJSON,
  depth = 0,
  ancestorIds: Set<string> = new Set(),
): FlatGraph {
  if (depth > MAX_SUBGRAPH_DEPTH) {
    throw new Error(`子图嵌套超过 ${MAX_SUBGRAPH_DEPTH} 层上限`);
  }

  const flatNodes: FlowgramWorkflowJSON['nodes'] = [];
  const flatEdges: FlowgramWorkflowJSON['edges'] = [];

  const containerBridgeMap = new Map<
    string,
    { inputNodeIds: string[]; outputNodeIds: string[] }
  >();

  const flatNodeIds = new Set<string>();

  for (const node of flowgramGraph.nodes) {
    if (isContainerNode(node)) {
      if (ancestorIds.has(node.id)) {
        throw new Error(`子图循环引用：${node.id}`);
      }

      const params = isRecord(node.data)
        ? ((node.data as Record<string, unknown>).parameterBindings ?? {})
        : {};
      const paramMap: Record<string, string | number | boolean> = {};
      if (isRecord(params)) {
        for (const [k, v] of Object.entries(params)) {
          if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
            paramMap[k] = v;
          }
        }
      }

      const innerGraph: FlowgramWorkflowJSON = {
        nodes: node.blocks ?? [],
        edges: node.edges ?? [],
      };
      const nextAncestorIds = new Set(ancestorIds);
      nextAncestorIds.add(node.id);
      const inner = flattenSubgraphs(innerGraph, depth + 1, nextAncestorIds);

      const { inputNodes, outputNodes } = findBridgeNodes(innerGraph.nodes);
      const prefixedInputIds = inputNodes.map((n) => `${node.id}/${n.id}`);
      const prefixedOutputIds = outputNodes.map((n) => `${node.id}/${n.id}`);

      containerBridgeMap.set(node.id, {
        inputNodeIds: prefixedInputIds,
        outputNodeIds: prefixedOutputIds,
      });

      for (const innerNode of inner.nodes) {
        const prefixedNode = {
          ...innerNode,
          id: `${node.id}/${innerNode.id}`,
        };
        if (Object.keys(paramMap).length > 0) {
          prefixedNode.data = applyParameterBindings(prefixedNode.data, paramMap) as Record<
            string,
            unknown
          >;
        }
        flatNodes.push(prefixedNode);
        flatNodeIds.add(prefixedNode.id);
      }

      for (const innerEdge of inner.edges) {
        flatEdges.push({
          ...innerEdge,
          sourceNodeID: `${node.id}/${innerEdge.sourceNodeID}`,
          targetNodeID: `${node.id}/${innerEdge.targetNodeID}`,
        });
      }
    } else {
      flatNodes.push(node);
      flatNodeIds.add(node.id);
    }
  }

  for (const edge of flowgramGraph.edges) {
    let sourceId = edge.sourceNodeID;
    let targetId = edge.targetNodeID;
    let sourcePortID = edge.sourcePortID;
    let targetPortID = edge.targetPortID;

    const sourceBridge = containerBridgeMap.get(sourceId);
    if (sourceBridge) {
      const outputIds = sourceBridge.outputNodeIds;
      if (outputIds.length > 0) {
        sourceId = outputIds[0] ?? sourceId;
        sourcePortID = undefined;
      }
    }

    const targetBridge = containerBridgeMap.get(targetId);
    if (targetBridge) {
      const inputIds = targetBridge.inputNodeIds;
      if (inputIds.length > 0) {
        targetId = inputIds[0] ?? targetId;
        targetPortID = undefined;
      }
    }

    if (flatNodeIds.has(sourceId) && flatNodeIds.has(targetId)) {
      flatEdges.push({
        sourceNodeID: sourceId,
        targetNodeID: targetId,
        sourcePortID,
        targetPortID,
      });
    }
  }

  return { nodes: flatNodes, edges: flatEdges };
}

export function toNazhWorkflowGraph(
  flowgramGraph: FlowgramWorkflowJSON,
  previousGraph: WorkflowGraph,
): WorkflowGraph {
  const flat = flattenSubgraphs(flowgramGraph);
  const businessNodes = flat.nodes.filter(isBusinessNode);
  const businessNodeIds = new Set(businessNodes.map((node) => node.id));
  const nodes = businessNodes.reduce<Record<string, WorkflowNodeDefinition>>((acc, node) => {
    const previousNode = previousGraph.nodes[node.id];
    const rawData = (node.data ?? {}) as Partial<FlowgramNodeData>;
    const position = node.meta?.position;
    const hasNodeType = hasOwnKey(rawData, 'nodeType');
    const hasConnectionId = hasOwnKey(rawData, 'connectionId');
    const hasConfig = hasOwnKey(rawData, 'config');
    const hasTimeoutMs = hasOwnKey(rawData, 'timeoutMs');
    const nodeType = String(
      (hasNodeType ? rawData.nodeType : undefined) ?? previousNode?.type ?? node.type,
    );
    const nextConfig = hasConfig
      ? ((rawData.config as WorkflowNodeDefinition['config']) ?? {})
      : previousNode?.config ?? {};

    acc[node.id] = {
      id: node.id,
      type: nodeType,
      connection_id: hasConnectionId
        ? rawData.connectionId ?? undefined
        : previousNode?.connection_id,
      config: stripNodeLocalAiConfig(nodeType, nextConfig),
      timeout_ms: hasTimeoutMs
        ? rawData.timeoutMs ?? undefined
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
    variables: previousGraph.variables,
    edges: flat.edges
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
