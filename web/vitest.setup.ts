/**
 * Vitest 全局 setup：在 `environment: 'node'` 下补齐 FlowGram 内部
 * `@flowgram.ai/variable-plugin` / `@flowgram.ai/i18n` 在模块加载期访问
 * `navigator.userAgent` / `navigator.language` 所需的 globalThis 字段。
 * 仅供测试环境使用，浏览器/Tauri webview 走原生 navigator。
 */
if (typeof globalThis.navigator === 'undefined') {
  Object.defineProperty(globalThis, 'navigator', {
    value: {
      userAgent: 'node-test',
      language: 'en-US',
      languages: ['en-US', 'en'],
    },
    writable: true,
    configurable: true,
  });
}
