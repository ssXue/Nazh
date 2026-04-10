import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

// ── 从 Rust 引擎自动生成的 IPC 契约类型（ts-rs） ────────────

export type {
  ConnectionDefinition,
  ConnectionRecord,
  DeployResponse,
  DispatchResponse,
  ExecutionEvent,
  JsonValue,
  UndeployResponse,
  WorkflowContext,
  WorkflowEdge,
} from './generated';

export type { WorkflowContext as WorkflowResult } from './generated';

// ── 前端扩展类型（补充 Rust 侧不需要的画布/UI 字段） ───────

import type {
  JsonValue,
  WorkflowGraph as WorkflowGraphBase,
  WorkflowNodeDefinition as WorkflowNodeDefinitionBase,
} from './generated';

/**
 * 带有画布布局元数据的节点定义。
 *
 * 在 Rust 生成的基础类型上：
 * - `id` / `config` / `buffer` 改为可选（Rust 侧 `#[serde(default)]`，前端构造时可省略）
 * - 追加前端独有的 `meta.position` 画布坐标
 */
export type WorkflowNodeDefinition =
  Omit<WorkflowNodeDefinitionBase, 'id' | 'config' | 'buffer'> & {
    id?: string;
    config?: JsonValue;
    buffer?: number;
    meta?: {
      position?: {
        x: number;
        y: number;
      };
    };
  };

/**
 * 带有前端画布编辑器状态的工作流图。
 *
 * - `connections` 改为可选（Rust 侧 `#[serde(default)]`）
 * - `nodes` 使用扩展后的 `WorkflowNodeDefinition`
 * - 追加前端独有的 `editor_graph`（FlowGram 画布状态）
 */
export type WorkflowGraph = Omit<WorkflowGraphBase, 'connections' | 'nodes'> & {
  connections?: WorkflowGraphBase['connections'];
  nodes: Record<string, WorkflowNodeDefinition>;
  editor_graph?: FlowgramWorkflowJSON;
};

// ── 纯前端类型（不跨 IPC 边界） ────────────────────────────

export interface WorkflowLogicBranch {
  key: string;
  label?: string;
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
