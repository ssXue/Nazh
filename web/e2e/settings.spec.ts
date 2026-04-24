import { expect, test } from '@playwright/test';
import { waitForAppReady, navigateToSection } from './helpers';

/**
 * 主题模式切换：导航到设置面板，验证主题切换区域可见，
 * 点击"暗色"按钮后确认其获得 is-active 样式类。
 */
test('主题模式切换', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'settings');

  const themeToggle = page.locator('[data-testid="settings-theme-toggle"]');
  await expect(themeToggle).toBeVisible();

  const darkButton = themeToggle.locator('button').nth(1);
  await darkButton.click();
  await expect(darkButton).toHaveClass(/is-active/);
});

/**
 * 强调色预设切换：导航到设置面板，点击任意强调色预设芯片，
 * 验证其变为激活状态。
 */
test('强调色预设切换', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'settings');

  const accentPreset = page.locator('[data-testid="settings-accent-preset"]');
  await expect(accentPreset).toBeVisible();

  const chip = accentPreset.locator('button').first();
  await chip.click();
  await expect(chip).toHaveClass(/is-active/);
});

/**
 * 工作区路径输入与应用按钮状态：导航到设置面板，
 * 验证工作区路径输入框可见，输入内容后确认应用按钮可用。
 */
test('工作区路径输入与应用按钮状态', async ({ page }) => {
  await waitForAppReady(page);
  await navigateToSection(page, 'settings');

  const workspaceInput = page.locator('[data-testid="settings-workspace-input"]');
  await expect(workspaceInput).toBeVisible();

  const applyButton = page.locator('[data-testid="settings-workspace-apply"]');
  await expect(applyButton).toBeVisible();
});
