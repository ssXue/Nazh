// Copilot 副驾驶面板烟雾测试。
//
// 预览模式下（hasTauriRuntime() === false）发送消息会收到
// "预览模式：AI 不可用" 回落文案。测试验证：
// - 面板可见性和输入框
// - 历史下拉开关
// - 新建本地对话
// - 预览模式回落消息
// - 折叠/展开切换

import { expect, test } from '@playwright/test';

import { navigateToSection, waitForAppReady } from './helpers';

test.describe('Copilot 副驾驶面板', () => {
  test.beforeEach(async ({ page }) => {
    await waitForAppReady(page);
    // Copilot 面板在 boards 视图下可见
    await navigateToSection(page, 'boards');
  });

  test('面板可见 + 输入框', async ({ page }) => {
    const panel = page.locator('[data-testid="copilot-panel"]');
    await expect(panel).toBeVisible({ timeout: 5_000 });

    const input = page.locator('[data-testid="copilot-input"]');
    await expect(input).toBeVisible();
  });

  test('历史下拉开关', async ({ page }) => {
    await page.locator('[data-testid="copilot-toggle-history"]').click();
    const dropdown = page.locator('[data-testid="copilot-history-dropdown"]');
    await expect(dropdown).toBeVisible();
  });

  test('新建本地对话', async ({ page }) => {
    await page.locator('[data-testid="copilot-new-conversation"]').click();
    // 新建对话后输入框仍然可见
    const input = page.locator('[data-testid="copilot-input"]');
    await expect(input).toBeVisible();
  });

  test('预览模式回落消息', async ({ page }) => {
    const input = page.locator('[data-testid="copilot-input"]');
    await expect(input).toBeVisible({ timeout: 5_000 });
    await input.fill('测试消息');

    const sendButton = page.locator('[data-testid="copilot-send"]');
    await expect(sendButton).toBeEnabled();
    await sendButton.click();

    // 预览模式下应出现回落消息
    const message = page.locator('[data-testid="copilot-message"]').first();
    await expect(message).toBeVisible({ timeout: 5_000 });
    await expect(message).toContainText('预览模式');
  });

  test('折叠和展开', async ({ page }) => {
    const collapseBtn = page.locator('[data-testid="copilot-collapse"]');
    await expect(collapseBtn).toBeVisible({ timeout: 5_000 });
    await collapseBtn.click();

    // 折叠后显示已折叠按钮
    const collapsedBtn = page.locator('[data-testid="copilot-collapsed"]');
    await expect(collapsedBtn).toBeVisible();

    // 点击展开
    await collapsedBtn.click();
    await expect(page.locator('[data-testid="copilot-panel"]')).toBeVisible({ timeout: 5_000 });
  });
});
