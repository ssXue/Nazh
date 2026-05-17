// Flow 控制节点插入烟雾测试。
//
// 通过双击节点添加面板卡片（dispatchEvent('dblclick')）插入节点，
// 验证画布渲染出对应 CSS class 的节点卡片。
// 容器节点（loop/subgraph）使用 FlowgramContainerCard 渲染；
// 桥接节点（subgraphInput/subgraphOutput）仅在容器内自动创建，不从面板直接插入。
//
// 不在范围内：
// - 节点内部逻辑/配置（由 Vitest + Rust 单测覆盖）
// - PinKind/PinType 的 IPC 真值（Playwright 跑 Chromium，IPC fallback 为 Any/Any/exec）

import { expect, test } from '@playwright/test';

import { createBoardAndOpenCanvas, insertNodeByDblClick } from './helpers';

test.describe('Flow 控制节点插入', () => {
  test.beforeEach(async ({ page }) => {
    await createBoardAndOpenCanvas(page);
  });

  test('if: 渲染卡片 + true/false 分支端口', async ({ page }) => {
    const card = await insertNodeByDblClick(page, 'if');
    await expect(card.locator('[data-port-id="true"]')).toBeVisible();
    await expect(card.locator('[data-port-id="false"]')).toBeVisible();
  });

  test('switch: 渲染卡片 + default 分支端口', async ({ page }) => {
    const card = await insertNodeByDblClick(page, 'switch');
    await expect(card.locator('[data-port-id="default"]')).toBeVisible();
  });

  test('tryCatch: 渲染卡片 + try/catch 分支端口', async ({ page }) => {
    const card = await insertNodeByDblClick(page, 'tryCatch');
    await expect(card.locator('[data-port-id="try"]')).toBeVisible();
    await expect(card.locator('[data-port-id="catch"]')).toBeVisible();
  });

  test('loop: 渲染容器卡片', async ({ page }) => {
    // loop 是容器节点，使用 FlowgramContainerCard 渲染
    const addCard = page.locator('.flowgram-add-card--loop').first();
    await addCard.dispatchEvent('dblclick');
    const containerCard = page.locator('.flowgram-card--loop').first();
    await expect(containerCard).toBeVisible({ timeout: 10_000 });
  });

  test('code: 渲染卡片', async ({ page }) => {
    await insertNodeByDblClick(page, 'code');
  });

  test('subgraph: 渲染容器卡片 + 自动创建桥接节点', async ({ page }) => {
    const addCard = page.locator('.flowgram-add-card--subgraph').first();
    await addCard.dispatchEvent('dblclick');
    const containerCard = page.locator('.flowgram-card--subgraph').first();
    await expect(containerCard).toBeVisible({ timeout: 10_000 });

    // subgraph 自动创建 subgraphInput / subgraphOutput 桥接节点
    await expect(page.locator('.flowgram-card--bridge.flowgram-card--subgraphInput').first()).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('.flowgram-card--bridge.flowgram-card--subgraphOutput').first()).toBeVisible({ timeout: 5_000 });
  });
});
