export function allocateNodeId(prefix: string, usedIds: ReadonlySet<string>): string {
  let index = 1;
  while (usedIds.has(`${prefix}_${index}`)) {
    index += 1;
  }

  return `${prefix}_${index}`;
}
