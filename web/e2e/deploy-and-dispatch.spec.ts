import { expect, test } from '@playwright/test';

/**
 * 核心链路测试：部署 AST 并发送 Payload 后收到事件和结果。
 *
 * 覆盖场景：
 * 1. 打开应用并进入看板工程
 * 2. 点击"运行"按钮部署工作流
 * 3. 验证状态标签变为"已部署"
 * 4. 点击"手动触发"按钮发送测试 Payload
 * 5. 验证执行事件流中出现节点事件
 */
test('部署 AST 并发送 Payload 后收到事件和结果', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 进入默认工程（若看板面板可见则点击第一个工程卡片）
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
  }

  // 等待画布工具栏渲染完毕，找到部署按钮
  const deployButton = page.locator('[data-testid="deploy-button"]');
  await expect(deployButton).toBeVisible({ timeout: 10_000 });
  await deployButton.click();

  // 验证工作流状态变为"已部署"
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });

  // 发送测试 Payload（手动触发按钮在已部署后才可见）
  const dispatchButton = page.locator('[data-testid="dispatch-button"]');
  await expect(dispatchButton).toBeVisible({ timeout: 5_000 });
  await dispatchButton.click();

  // 验证执行事件流中出现节点相关日志
  await expect(page.locator('[data-testid="event-feed"]')).toContainText('节点', {
    timeout: 10_000,
  });
});

/**
 * 辅助测试：在纯 Web 预览模式下验证 UI 元素可见性。
 *
 * 当后端 Tauri 运行时不可用时，应用仍应正常渲染所有控件。
 */
test('纯 Web 预览模式下核心 UI 元素可见', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 侧边栏导航项应可见
  await expect(page.locator('[data-testid="sidebar-boards"]')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('[data-testid="sidebar-dashboard"]')).toBeVisible({ timeout: 5_000 });

  // 状态标签应存在
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 5_000 });

  // 进入看板后，画布工具栏应可见
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
  }
});
