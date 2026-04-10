import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  timeout: 60_000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:1420',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'cd ../src-tauri && ../web/node_modules/.bin/tauri dev --no-watch',
    url: 'http://localhost:1420',
    timeout: 120_000,
    reuseExistingServer: true,
  },
});
