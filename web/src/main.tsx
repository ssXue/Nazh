import 'reflect-metadata';
import '@flowgram.ai/free-layout-editor/index.css';

import React from 'react';
import ReactDOM from 'react-dom/client';

import App from './App';
import { installDesktopShellGuards } from './lib/tauri';
import './styles.css';

const cleanupDesktopShellGuards = installDesktopShellGuards();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    cleanupDesktopShellGuards();
  });
}
