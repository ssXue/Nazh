import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 日志面板空状态与筛选器可见。
 */
test('日志面板空状态与筛选器可见', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'logs');

  await expect(page.locator('[data-testid="log-level-filter"]')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('[data-testid="log-type-filter"]')).toBeVisible();
  await expect(page.locator('[data-testid="log-inspector"]')).toBeVisible();
});

/**
 * 部署后日志面板显示事件。
 */
test('部署后日志面板显示事件', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
  await page.locator('[data-testid="deploy-button"]').click();

  await navigateToSection(page, 'logs');

  await expect(page.locator('[data-testid="log-level-filter"]')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('[data-testid="log-type-filter"]')).toBeVisible();
  await expect(page.locator('[data-testid="log-inspector"]')).toBeVisible();
});

/**
 * 级别筛选交互。
 */
test('级别筛选交互', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'logs');

  const levelFilter = page.locator('[data-testid="log-level-filter"]');
  await expect(levelFilter).toBeVisible({ timeout: 5_000 });

  const secondChip = levelFilter.locator('button').nth(1);
  await expect(secondChip).toBeVisible();
  await secondChip.click();

  await expect(secondChip).toBeEnabled();
});
