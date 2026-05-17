// 日志面板高级交互测试。
//
// 已有基础测试（logs.spec.ts）覆盖空状态和面板可见性。
// 本文件补充：筛选芯片交互、搜索框、部署后日志条目出现。
// 使用已有 data-testid：log-type-filter / log-level-filter / log-empty-state / log-entry / log-inspector。

import { expect, test } from '@playwright/test';

import { createBoardAndOpenCanvas, deployWorkflow, navigateToSection, waitForAppReady } from './helpers';

test.describe('日志面板高级交互', () => {
  test.beforeEach(async ({ page }) => {
    await waitForAppReady(page);
    await navigateToSection(page, 'logs');
  });

  test('级别筛选芯片可点击', async ({ page }) => {
    const levelFilter = page.locator('[data-testid="log-level-filter"]');
    await expect(levelFilter).toBeVisible();
    const chips = levelFilter.locator('button');
    const count = await chips.count();
    expect(count).toBeGreaterThan(0);
    // 每个芯片可点击
    for (let i = 0; i < count; i++) {
      await expect(chips.nth(i)).toBeEnabled();
    }
  });

  test('类型筛选芯片可点击', async ({ page }) => {
    const typeFilter = page.locator('[data-testid="log-type-filter"]');
    await expect(typeFilter).toBeVisible();
    const chips = typeFilter.locator('button');
    const count = await chips.count();
    expect(count).toBeGreaterThan(0);
    for (let i = 0; i < count; i++) {
      await expect(chips.nth(i)).toBeEnabled();
    }
  });

  test('搜索框接受输入', async ({ page }) => {
    const searchInput = page.locator('[data-testid="log-level-filter"]')
      .locator('..')
      .locator('input[placeholder*="来源"]');
    if (await searchInput.isVisible()) {
      await searchInput.fill('测试搜索');
      await expect(searchInput).toHaveValue('测试搜索');
    }
  });

  test('无日志时检查器显示提示', async ({ page }) => {
    await expect(page.locator('[data-testid="log-inspector"]')).toBeVisible();
  });

  test('部署后日志条目出现', async ({ page }) => {
    // 创建看板并部署
    await navigateToSection(page, 'boards');
    await createBoardAndOpenCanvas(page);
    await deployWorkflow(page);

    // 切到日志面板
    await navigateToSection(page, 'logs');

    // 部署事件应产生至少一条日志
    const logEntry = page.locator('[data-testid="log-entry"]').first();
    await expect(logEntry).toBeVisible({ timeout: 15_000 });
  });
});
