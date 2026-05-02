import 'reflect-metadata';
import '@flowgram.ai/free-layout-editor/index.css';

import React from 'react';
import ReactDOM from 'react-dom/client';

import App from './App';
import { AppErrorBoundary } from './components/app/AppErrorBoundary';
import { validateNodeRegistry } from './components/flowgram/flowgram-node-library';
import { installDesktopShellGuards } from './lib/tauri';
import './styles.css';

validateNodeRegistry();

const cleanupDesktopShellGuards = installDesktopShellGuards();

// FlowGram 1.0.11 的 Provider 在 root StrictMode 开发态双挂载下会重复注册
// FlowRendererRegistry；StrictMode 只放到不包含画布 Provider 的自有 UI 子树。
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <AppErrorBoundary>
    <App />
  </AppErrorBoundary>,
);

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    cleanupDesktopShellGuards();
  });
}
