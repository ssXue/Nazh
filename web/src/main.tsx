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

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <App />
    </AppErrorBoundary>
  </React.StrictMode>,
);

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    cleanupDesktopShellGuards();
  });
}
