// Pure 计算节点插入烟雾测试。
//
// 纯计算节点（c2f / minutesSince / lookup）使用 pure-form 渲染模式：
// CSS class `flowgram-card--pure-form` + data 属性 `data-pure-form="true"`。

import { expect, test } from '@playwright/test';

import { createBoardAndOpenCanvas, insertNodeByDblClick } from './helpers';

const PURE_NODES = ['c2f', 'minutesSince', 'lookup'] as const;

for (const nodeType of PURE_NODES) {
  test.describe(`Pure 节点: ${nodeType}`, () => {
    test(`${nodeType}: 渲染 pure-form 卡片`, async ({ page }) => {
      await createBoardAndOpenCanvas(page);
      const card = await insertNodeByDblClick(page, nodeType);

      // 纯计算节点带 pure-form 标记
      await expect(card).toHaveClass(/flowgram-card--pure-form/);
      await expect(card.locator('.flowgram-card__body')).toHaveAttribute('data-pure-form', 'true');
    });
  });
}
