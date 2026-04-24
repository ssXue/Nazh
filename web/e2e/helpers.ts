import { expect, type Page } from '@playwright/test';

/**
 * 等待应用加载完成。
 */
export async function waitForAppReady(page: Page) {
  await page.goto('/');
  await page.waitForLoadState('networkidle');
  await expect(page.locator('[data-testid="workflow-status"]')).toBeVisible({ timeout: 10_000 });
}

/**
 * 通过侧边栏导航到指定面板。
 */
export async function navigateToSection(page: Page, sectionKey: string) {
  const nav = page.locator(`[data-testid="sidebar-${sectionKey}"]`);
  await expect(nav).toBeVisible({ timeout: 5_000 });
  await nav.click();
}

/**
 * 打开第一个看板工程。
 */
export async function openFirstBoard(page: Page) {
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({ timeout: 10_000 });
  }
}

/**
 * 部署当前工作流，等待已部署状态。
 */
export async function deployWorkflow(page: Page) {
  const deployButton = page.locator('[data-testid="deploy-button"]');
  await expect(deployButton).toBeVisible({ timeout: 10_000 });
  await deployButton.click();
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });
}

/**
 * 卸载当前工作流，等待非已部署状态。
 */
export async function undeployWorkflow(page: Page) {
  const undeployButton = page.locator('[data-testid="undeploy-button"]');
  await expect(undeployButton).toBeVisible({ timeout: 5_000 });
  await undeployButton.click();
  await expect(page.locator('[data-testid="workflow-status"]')).not.toContainText('已部署', {
    timeout: 10_000,
  });
}
