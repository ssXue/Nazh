export const NODE_CATEGORIES = [
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
