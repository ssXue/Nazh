import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 新建看板后看板卡片出现。
 */
test('新建看板后进入画布视图', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await page.locator('[data-testid="board-create"]').click();

  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
});

/**
 * 打开看板进入画布视图，部署按钮可见。
 */
test('打开看板进入画布视图', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await expect(page.locator('[data-testid="board-entry"]').first()).toBeVisible({ timeout: 5_000 });
  await page.locator('[data-testid="board-entry"]').first().click();

  await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
});

/**
 * 删除看板确认后看板消失。
 */
test('删除看板确认后看板消失', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  if (!(await page.locator('[data-testid="board-entry"]').first().isVisible())) {
    await page.locator('[data-testid="board-create"]').click();
    await expect(page.locator('[data-testid="board-entry"]').first()).toBeVisible({ timeout: 5_000 });
  }

  const countBefore = await page.locator('[data-testid="board-entry"]').count();

  await page.locator('[data-testid="board-delete"]').first().click();
  await page.locator('[data-testid="board-delete-confirm"]').click();

  const countAfter = await page.locator('[data-testid="board-entry"]').count();
  expect(countAfter).toBe(countBefore - 1);
});

/**
 * 取消删除后看板仍存在。
 */
test('取消删除后看板仍存在', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  if (!(await page.locator('[data-testid="board-entry"]').first().isVisible())) {
    await page.locator('[data-testid="board-create"]').click();
    await expect(page.locator('[data-testid="board-entry"]').first()).toBeVisible({ timeout: 5_000 });
  }

  const countBefore = await page.locator('[data-testid="board-entry"]').count();

  await page.locator('[data-testid="board-delete"]').first().click();
  await page.locator('[data-testid="board-delete-cancel"]').click();

  const countAfter = await page.locator('[data-testid="board-entry"]').count();
  expect(countAfter).toBe(countBefore);
});

/**
 * 看板面板始终显示新建按钮。
 */
test('看板面板始终显示新建按钮', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'boards');

  await expect(page.locator('[data-testid="board-create"]')).toBeVisible({ timeout: 5_000 });
});
