import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 空连接列表显示空状态。
 *
 * 导航到连接工作室后，检查是否显示空状态提示或已有连接卡片。
 * 若为空状态，则验证提示文本包含"暂无连接"相关描述。
 */
test('空连接列表显示空状态', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'connections');

  const emptyState = page.locator('[data-testid="connection-empty-state"]');
  const connectionCards = page.locator('[data-testid="connection-card"]');

  if (await emptyState.isVisible()) {
    await expect(emptyState).toContainText(/暂无|没有|空/);
  } else {
    await expect(connectionCards.first()).toBeVisible();
  }
});

/**
 * 添加连接后卡片出现。
 *
 * 点击第一个连接模板按钮（Modbus TCP）后，验证连接卡片网格中出现新的连接卡片。
 */
test('添加连接后卡片出现', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'connections');

  await page.locator('[data-testid="connection-add"]').first().click();

  await expect(page.locator('[data-testid="connection-card"]').first()).toBeVisible({
    timeout: 5_000,
  });
});

/**
 * 添加多种类型连接。
 *
 * 依次点击 Modbus TCP 和串口设备模板按钮，验证网格中存在两张连接卡片。
 */
test('添加多种类型连接', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'connections');

  await page.locator('[data-testid="connection-add"]').first().click();
  await expect(page.locator('[data-testid="connection-card"]').first()).toBeVisible({
    timeout: 5_000,
  });

  await page.keyboard.press('Escape');

  await page.locator('[data-testid="connection-add"]').nth(1).click();

  await expect(page.locator('[data-testid="connection-card"]')).toHaveCount(2, {
    timeout: 5_000,
  });
});

/**
 * 打开连接设置面板。
 *
 * 添加一个连接后，点击连接卡片上的设置按钮，验证右侧设置面板中出现删除按钮。
 */
test('打开连接设置面板', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'connections');

  await page.locator('[data-testid="connection-add"]').first().click();
  await expect(page.locator('[data-testid="connection-card"]').first()).toBeVisible({
    timeout: 5_000,
  });

  await page.keyboard.press('Escape');

  const card = page.locator('[data-testid="connection-card"]').first();
  await card.locator('.connection-card__settings').click({ force: true });

  await expect(page.locator('[data-testid="connection-delete"]')).toBeVisible({
    timeout: 5_000,
  });
});
