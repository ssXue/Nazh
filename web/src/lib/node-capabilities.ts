// 节点能力标签位图常量表，与 Rust 侧 `nazh_core::NodeCapabilities` 严格对应。
// 位分配由 ADR-0011 锁死；新增/修改须与 crates/core/src/node.rs 同步。

export const NODE_CAPABILITY_FLAGS = {
  PURE: 1 << 0,
  NETWORK_IO: 1 << 1,
  FILE_IO: 1 << 2,
  DEVICE_IO: 1 << 3,
  TRIGGER: 1 << 4,
  BRANCHING: 1 << 5,
  MULTI_OUTPUT: 1 << 6,
  BLOCKING: 1 << 7,
} as const;

export type NodeCapabilityName = keyof typeof NODE_CAPABILITY_FLAGS;

export const NODE_CAPABILITY_LABELS: Record<NodeCapabilityName, string> = {
  PURE: '纯计算',
  NETWORK_IO: '网络 IO',
  FILE_IO: '文件 IO',
  DEVICE_IO: '设备 IO',
  TRIGGER: '触发器',
  BRANCHING: '分支',
  MULTI_OUTPUT: '多输出',
  BLOCKING: '阻塞',
};

export function hasCapability(
  bits: number,
  flag: NodeCapabilityName,
): boolean {
  return (bits & NODE_CAPABILITY_FLAGS[flag]) !== 0;
}

export function capabilityNames(bits: number): NodeCapabilityName[] {
  return (
    Object.keys(NODE_CAPABILITY_FLAGS) as NodeCapabilityName[]
  ).filter((name) => hasCapability(bits, name));
}
