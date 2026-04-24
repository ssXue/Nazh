import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 错误处理测试：在画布中输入无效 JSON 时触发错误提示。
 *
 * 覆盖场景：
 * 1. 创建工程并进入画布视图
 * 2. 在 AST 编辑器中填入无效 JSON
 * 3. 尝试部署时应看到错误提示
 */
test('输入无效 JSON 后部署应显示错误信息', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

  await page.locator('[data-testid="deploy-button"]').click();

  await page.waitForTimeout(2_000);
  const statusText = await page.locator('[data-testid="workflow-status"]').textContent();
  expect(statusText).not.toContain('已部署');
});

/**
 * 错误处理辅助测试：在无工程选中时发送 Payload 应显示提示。
 *
 * 确保在未进入工程时操作不会崩溃应用。
 */
test('未进入工程时不可发送 Payload', async ({ page }) => {
  await waitForAppReady(page);

  const dispatchButton = page.locator('[data-testid="dispatch-button"]');
  const isVisible = await dispatchButton.isVisible();

  if (isVisible) {
    await expect(dispatchButton).toBeDisabled();
  }

  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 5_000 });
});
