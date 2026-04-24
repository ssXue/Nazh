import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * AI 配置面板基本元素可见。
 *
 * 导航到 AI 配置面板后，验证"添加连接"按钮与"Agent 参数"按钮均可见。
 */
test('AI 配置面板基本元素可见', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'ai');

  await expect(page.locator('[data-testid="ai-provider-add"]')).toBeVisible();
  await expect(page.locator('[data-testid="ai-agent-settings"]')).toBeVisible();
});

/**
 * 添加提供商表单打开与预设选择。
 *
 * 点击"添加连接"按钮后，验证抽屉面板打开并显示"添加提供商"标题。
 * 点击第一个预设按钮后，确认表单字段被自动填充。
 */
test('添加提供商表单打开与预设选择', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'ai');

  await page.locator('[data-testid="ai-provider-add"]').click();
  await expect(page.getByRole('heading', { name: '添加提供商' })).toBeVisible({ timeout: 5_000 });

  const firstPreset = page.locator('[data-testid="ai-provider-preset"]').first();
  await expect(firstPreset).toBeVisible();
  await firstPreset.click();

  const nameInput = page.locator('#ai-provider-name');
  await expect(nameInput).not.toBeEmpty();
});

/**
 * Agent 参数对话框打开。
 *
 * 点击"Agent 参数"按钮后，验证对话框打开并显示"全局 Agent 参数"标题，
 * 同时确认 temperature 和 maxTokens 字段可见。
 */
test('Agent 参数对话框打开', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'ai');

  await page.locator('[data-testid="ai-agent-settings"]').click();
  await expect(page.getByRole('heading', { name: '全局 Agent 参数' })).toBeVisible({ timeout: 5_000 });

  await expect(page.locator('#ai-agent-temperature')).toBeVisible();
  await expect(page.locator('#ai-agent-max-tokens')).toBeVisible();
});
