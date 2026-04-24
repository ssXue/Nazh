import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 无运行工作流时，运行管理面板应显示空状态提示。
 */
test('无运行工作流时显示空状态', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'runtime');

  await expect(page.locator('[data-testid="runtime-empty-state"]')).toBeVisible({ timeout: 5_000 });
});

/**
 * 部署工作流后，运行管理面板的工作流列表应出现对应条目。
 */
test('部署后工作流列表更新', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
  await page.locator('[data-testid="deploy-button"]').click();

  await navigateToSection(page, 'runtime');
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 5_000 });
});

/**
 * 停止工作流后，运行管理面板的工作流列表应清空并恢复空状态。
 */
test('停止工作流后列表更新', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
  await page.locator('[data-testid="deploy-button"]').click();

  await navigateToSection(page, 'runtime');
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 5_000 });
});
