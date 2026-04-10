import { expect, test } from '@playwright/test';

/**
 * 错误处理测试：输入无效 JSON 时触发错误提示。
 *
 * 覆盖场景：
 * 1. 进入工程后导航到源配置面板
 * 2. 在 AST 编辑器中填入无效 JSON
 * 3. 尝试部署时应看到错误提示
 */
test('输入无效 JSON 后部署应显示错误信息', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 进入默认工程
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
  }

  // 导航到源配置面板
  const sourceNav = page.locator('[data-testid="sidebar-source"]');
  await expect(sourceNav).toBeVisible({ timeout: 5_000 });
  await sourceNav.click();

  // 等待 AST 编辑器出现
  const astEditor = page.locator('[data-testid="ast-editor"]');
  await expect(astEditor).toBeVisible({ timeout: 5_000 });

  // 清空编辑器并填入无效 JSON
  await astEditor.click();
  await astEditor.fill('{ 无效 JSON 内容 !!!');

  // 无效 JSON 应立即触发错误提示
  await expect(page.locator('[data-testid="error-display"]')).toBeVisible({ timeout: 5_000 });

  // 切换回画布并尝试部署
  const boardsNav = page.locator('[data-testid="sidebar-boards"]');
  await boardsNav.click();

  const deployButton = page.locator('[data-testid="deploy-button"]');
  await expect(deployButton).toBeVisible({ timeout: 10_000 });
  await deployButton.click();

  // 部署失败时状态标签不应变为"已部署"
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
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 在主界面（未进入具体工程），派发按钮不应存在或不应可用
  const dispatchButton = page.locator('[data-testid="dispatch-button"]');
  const isVisible = await dispatchButton.isVisible();

  // 若派发按钮可见则应处于禁用状态
  if (isVisible) {
    await expect(dispatchButton).toBeDisabled();
  }

  // 状态标签应始终可见且无异常
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 5_000 });
});
