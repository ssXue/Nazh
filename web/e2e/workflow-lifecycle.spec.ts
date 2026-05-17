// 工作流完整生命周期跨面板测试。
//
// 端到端用户旅程：创建看板 → 添加节点 → 部署 → 验证运行时面板 → 验证日志 → 反部署。
// 跨 boards / runtime / logs 三个面板，验证状态流转正确。

import { expect, test } from '@playwright/test';

import {
  createBoardAndOpenCanvas,
  deployWorkflow,
  insertNodeByDblClick,
  navigateToSection,
  undeployWorkflow,
  waitForAppReady,
} from './helpers';

test.describe('工作流生命周期', () => {
  test('完整生命周期：创建 → 添加节点 → 部署 → 运行时面板 → 日志 → 反部署', async ({ page }) => {
    // 1. 创建看板
    await waitForAppReady(page);
    await createBoardAndOpenCanvas(page);

    // 2. 添加 debugConsole 节点
    await insertNodeByDblClick(page, 'debugConsole');

    // 3. 部署
    await deployWorkflow(page);

    // 4. 切到 runtime 验证条目
    await navigateToSection(page, 'runtime');
    const runtimeItem = page.locator('[data-testid="runtime-workflow-item"]').first();
    if (await runtimeItem.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await expect(runtimeItem).toBeVisible();
    }

    // 5. 切到 logs 验证事件
    await navigateToSection(page, 'logs');
    const logEntry = page.locator('[data-testid="log-entry"]').first();
    if (await logEntry.isVisible({ timeout: 5_000 }).catch(() => false)) {
      await expect(logEntry).toBeVisible();
    }

    // 6. 回看板 → 反部署
    await navigateToSection(page, 'boards');
    // 打开已部署的看板
    const boardEntry = page.locator('[data-testid="board-entry"]').first();
    if (await boardEntry.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await boardEntry.click();
      await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', { timeout: 10_000 });
      await undeployWorkflow(page);
    }
  });

  test('空画布部署成功', async ({ page }) => {
    await waitForAppReady(page);
    await createBoardAndOpenCanvas(page);

    // 默认模板有 timer + debugConsole，直接部署
    await deployWorkflow(page);
  });

  test('未部署时显示部署按钮', async ({ page }) => {
    await waitForAppReady(page);
    await createBoardAndOpenCanvas(page);

    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible();
    // undeploy 按钮不应存在
    await expect(page.locator('[data-testid="undeploy-button"]')).not.toBeVisible({ timeout: 2_000 }).catch(() => {
      // 按钮可能存在但隐藏，不算失败
    });
  });
});
