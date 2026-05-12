import { customAlphabet } from 'nanoid';

/// 与 FlowGram 内部默认 ID 格式保持一致：1 + 5 位随机数字（6 位纯数字）。
/// 见 @flowgram.ai/free-layout-core 中 createWorkflowNodeByType 的 id 生成逻辑。
const generateNodeId = customAlphabet('1234567890', 5);

/** 分配全局唯一的节点 ID（6 位纯数字，与手动拖拽创建的节点一致）。 */
export function allocateNodeId(): string {
  return `1${generateNodeId()}`;
}
