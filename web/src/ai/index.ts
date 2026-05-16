/// AI 前端基础设施入口。
///
/// 所有 AI HTTP 调用从前端直接发起，Rust 引擎不再持有 HTTP 客户端。
/// API key 通过 IPC 按需从 Rust 本地配置读取。

export { loadApiKey } from './api-key';
export { createLanguageModel } from './providers';
export { aiStreamText } from './stream';
export { copilotStream } from './copilot';
export { buildCopilotTools, type CanvasOpEvent } from './copilot-tools';
export {
  extractDeviceFromText,
  extractDeviceProposal,
  extractDeviceProposalStream,
  type ExtractionProposal,
  type UncertaintyItem,
} from './device-extraction';
export { testProviderConnection, type ConnectionTestResult } from './test-connection';
export type { AiStreamOptions, StreamCallbacks, StreamResult } from './stream';
export type { CopilotCallbacks, CopilotResult, CopilotStreamOptions } from './copilot';
