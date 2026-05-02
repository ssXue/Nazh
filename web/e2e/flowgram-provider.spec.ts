import { expect, test } from '@playwright/test';
import { navigateToSection, waitForAppReady } from './helpers';

const FLOWGRAM_PROVIDER_FAILURE = /FlowRendererRegistry|Ambiguous match found for serviceIdentifier/i;

/**
 * FlowGram Provider 初始化 smoke test。
 *
 * Playwright 当前跑在 Chromium 页面而非 Tauri webview；本用例只守住画布
 * Provider 初始化不因重复 DI binding 崩溃，不断言任何 IPC 真值。
 */
test('打开画布时 FlowGram Provider 不重复注册 renderer 服务', async ({ page }) => {
  const failures: string[] = [];

  page.on('pageerror', (error) => {
    const message = error.message;
    if (FLOWGRAM_PROVIDER_FAILURE.test(message)) {
      failures.push(message);
    }
  });
  page.on('console', (message) => {
    if (message.type() !== 'error') return;
    const text = message.text();
    if (FLOWGRAM_PROVIDER_FAILURE.test(text)) {
      failures.push(text);
    }
  });

  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  const firstBoard = page.locator('[data-testid="board-entry"]').first();
  if (await firstBoard.isVisible()) {
    await firstBoard.click();
  } else {
    await page.locator('[data-testid="board-create"]').click();
  }

  await expect(page.locator('.flowgram-editor')).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(500);

  expect(failures).toEqual([]);
});
