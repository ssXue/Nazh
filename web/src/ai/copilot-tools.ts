/// Copilot 工具定义与执行。
///
/// 将工具分为两类：
/// - 查询工具：通过 IPC 调度到 Rust 引擎执行
/// - 画布工具：由前端直接执行（操作 FlowGram 画布）

import { invoke } from '@tauri-apps/api/core';
import { tool } from 'ai';
import { z } from 'zod';

import { allocateNodeId } from '../lib/workflow-node-id';

/// 查询工具：通过 Rust IPC 执行。
async function dispatchQueryTool(
  name: string,
  args: Record<string, unknown>,
  workspacePath?: string | null,
): Promise<string> {
  return invoke<string>('copilot_dispatch_tool', {
    toolName: name,
    argumentsJson: JSON.stringify(args),
    workspacePath: workspacePath ?? null,
  });
}

/// ref → 实际节点 ID 的映射表。每次 copilot 会话维护一份。
type RefMap = Map<string, string>;

/// 构建所有 copilot 工具定义（用于 ai-sdk streamText 的 tools 参数）。
///
/// 画布操作通过 onCanvasOp 回调通知调用方。
/// refMap 在多次工具调用间共享，将 AI 使用的短引用名映射到实际节点实体 ID。
export function buildCopilotTools(
  onCanvasOp?: (op: CanvasOpEvent) => void,
  refMap?: RefMap,
  workspacePath?: string,
) {
  const map = refMap ?? new Map<string, string>();
  return {
    // ── 查询工具 ──
    query_node_catalog: tool({
      description: '列出工作流引擎中所有可用的节点类型及其描述。创建工作流前应先调用此工具确认可用的节点类型。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('query_node_catalog', {}, workspacePath),
    }),
    describe_node: tool({
      description: '获取指定节点类型的输入/输出 pin schema，用于了解节点需要的配置字段和数据类型。',
      inputSchema: z.object({
        node_type: z.string().describe('节点类型标识符，如 timer、http、modbusRead 等'),
      }),
      execute: async ({ node_type }): Promise<string> => dispatchQueryTool('describe_node', { node_type }, workspacePath),
    }),
    list_connections: tool({
      description: '列出当前配置的所有连接（串口、Modbus、MQTT、HTTP、CAN 等），返回连接 ID、类型和使用状态。I/O 节点需要 connection_id 参数时应先调用此工具获取。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('list_connections', {}, workspacePath),
    }),
    search_devices: tool({
      description: '搜索已定义的设备 DSL 资产，返回设备 ID、名称、类型等摘要信息。需要 capabilityCall 节点时应先查找目标设备。',
      inputSchema: z.object({
        keyword: z.string().optional().describe('搜索关键词（匹配 ID、名称或类型）'),
      }),
      execute: async ({ keyword }): Promise<string> => dispatchQueryTool('search_devices', { keyword }, workspacePath),
    }),
    search_capabilities: tool({
      description: '搜索已定义的能力 DSL 资产，返回能力 ID、名称、关联设备等摘要信息。capabilityCall 节点需要能力 ID 时应先调用此工具。',
      inputSchema: z.object({
        device_id: z.string().optional().describe('按设备 ID 过滤'),
        keyword: z.string().optional().describe('搜索关键词（匹配 ID 或名称）'),
      }),
      execute: async (args): Promise<string> => dispatchQueryTool('search_capabilities', args, workspacePath),
    }),
    get_active_workflow: tool({
      description: '获取当前活跃工作流的结构信息（节点列表、连接关系等）。需要了解画布上已有节点时调用。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('get_active_workflow', {}, workspacePath),
    }),
    query_workflow_status: tool({
      description: '获取所有已部署工作流的运行时状态摘要，包括节点数、连线数、部署时间和活跃状态。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('query_workflow_status', {}, workspacePath),
    }),
    read_asset_yaml: tool({
      description: '读取指定设备或能力资产的完整 YAML 内容，用于了解设备的信号定义或能力的实现细节。',
      inputSchema: z.object({
        asset_type: z.enum(['device', 'capability']).describe('资产类型'),
        asset_id: z.string().describe('资产 ID'),
      }),
      execute: async (args): Promise<string> => dispatchQueryTool('read_asset_yaml', args, workspacePath),
    }),
    validate_workflow: tool({
      description: '验证工作流 JSON 结构是否合法（DAG 拓扑校验、节点类型存在性）。建议在完成节点添加和连线后调用此工具检查。',
      inputSchema: z.object({
        workflow_json: z.string().describe('工作流 JSON 字符串'),
      }),
      execute: async ({ workflow_json }): Promise<string> => dispatchQueryTool('validate_workflow', { workflow_json }, workspacePath),
    }),
    get_scripting_reference: tool({
      description: '获取 Rhai 脚本 API 参考文档，包括所有内置函数（rand、now_ms、from_json、to_json、is_blank）、工作流变量 API（vars.get/set/cas）和使用约束。当你需要编写或解释 code 节点、if/switch/loop 条件脚本时调用此工具。',
      inputSchema: z.object({}),
      execute: async (): Promise<string> => dispatchQueryTool('get_scripting_reference', {}, workspacePath),
    }),
    // ── 画布工具 ──
    create_workflow: tool({
      description: '在画布上创建新工作流工程。创建工作流时应首先调用此工具初始化画布，然后依次调用 add_workflow_node 添加节点，再用 add_workflow_edge 连接节点。',
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
      description: '在画布上添加一个工作流节点。ref 是同一工作流内稳定的英文别名（如 timer、debug、modbus_read），后续用 add_workflow_edge 引用。node_type 必须是 query_node_catalog 返回的合法类型。',
      inputSchema: z.object({
        ref: z.string().describe('节点引用 ID（同一工作流内的英文别名，如 timer、debug）'),
        node_type: z.string().describe('节点类型标识符，必须是 query_node_catalog 返回的合法类型'),
        label: z.string().optional().describe('节点显示名称'),
        config: z.record(z.unknown()).optional().describe('节点配置（如 {"interval_ms": 5000}）'),
        connection_id: z.string().optional().describe('关联的连接 ID（仅设备 I/O 节点需要）'),
      }),
      execute: async (args): Promise<string> => {
        const nodeId = allocateNodeId();
        map.set(args.ref, nodeId);
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
      description: '连接两个节点。from_ref 和 to_ref 必须是之前 add_workflow_node 中定义的 ref 值。每个有后续处理的节点都必须有连线指向下游，不允许孤立节点。',
      inputSchema: z.object({
        from_ref: z.string().describe('起始节点的 ref'),
        to_ref: z.string().describe('目标节点的 ref'),
        source_port_id: z.string().optional().describe('起始节点的输出端口 ID（默认使用第一个输出）'),
        target_port_id: z.string().optional().describe('目标节点的输入端口 ID（默认使用第一个输入）'),
      }),
      execute: async (args): Promise<string> => {
        const fromId = map.get(args.from_ref);
        const toId = map.get(args.to_ref);
        if (!fromId) {
          return `错误: 未知起始节点 ref "${args.from_ref}"，请先通过 add_workflow_node 创建该节点`;
        }
        if (!toId) {
          return `错误: 未知目标节点 ref "${args.to_ref}"，请先通过 add_workflow_node 创建该节点`;
        }
        onCanvasOp?.({
          type: 'add_edge',
          fromRef: args.from_ref,
          toRef: args.to_ref,
          fromId,
          toId,
          sourcePortId: args.source_port_id,
          targetPortId: args.target_port_id,
        });
        return `连线 ${args.from_ref} → ${args.to_ref} 已添加`;
      },
    }),
    // ── 编辑/删除工具 ──
    edit_workflow_node: tool({
      description: '修改画布上已有节点的配置。node_id 必须是当前画布上存在的节点实际 ID（从画布状态中获取），不是 ref。只传需要修改的字段，未传的字段保持不变。',
      inputSchema: z.object({
        node_id: z.string().describe('目标节点的实际 ID（从画布状态中获取）'),
        label: z.string().optional().describe('新显示名称'),
        config: z.record(z.unknown()).optional().describe('要更新的配置字段（与现有配置浅合并）'),
        connection_id: z.string().optional().describe('新的关联连接 ID'),
      }),
      execute: async (args): Promise<string> => {
        onCanvasOp?.({
          type: 'update_node',
          nodeId: args.node_id,
          label: args.label,
          config: args.config,
          connectionId: args.connection_id,
        });
        return `节点 ${args.node_id} 已更新`;
      },
    }),
    delete_workflow_node: tool({
      description: '删除画布上的一个节点及其所有连线。node_id 必须是当前画布上存在的节点实际 ID（从画布状态中获取），不是 ref。',
      inputSchema: z.object({
        node_id: z.string().describe('要删除的节点实际 ID（从画布状态中获取）'),
      }),
      execute: async (args): Promise<string> => {
        onCanvasOp?.({ type: 'delete_node', nodeId: args.node_id });
        return `节点 ${args.node_id} 已删除`;
      },
    }),
    delete_workflow_edge: tool({
      description: '删除两个节点之间的连线。from 和 to 是节点的实际 ID（从画布状态中获取）。',
      inputSchema: z.object({
        from: z.string().describe('起始节点 ID'),
        to: z.string().describe('目标节点 ID'),
      }),
      execute: async (args): Promise<string> => {
        onCanvasOp?.({ type: 'delete_edge', from: args.from, to: args.to });
        return `连线 ${args.from} → ${args.to} 已删除`;
      },
    }),
  };
}

/// 画布操作事件类型（与 CopilotPanel 兼容）。
export interface CanvasOpEvent {
  type: 'add_node' | 'add_edge' | 'create_workflow'
      | 'update_node' | 'delete_node' | 'delete_edge';
  nodeId?: string;
  ref?: string;
  nodeType?: string;
  label?: string;
  config?: Record<string, unknown>;
  connectionId?: string;
  fromRef?: string;
  toRef?: string;
  fromId?: string;
  toId?: string;
  sourcePortId?: string;
  targetPortId?: string;
  name?: string;
  from?: string;
  to?: string;
}
