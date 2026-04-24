import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const projectRoot = join(__dirname, '../..');

export default defineConfig({
  testDir: '.',
  timeout: 60_000,
  retries: 1,
  workers: 2,
  use: {
    baseURL: 'http://localhost:1420',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: `cd "${join(projectRoot, 'src-tauri')}" && "${join(projectRoot, 'web/node_modules/.bin/tauri')}" dev --no-watch`,
    url: 'http://localhost:1420',
    timeout: 120_000,
    reuseExistingServer: true,
  },
});
