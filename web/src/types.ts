import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface WorkflowGraph {
  name?: string;
  connections?: ConnectionDefinition[];
  editor_graph?: FlowgramWorkflowJSON;
  nodes: Record<string, WorkflowNodeDefinition>;
  edges: WorkflowEdge[];
}

export interface ConnectionDefinition {
  id: string;
  type: string;
  metadata?: JsonValue;
}

export interface WorkflowNodeDefinition {
  id?: string;
  type: string;
  connection_id?: string;
  config?: JsonValue;
  ai_description?: string;
  timeout_ms?: number;
  buffer?: number;
  meta?: {
    position?: {
      x: number;
      y: number;
    };
  };
}

export interface WorkflowLogicBranch {
  key: string;
  label?: string;
}

export interface WorkflowEdge {
  from: string;
  to: string;
  source_port_id?: string;
  target_port_id?: string;
}

export interface WorkflowEvent {
  NodeStarted?: {
    node_id: string;
    trace_id: string;
  };
  NodeCompleted?: {
    node_id: string;
    trace_id: string;
  };
  NodeFailed?: {
    node_id: string;
    trace_id: string;
    error: string;
  };
  WorkflowOutput?: {
    node_id: string;
    trace_id: string;
  };
}

export interface WorkflowRuntimeState {
  traceId: string | null;
  lastEventType: 'started' | 'completed' | 'failed' | 'output' | null;
  lastNodeId: string | null;
  lastError: string | null;
  lastUpdatedAt: number | null;
  activeNodeIds: string[];
  completedNodeIds: string[];
  failedNodeIds: string[];
  outputNodeIds: string[];
}

export type WorkflowWindowStatus =
  | 'idle'
  | 'deployed'
  | 'running'
  | 'completed'
  | 'failed'
  | 'preview';

export interface WorkflowResult {
  trace_id: string;
  timestamp: string;
  payload: JsonValue;
}

export interface RuntimeLogEntry {
  id: string;
  timestamp: number;
  level: 'info' | 'success' | 'warn' | 'error';
  source: string;
  message: string;
  detail?: string | null;
}

export interface AppErrorRecord {
  id: string;
  timestamp: number;
  scope: 'workflow' | 'command' | 'frontend' | 'runtime';
  title: string;
  detail?: string | null;
}

export interface DeployResponse {
  nodeCount: number;
  edgeCount: number;
  rootNodes: string[];
}

export interface UndeployResponse {
  hadWorkflow: boolean;
  abortedTimerCount: number;
}

export interface DispatchResponse {
  traceId: string;
}

export interface ConnectionRecord {
  id: string;
  kind: string;
  metadata: JsonValue;
  in_use: boolean;
  last_borrowed_at?: string | null;
}

export const SAMPLE_AST = JSON.stringify(
  {
    name: 'Industrial Temperature Demo',
    connections: [
      {
        id: 'plc-main',
        type: 'modbus',
        metadata: {
          host: '192.168.10.11',
          port: 502,
          rack: 0,
          slot: 1,
        },
      },
    ],
    nodes: {
      ingress: {
        type: 'native',
        connection_id: 'plc-main',
        config: {
          message: 'PLC temperature frame received',
          inject: {
            gateway: 'edge-a',
            area: 'boiler-room',
          },
        },
      },
      normalize: {
        type: 'rhai',
        config: {
          script:
            'payload["temperature_f"] = (payload["value"] * 1.8) + 32.0; payload["status"] = payload["value"] > 80 ? "alert" : "nominal"; payload',
        },
      },
      enrich: {
        type: 'rhai',
        config: {
          script:
            'payload["tag"] = `${payload["gateway"]}:${payload["area"]}`; payload',
        },
      },
    },
    edges: [
      {
        from: 'ingress',
        to: 'normalize',
      },
      {
        from: 'normalize',
        to: 'enrich',
      },
    ],
  },
  null,
  2,
);

export const SAMPLE_PAYLOAD = JSON.stringify(
  {
    value: 73.4,
    quality: 'good',
  },
  null,
  2,
);
