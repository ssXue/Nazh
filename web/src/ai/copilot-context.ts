/// Copilot 运行时上下文构建。
///
/// 将当前画布状态（工作流拓扑、选中节点）转换为简洁的中文文本段落，
/// 追加到系统提示中，让 AI 在对话开局即具备画布感知能力。

import type { WorkflowGraph } from '../types';

interface SelectedNodeSummary {
  id: string;
  type: string;
  label?: string;
}

/// 构建运行时上下文文本。返回空字符串表示无画布或画布为空。
export function buildRuntimeContextPrompt(
  graph: WorkflowGraph | null | undefined,
  selectedNode: SelectedNodeSummary | null | undefined,
): string {
  if (!graph) return '';

  const nodeEntries = Object.entries(graph.nodes);
  if (nodeEntries.length === 0) return '';

  const parts: string[] = ['\n\n## 当前画布状态\n'];

  // 工作流名称
  const nameSuffix = graph.name ? `：${graph.name}` : '';
  parts.push(`工作流${nameSuffix}（${nodeEntries.length} 个节点，${graph.edges.length} 条连线）`);

  // 节点列表
  parts.push('节点：');
  for (const [id, def] of nodeEntries) {
    const typeStr = def.node_type ?? 'unknown';
    const labelStr = def.label ? ` ${def.label}` : '';
    const connStr = def.connection_id ? ` → 连接: ${def.connection_id}` : '';
    parts.push(`- ${id} [${typeStr}]${labelStr}${connStr}`);
  }

  // 连线摘要
  if (graph.edges.length > 0) {
    const edgeStrs = graph.edges.map((e) => `${e.from} → ${e.to}`);
    parts.push(`连线：${edgeStrs.join('; ')}`);
  }

  // 选中节点
  if (selectedNode) {
    const selLabel = selectedNode.label ? ` ${selectedNode.label}` : '';
    parts.push(`当前选中：${selectedNode.id} [${selectedNode.type}]${selLabel}`);
  }

  return parts.join('\n');
}
