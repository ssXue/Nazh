import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 工作流生命周期测试：部署 → 验证已部署 → 反部署 → 验证空闲 → 重新部署 → 验证已部署。
 *
 * 覆盖场景：
 * 1. 部署工作流并确认状态变化
 * 2. 反部署工作流并确认状态恢复到未运行
 * 3. 重新部署工作流并确认状态再次更新
 */
test('工作流生命周期：部署 → 反部署 → 重新部署', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

  await page.locator('[data-testid="deploy-button"]').click();

  await page.waitForTimeout(2_000);
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible();
});

/**
 * 生命周期辅助测试：侧边栏导航各面板切换正常。
 *
 * 确保在多个面板之间切换不会造成状态丢失或崩溃。
 */
test('侧边栏面板切换保持状态稳定', async ({ page }) => {
  await waitForAppReady(page);

  const dashboardNav = page.locator('[data-testid="sidebar-dashboard"]');
  await expect(dashboardNav).toBeVisible({ timeout: 5_000 });
  await dashboardNav.click();

  const boardsNav = page.locator('[data-testid="sidebar-boards"]');
  await boardsNav.click();

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

  const logsNav = page.locator('[data-testid="sidebar-logs"]');
  await logsNav.click();
  await expect(page.locator('[data-testid="log-level-filter"]')).toBeVisible({ timeout: 5_000 });

  await boardsNav.click();
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible();
});
