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

// Node.js ≥ 25 暴露原生 localStorage（只读代理），遮蔽 jsdom 环境注入的完整实现。
// jsdom 环境下用 jsdom 自带 Storage 替换；node 环境下提供轻量 stub。
// 详见 vitest-dev/vitest#8757
if (typeof globalThis.localStorage?.setItem !== 'function') {
  const { JSDOM } = await import('jsdom');
  const dom = new JSDOM('', { url: 'http://localhost' });
  globalThis.localStorage = dom.window.localStorage;
  globalThis.sessionStorage = dom.window.sessionStorage;
}
