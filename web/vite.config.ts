import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

function vendorChunk(id: string): string | undefined {
  if (!id.includes('/node_modules/')) {
    return undefined;
  }

  if (id.includes('/@flowgram.ai/')) {
    return 'vendor-flowgram';
  }

  if (id.includes('/@tauri-apps/')) {
    return 'vendor-tauri';
  }

  if (id.includes('/react-json-view-lite/') || id.includes('/js-yaml/')) {
    return 'vendor-data';
  }

  if (id.includes('/modern-screenshot/')) {
    return 'vendor-export';
  }

  return undefined;
}

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    chunkSizeWarningLimit: 6500,
    rollupOptions: {
      output: {
        manualChunks: vendorChunk,
      },
    },
  },
  server: {
    host: '0.0.0.0',
    port: 1420,
    strictPort: true,
  },
  preview: {
    host: '0.0.0.0',
    port: 1420,
  },
});
