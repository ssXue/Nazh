// 节点类型 → capabilities 位图的轻量缓存。
// 由 list_node_types IPC 填充，供 FlowgramCanvas 渲染时同步查询。
import { hasTauriRuntime, listNodeTypes } from './tauri';

let cache = new Map<string, number>();

export function getCachedCapabilities(nodeType: string): number | undefined {
  return cache.get(nodeType);
}

export async function refreshCapabilitiesCache(): Promise<void> {
  if (!hasTauriRuntime()) return;
  try {
    const resp = await listNodeTypes();
    cache = new Map(resp.types.map((t) => [t.name, t.capabilities] as const));
  } catch {
    // graceful degradation
  }
}