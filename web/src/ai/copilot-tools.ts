/// Copilot 工具定义与执行。
///
/// 将工具分为两类：
/// - 查询工具：通过 IPC 调度到 Rust 引擎执行
/// - 画布工具：由前端直接执行（操作 FlowGram 画布）

import { invoke } from '@tauri-apps/api/core';
import { tool } from 'ai';
import { z } from 'zod';

/// 查询工具：通过 Rust IPC 执行。
async function dispatchQueryTool(name: string, args: Record<string, unknown> & { workspace_path?: string | null }): Promise<string> {
  return invoke<string>('copilot_dispatch_tool', {
    toolName: name,
    argumentsJson: JSON.stringify(args),
    workspacePath: args.workspace_path ?? null,
  });
}

/// 构建所有 copilot 工具定义（用于 ai-sdk streamText 的 tools 参数）。
///
/// 画布操作通过 onCanvasOp 回调通知调用方。
export function buildCopilotTools(onCanvasOp?: (op: CanvasOpEvent) => void) {
  return {
    // ── 查询工具 ──
    query_node_catalog: tool({
      description: '列出工作流引擎中所有可用的节点类型及其描述。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('query_node_catalog', {}),
    }),
    describe_node: tool({
      description: '获取指定节点类型的输入/输出 pin schema。',
      inputSchema: z.object({
        node_type: z.string().describe('节点类型标识符'),
      }),
      execute: async ({ node_type }): Promise<string> => dispatchQueryTool('describe_node', { node_type }),
    }),
    list_connections: tool({
      description: '列出当前配置的所有连接（串口、Modbus、MQTT、HTTP 等）。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('list_connections', {}),
    }),
    search_devices: tool({
      description: '搜索已定义的设备 DSL 资产。',
      inputSchema: z.object({
        keyword: z.string().optional().describe('搜索关键词'),
      }),
      execute: async ({ keyword }): Promise<string> => dispatchQueryTool('search_devices', { keyword }),
    }),
    search_capabilities: tool({
      description: '搜索已定义的能力 DSL 资产。',
      inputSchema: z.object({
        device_id: z.string().optional().describe('按设备 ID 过滤'),
        keyword: z.string().optional().describe('搜索关键词'),
      }),
      execute: async (args): Promise<string> => dispatchQueryTool('search_capabilities', args),
    }),
    get_active_workflow: tool({
      description: '获取当前活跃工作流的结构信息。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('get_active_workflow', {}),
    }),
    query_workflow_status: tool({
      description: '获取所有已部署工作流的运行时状态摘要。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('query_workflow_status', {}),
    }),
    read_asset_yaml: tool({
      description: '读取指定设备或能力资产的完整 YAML 内容。',
      inputSchema: z.object({
        asset_type: z.enum(['device', 'capability']).describe('资产类型'),
        asset_id: z.string().describe('资产 ID'),
      }),
      execute: async (args): Promise<string> => dispatchQueryTool('read_asset_yaml', args),
    }),
    validate_workflow: tool({
      description: '验证工作流 JSON 结构是否合法。',
      inputSchema: z.object({
        workflow_json: z.string().describe('工作流 JSON 字符串'),
      }),
      execute: async ({ workflow_json }): Promise<string> => dispatchQueryTool('validate_workflow', { workflow_json }),
    }),
    // ── 画布工具 ──
    create_workflow: tool({
      description: '在画布上创建新工作流工程。',
      inputSchema: z.object({
        name: z.string().optional().describe('工程名称'),
        description: z.string().optional().describe('工程描述'),
      }),
      execute: async ({ name }): Promise<string> => {
        onCanvasOp?.({ type: 'create_workflow', name });
        return `工作流「${name ?? '新工作流'}」已创建，可以开始添加节点`;
      },
    }),
    add_workflow_node: tool({
      description: '在画布上添加一个工作流节点。',
      inputSchema: z.object({
        ref: z.string().describe('节点引用 ID'),
        node_type: z.string().describe('节点类型标识符'),
        label: z.string().optional().describe('节点显示名称'),
        config: z.record(z.unknown()).optional().describe('节点配置'),
        connection_id: z.string().optional().describe('关联的连接 ID'),
      }),
      execute: async (args): Promise<string> => {
        const nodeId = `ai_${crypto.randomUUID().replace(/-/g, '')}`;
        onCanvasOp?.({
          type: 'add_node',
          nodeId,
          ref: args.ref,
          nodeType: args.node_type,
          label: args.label,
          config: args.config,
          connectionId: args.connection_id,
        });
        return `节点 ${args.ref}（${args.node_type}）已添加，ID: ${nodeId}`;
      },
    }),
    add_workflow_edge: tool({
      description: '连接两个节点。',
      inputSchema: z.object({
        from_ref: z.string().describe('起始节点的 ref'),
        to_ref: z.string().describe('目标节点的 ref'),
        source_port_id: z.string().optional().describe('起始节点的输出端口 ID'),
        target_port_id: z.string().optional().describe('目标节点的输入端口 ID'),
      }),
      execute: async (args): Promise<string> => {
        onCanvasOp?.({
          type: 'add_edge',
          fromRef: args.from_ref,
          toRef: args.to_ref,
          sourcePortId: args.source_port_id,
          targetPortId: args.target_port_id,
        });
        return `连线 ${args.from_ref} → ${args.to_ref} 已添加`;
      },
    }),
  };
}

/// 画布操作事件类型（与 CopilotPanel 兼容）。
export interface CanvasOpEvent {
  type: 'add_node' | 'add_edge' | 'create_workflow';
  nodeId?: string;
  ref?: string;
  nodeType?: string;
  label?: string;
  config?: Record<string, unknown>;
  connectionId?: string;
  fromRef?: string;
  toRef?: string;
  sourcePortId?: string;
  targetPortId?: string;
  name?: string;
}
