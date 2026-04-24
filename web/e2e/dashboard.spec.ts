import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 仪表盘默认统计数据显示。
 *
 * 验证仪表盘面板中统计卡片全部可见，且就绪状态仪表盘
 * 显示"待部署"或"已就绪"。
 */
test('仪表盘默认统计数据显示', async ({ page }) => {
  await waitForAppReady(page);

  await navigateToSection(page, 'dashboard');

  const statCards = page.locator('.dashboard-stat-card');
  await expect(statCards.first()).toBeVisible({ timeout: 5_000 });

  const readinessText = page.locator('.dashboard-gauge__center span');
  await expect(readinessText.last()).toContainText(/待部署|已就绪/);
});

/**
 * 导航到看板按钮可用。
 *
 * 在仪表盘面板点击"所有看板"导航按钮后，
 * 应切换到看板列表面板并显示看板条目或创建按钮。
 */
test('导航到看板按钮可用', async ({ page }) => {
  await waitForAppReady(page);

  await navigateToSection(page, 'dashboard');

  const navigateButton = page.locator('[data-testid="dashboard-navigate-boards"]');
  await expect(navigateButton).toBeVisible({ timeout: 5_000 });
  await navigateButton.click();

  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  const boardCreate = page.locator('[data-testid="board-create"]');
  await expect(boardEntry.or(boardCreate).first()).toBeVisible({ timeout: 5_000 });
});
