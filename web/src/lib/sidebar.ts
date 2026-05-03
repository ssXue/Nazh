//! 侧栏导航配置构建。

import type { SidebarSectionConfig } from '../components/app/types';
import { hasTauriRuntime } from './tauri';

/** 根据当前运行时状态构建侧栏导航区段配置列表。 */
export function buildSidebarSections(
  workflowStatusLabel: string,
  runtimeWorkflowCount: number,
  globalConnectionCount: number,
  logCount: number,
  boardCount: number,
  activeBoardName: string | null,
): SidebarSectionConfig[] {
  return [
    {
      key: 'dashboard',
      group: 'top',
      label: 'Dashboard',
      badge: workflowStatusLabel,
    },
    {
      key: 'boards',
      group: 'top',
      label: '所有看板',
      badge: activeBoardName ?? `${boardCount} 个工程`,
    },
    {
      key: 'runtime',
      group: 'main',
      label: '运行时管理',
      badge: runtimeWorkflowCount > 0 ? `${runtimeWorkflowCount} 个在线` : '当前空闲',
    },
    {
      key: 'connections',
      group: 'main',
      label: '连接资源',
      badge: `${globalConnectionCount} 个`,
    },
    {
      key: 'devices',
      group: 'main',
      label: '设备建模',
      badge: hasTauriRuntime() ? 'Device DSL' : '预览态',
    },
    {
      key: 'dsl-editor',
      group: 'main',
      label: 'DSL 编辑器',
      badge: 'Workflow DSL',
    },
    {
      key: 'plugins',
      group: 'main',
      label: '插件管理',
      badge: '节点类型',
    },
    {
      key: 'logs',
      group: 'main',
      label: '结构化日志',
      badge: logCount > 0 ? `${logCount} 条` : activeBoardName ? '等待事件' : '全局会话',
    },
    {
      key: 'ai',
      group: 'main',
      label: 'AI 配置',
      badge: hasTauriRuntime() ? 'Copilot' : '预览态',
    },
    {
      key: 'settings',
      group: 'settings',
      label: '设置',
      badge: hasTauriRuntime() ? '桌面态' : '预览态',
    },
    {
      key: 'about',
      group: 'settings',
      label: '关于',
      badge: 'Nazh',
    },
  ];
}
