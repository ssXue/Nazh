import { allocateScopedId } from './id-allocation';

/** 兼容旧命名：后续逐步迁移到 allocateScopedId。 */
export const allocateNodeId = allocateScopedId;

export { allocateScopedId };
