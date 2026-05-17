// Runtime Dock 运行观测窗体烟雾测试。
//
// RuntimeDock 有 5 个 Tab（events/results/payload/connections/variables）。
// 需要先打开看板才能看到 RuntimeDock。
// 预览模式下无真实运行数据，验证 Tab 切换和空状态 UI。

import { expect, test } from '@playwright/test';

import { createBoardAndOpenCanvas } from './helpers';

test.describe('Runtime Dock 运行观测窗体', () => {
  test.beforeEach(async ({ page }) => {
    await createBoardAndOpenCanvas(page);
  });

  test('默认 events tab 激活', async ({ page }) => {
    const eventsTab = page.locator('[data-testid="runtime-dock-tab-events"]');
    await expect(eventsTab).toBeVisible({ timeout: 5_000 });
    await expect(eventsTab).toHaveClass(/is-active/);
  });

  test('切换到 results tab 显示空列表', async ({ page }) => {
    await page.locator('[data-testid="runtime-dock-tab-results"]').click();
    await expect(page.locator('[data-testid="result-list"]')).toBeVisible();
  });

  test('切换到 payload tab 显示编辑器', async ({ page }) => {
    await page.locator('[data-testid="runtime-dock-tab-payload"]').click();
    // payload 编辑器区域可见
    await expect(page.locator('.runtime-dock__payload-editor')).toBeVisible({ timeout: 5_000 });
  });

  test('切换到 connections tab 显示空状态', async ({ page }) => {
    await page.locator('[data-testid="runtime-dock-tab-connections"]').click();
    await expect(page.getByText('暂无连接占用')).toBeVisible({ timeout: 5_000 });
  });

  test('variables tab 有工作流和全局子 tab', async ({ page }) => {
    await page.locator('[data-testid="runtime-dock-tab-variables"]').click();
    await expect(page.locator('[data-testid="variable-tab-workflow"]')).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('[data-testid="variable-tab-global"]')).toBeVisible();
  });

  test('无事件时复制按钮禁用', async ({ page }) => {
    const copyBtn = page.locator('[data-testid="runtime-dock-copy-events"]');
    await expect(copyBtn).toBeVisible({ timeout: 5_000 });
    await expect(copyBtn).toBeDisabled();
  });

  test('分栏按钮创建第二列', async ({ page }) => {
    // 找到第一个可见的分栏按钮（events tab 的 split）
    const splitBtn = page.locator('.runtime-dock__tab-split').first();
    await expect(splitBtn).toBeVisible({ timeout: 5_000 });
    await splitBtn.click();

    // 分栏后应出现多列布局
    await expect(page.locator('.runtime-dock__columns')).toHaveClass(/is-multi/);
  });
});
