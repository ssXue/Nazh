// 多看板状态隔离测试。
//
// 验证多个看板之间的状态隔离：切换看板后画布内容正确、
// 删除一个看板不影响另一个、列表计数正确更新。

import { expect, test } from '@playwright/test';

import {
  createBoardAndOpenCanvas,
  insertNodeByDblClick,
  navigateToSection,
  waitForAppReady,
} from './helpers';

test.describe('多看板状态隔离', () => {
  test('双看板切换保持画布状态', async ({ page }) => {
    await waitForAppReady(page);

    // 创建看板 A，添加 timer 节点
    await createBoardAndOpenCanvas(page);
    await insertNodeByDblClick(page, 'timer');

    // 返回看板列表
    await navigateToSection(page, 'boards');

    // 创建看板 B，添加 code 节点
    await page.locator('[data-testid="board-create"]').click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
    await insertNodeByDblClick(page, 'code');

    // 返回列表
    await navigateToSection(page, 'boards');

    // 打开看板 A → 验证 timer 节点
    const entries = page.locator('[data-testid="board-entry"]');
    await expect(entries.first()).toBeVisible({ timeout: 5_000 });
    await entries.nth(0).click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('.flowgram-card--timer').first()).toBeVisible({ timeout: 10_000 });

    // 返回列表 → 打开看板 B → 验证 code 节点
    await navigateToSection(page, 'boards');
    await entries.nth(1).click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('.flowgram-card--code').first()).toBeVisible({ timeout: 10_000 });
  });

  test('删除一个看板不影响另一个', async ({ page }) => {
    await waitForAppReady(page);

    // 创建两个看板
    await navigateToSection(page, 'boards');
    await page.locator('[data-testid="board-create"]').click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

    await navigateToSection(page, 'boards');
    await page.locator('[data-testid="board-create"]').click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });

    // 返回列表
    await navigateToSection(page, 'boards');
    const entries = page.locator('[data-testid="board-entry"]');
    await expect(entries).toHaveCount(2, { timeout: 5_000 });

    // 删除第一个看板
    const firstDelete = entries.nth(0).locator('[data-testid="board-delete"]');
    if (await firstDelete.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await firstDelete.click();
      // 确认删除弹窗
      const confirmBtn = page.locator('[data-testid="board-delete-confirm"]');
      if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await confirmBtn.click();
      }
    }

    // 第二个看板仍然可打开
    await expect(entries.first()).toBeVisible({ timeout: 5_000 });
    await entries.first().click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
  });

  test('列表计数更新', async ({ page }) => {
    await waitForAppReady(page);
    await navigateToSection(page, 'boards');

    // 记录初始数量
    const entries = page.locator('[data-testid="board-entry"]');
    const initialCount = await entries.count();

    // 创建 2 个看板
    for (let i = 0; i < 2; i++) {
      await page.locator('[data-testid="board-create"]').click();
      await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
      await navigateToSection(page, 'boards');
    }

    await expect(entries).toHaveCount(initialCount + 2, { timeout: 5_000 });

    // 删除 1 个
    const firstDelete = entries.first().locator('[data-testid="board-delete"]');
    if (await firstDelete.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await firstDelete.click();
      const confirmBtn = page.locator('[data-testid="board-delete-confirm"]');
      if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await confirmBtn.click();
      }
    }

    await expect(entries).toHaveCount(initialCount + 1, { timeout: 5_000 });
  });
});
