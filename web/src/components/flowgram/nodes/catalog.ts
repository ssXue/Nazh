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

export const NODE_CATEGORY_MAP: Record<
  string,
  { category: NodeCategory; description: string }
> = {
  if: { category: '流程控制', description: '布尔条件分支路由' },
  switch: { category: '流程控制', description: '多路分支路由' },
  tryCatch: { category: '流程控制', description: '脚本异常捕获路由' },
  loop: { category: '流程控制', description: '循环迭代与逐项分发' },
  code: { category: '脚本执行', description: '沙箱化脚本执行节点' },
  native: { category: '数据注入', description: '打印 payload 元数据，可选附加连接上下文' },
  timer: { category: '硬件接口', description: '按固定间隔触发工作流并注入计时元数据' },
  serialTrigger: {
    category: '硬件接口',
    description: '接收串口外设数据流并触发工作流',
  },
  modbusRead: {
    category: '硬件接口',
    description: '读取 Modbus 寄存器并将遥测数据写入 payload',
  },
  mqttClient: { category: '硬件接口', description: '发布或订阅 MQTT 消息' },
  httpClient: { category: '外部通信', description: '将 payload 发送到 HTTP 端点' },
  barkPush: { category: '外部通信', description: '向 Bark 服务发送 iOS 推送通知' },
  sqlWriter: { category: '持久化', description: '将当前 payload 持久化到本地 SQLite 表' },
  debugConsole: { category: '调试工具', description: '将 payload 打印到调试控制台以供检查' },
  subgraph: { category: '子图封装', description: '封装子拓扑为单节点并在部署前展开' },
  subgraphInput: { category: '子图封装', description: '子图展开后的入口桥接节点' },
  subgraphOutput: { category: '子图封装', description: '子图展开后的出口桥接节点' },
  c2f: { category: '纯计算', description: '摄氏转华氏（pure-form，仅 Data 引脚）' },
  minutesSince: { category: '纯计算', description: '给定 RFC3339 时间戳返回距今分钟数（pure-form，仅 Data 引脚）' },
  humanLoop: { category: '流程控制', description: '暂停工作流等待人工审批响应' },
};
