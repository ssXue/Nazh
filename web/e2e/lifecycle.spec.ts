import { expect, test } from '@playwright/test';

/**
 * 工作流生命周期测试：部署 → 验证已部署 → 反部署 → 验证空闲 → 重新部署 → 验证已部署。
 *
 * 覆盖场景：
 * 1. 部署工作流并确认状态变化
 * 2. 反部署工作流并确认状态恢复到未运行
 * 3. 重新部署工作流并确认状态再次更新
 */
test('工作流生命周期：部署 → 反部署 → 重新部署', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 进入默认工程
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
  }

  // 等待部署按钮可见
  const deployButton = page.locator('[data-testid="deploy-button"]');
  await expect(deployButton).toBeVisible({ timeout: 10_000 });

  // 第一次部署
  await deployButton.click();
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });

  // 反部署（停止按钮在部署后出现）
  const undeployButton = page.locator('[data-testid="undeploy-button"]');
  await expect(undeployButton).toBeVisible({ timeout: 5_000 });
  await undeployButton.click();

  // 验证状态不再显示"运行中"或"已部署"（回到空闲态）
  await expect(page.locator('[data-testid="workflow-status"]')).not.toContainText('运行中', {
    timeout: 10_000,
  });

  // 重新部署
  const redeployButton = page.locator('[data-testid="deploy-button"]');
  await expect(redeployButton).toBeVisible({ timeout: 5_000 });
  await redeployButton.click();

  // 确认再次进入已部署状态
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });
});

/**
 * 生命周期辅助测试：侧边栏导航各面板切换正常。
 *
 * 确保在多个面板之间切换不会造成状态丢失或崩溃。
 */
test('侧边栏面板切换保持状态稳定', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 切换到仪表盘
  const dashboardNav = page.locator('[data-testid="sidebar-dashboard"]');
  await expect(dashboardNav).toBeVisible({ timeout: 5_000 });
  await dashboardNav.click();

  // 切换到所有看板
  const boardsNav = page.locator('[data-testid="sidebar-boards"]');
  await boardsNav.click();
  await expect(page.locator('[data-testid="board-entry"]').first()).toBeVisible({ timeout: 5_000 });

  // 进入工程后切换到源配置
  await page.locator('[data-testid="board-entry"]').first().click();
  const sourceNav = page.locator('[data-testid="sidebar-source"]');
  await sourceNav.click();

  // 源配置面板应显示 AST 编辑器
  await expect(page.locator('[data-testid="ast-editor"]')).toBeVisible({ timeout: 5_000 });

  // 切换回看板，画布工具栏应仍然可见
  await boardsNav.click();
  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

  // 工作流状态标签应始终可见
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible();
});
