/** 分配全局唯一的节点 ID。 */
export function allocateNodeId(): string {
  return crypto.randomUUID();
}
