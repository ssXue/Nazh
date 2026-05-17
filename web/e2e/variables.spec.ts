// 运行时变量和全局变量面板烟雾测试。
//
// 预览模式下工作流变量显示"未选中已部署的工作流"空状态；
// 全局变量显示"暂无全局变量"空状态。添加表单可展开/收起。

import { expect, test } from '@playwright/test';

import { createBoardAndOpenCanvas } from './helpers';

test.describe('变量面板', () => {
  test.beforeEach(async ({ page }) => {
    await createBoardAndOpenCanvas(page);
    // 切换到 variables tab
    await page.locator('[data-testid="runtime-dock-tab-variables"]').click();
  });

  test('运行时变量空状态', async ({ page }) => {
    // 默认在工作流变量子 tab
    await expect(page.locator('[data-testid="runtime-variable-empty-state"]')).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('[data-testid="runtime-variable-empty-state"]')).toContainText('未选中');
  });

  test('全局变量空状态', async ({ page }) => {
    await page.locator('[data-testid="variable-tab-global"]').click();
    await expect(page.locator('[data-testid="global-variables-panel"]')).toBeVisible({ timeout: 5_000 });
    // 预览模式下无全局变量
    await expect(page.getByText('暂无全局变量')).toBeVisible();
  });

  test('全局变量添加按钮展开表单', async ({ page }) => {
    await page.locator('[data-testid="variable-tab-global"]').click();
    const addBtn = page.locator('[data-testid="global-variable-add-btn"]');
    await expect(addBtn).toBeVisible({ timeout: 5_000 });
    await addBtn.click();

    // 表单出现
    await expect(page.locator('.global-variables-panel__add-form')).toBeVisible();
  });

  test('全局变量添加取消关闭表单', async ({ page }) => {
    await page.locator('[data-testid="variable-tab-global"]').click();
    await page.locator('[data-testid="global-variable-add-btn"]').click();
    await expect(page.locator('.global-variables-panel__add-form')).toBeVisible();

    // 点击取消
    await page.locator('.global-variables-panel__add-form').getByRole('button', { name: '取消' }).click();
    await expect(page.locator('.global-variables-panel__add-form')).not.toBeVisible();
    // 添加按钮重新出现
    await expect(page.locator('[data-testid="global-variable-add-btn"]')).toBeVisible();
  });
});
