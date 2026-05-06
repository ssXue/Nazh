/** 分配 `${prefix}_${index}` 形式的稳定 ID，可用于节点、设备等实体。 */
export function allocateNodeId(prefix: string, usedIds: ReadonlySet<string>): string {
  let index = 1;
  while (usedIds.has(`${prefix}_${index}`)) {
    index += 1;
  }

  return `${prefix}_${index}`;
}
