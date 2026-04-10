//! 侧栏导航配置构建。

import type { SidebarSectionConfig } from '../components/app/types';
import type { DeployResponse } from '../types';
import { BOARD_LIBRARY } from '../components/app/BoardsPanel';
import { hasTauriRuntime } from './tauri';

/** 根据当前运行时状态构建侧栏导航区段配置列表。 */
export function buildSidebarSections(
  workflowStatusLabel: string,
  graphConnectionCount: number,
  deployInfo: DeployResponse | null,
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
      badge: activeBoardName ?? `${BOARD_LIBRARY.length} 个工程`,
    },
    {
      key: 'connections',
      group: 'main',
      label: '连接资源',
      badge: activeBoardName ? `${graphConnectionCount} 个` : '未进入工程',
    },
    {
      key: 'payload',
      group: 'main',
      label: '测试载荷',
      badge: activeBoardName ? (deployInfo ? '可发送' : '待部署') : '未进入工程',
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
