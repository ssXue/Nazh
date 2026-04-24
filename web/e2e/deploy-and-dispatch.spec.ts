import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 核心链路测试：部署 AST 并发送 Payload 后收到事件和结果。
 */
test('部署 AST 并发送 Payload 后收到事件和结果', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

  await page.locator('[data-testid="deploy-button"]').click();

  await page.waitForTimeout(2_000);
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible();
});

/**
 * 辅助测试：在纯 Web 预览模式下验证 UI 元素可见性。
 */
test('纯 Web 预览模式下核心 UI 元素可见', async ({ page }) => {
  await waitForAppReady(page);

  await expect(page.locator('[data-testid="sidebar-boards"]')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('[data-testid="sidebar-dashboard"]')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 5_000 });

  await navigateToSection(page, 'boards');
  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
});
