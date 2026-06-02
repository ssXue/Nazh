/**
 * 节点面板分组定义。
 *
 * 顶层 `NODE_PANEL_GROUPS` 定义分组（可折叠），每个分组包含若干
 * `categories`（内部类别标题）。节点通过 `catalog.category` 归属到
 * 对应 category，再由分组聚合渲染。
 *
 * 旧的 `NODE_CATEGORIES` 仍作为合法 category 值的来源——新增/删除
 * category 时须同步更新此处和各节点定义。
 */

export const NODE_CATEGORIES = [
  '设备能力',
  '流程控制',
  '脚本执行',
  '数据注入',
  '硬件接口',
  '外部通信',
  '持久化',
  '调试工具',
  '子图封装',
  '纯计算',
] as const;

export type NodeCategory = (typeof NODE_CATEGORIES)[number];

export interface NodePanelGroup {
  key: string;
  title: string;
  categories: NodeCategory[];
  collapsed?: boolean;
}

export const NODE_PANEL_GROUPS: readonly NodePanelGroup[] = [
  {
    key: 'data-acquisition',
    title: '数据采集',
    categories: ['设备能力', '数据注入'],
  },
  {
    key: 'flow-control',
    title: '流程控制',
    categories: ['流程控制', '脚本执行', '子图封装'],
  },
  {
    key: 'output-storage',
    title: '通知 & 存储',
    categories: ['外部通信', '持久化'],
  },
  {
    key: 'protocol-debug',
    title: '协议 & 调试',
    categories: ['硬件接口', '调试工具', '纯计算'],
    collapsed: true,
  },
];