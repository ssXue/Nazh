/// Copilot 消息发送通道。
///
/// CopilotPanel 在挂载时注册 sendMessage，
/// RuntimeDock 等外部组件通过 sendToCopilot 发送消息给 Copilot，
/// 无需经过 prop drilling。

type SendFn = (text: string) => void;

let _sendFn: SendFn | null = null;

/// CopilotPanel 调用：注册发送回调。
export function registerCopilotSend(fn: SendFn): () => void {
  _sendFn = fn;
  return () => { _sendFn = null; };
}

/// 外部组件调用：发送消息给 Copilot 并展开面板。
export function sendToCopilot(text: string): void {
  _sendFn?.(text);
}
