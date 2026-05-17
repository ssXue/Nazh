// 基础设施面板（设备/连接 Tab 切换 + 导入抽屉）烟雾测试。
//
// InfrastructurePanel 有两个 Tab：设备（默认）和连接。
// 预览模式下设备面板显示"需要 Tauri 桌面运行时"空状态。
// 导入抽屉点击"接入设备"后打开，可关闭。

import { expect, test } from '@playwright/test';

import { navigateToSection, waitForAppReady } from './helpers';

test.describe('基础设施面板', () => {
  test.beforeEach(async ({ page }) => {
    await waitForAppReady(page);
    await navigateToSection(page, 'connections');
  });

  test('默认显示设备 tab 为激活状态', async ({ page }) => {
    const devicesTab = page.locator('[data-testid="infra-tab-devices"]');
    await expect(devicesTab).toHaveClass(/is-active/);
  });

  test('切换到连接 tab 显示 ConnectionStudio', async ({ page }) => {
    await page.locator('[data-testid="infra-tab-connections"]').click();
    const connectionsTab = page.locator('[data-testid="infra-tab-connections"]');
    await expect(connectionsTab).toHaveClass(/is-active/);
  });

  test('切回设备 tab 显示设备面板', async ({ page }) => {
    await page.locator('[data-testid="infra-tab-connections"]').click();
    await page.locator('[data-testid="infra-tab-devices"]').click();
    const devicesTab = page.locator('[data-testid="infra-tab-devices"]');
    await expect(devicesTab).toHaveClass(/is-active/);
    // 预览模式下显示空状态
    await expect(page.locator('[data-testid="device-empty-state"]')).toBeVisible();
  });

  test('导入抽屉打开和关闭', async ({ page }) => {
    const importButton = page.locator('[data-testid="infra-import-button"]');
    await expect(importButton).toBeVisible();
    await importButton.click();

    // 抽屉出现
    const drawer = page.locator('[data-testid="device-import-drawer"]');
    await expect(drawer).toBeVisible();

    // 关闭抽屉
    const closeButton = page.locator('[data-testid="device-import-close"]');
    await closeButton.click();
    await expect(drawer).not.toBeVisible();
  });

  test('预览模式设备空状态', async ({ page }) => {
    await expect(page.locator('[data-testid="device-empty-state"]')).toBeVisible();
    await expect(page.locator('[data-testid="device-empty-state"]')).toContainText('Tauri');
  });
});
