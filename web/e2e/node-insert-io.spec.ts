// I/O 节点插入烟雾测试。
//
// 18 个 IO 节点的参数化插入验证。每个测试：
// 1. 创建看板 → 双击添加面板卡片 → 验证画布卡片可见
// 2. 部分节点额外验证端口属性（data-port-id / data-port-pin-kind）
//
// modbusRead / deviceSignalRead 有双端口模式（out + latest），
// 额外断言端口 PinKind 属性在合法枚举内（不依赖 IPC 真值）。

import { expect, test } from '@playwright/test';

import { createBoardAndOpenCanvas, insertNodeByDblClick } from './helpers';

// 标准卡片节点：仅验证卡片可见
const STANDARD_IO_NODES = [
  'timer',
  'serialTrigger',
  'native',
  'httpClient',
  'mqttClient',
  'barkPush',
  'sqlWriter',
  'debugConsole',
  'capabilityCall',
  'humanLoop',
  'deviceEventTrigger',
  'canRead',
  'canWrite',
  'ethercatPdoRead',
  'ethercatPdoWrite',
  'ethercatStatus',
] as const;

// 合法的 PinKind 枚举（与前端 PinKind 类型同步）
const VALID_PIN_KINDS = ['exec', 'data', 'reactive'] as string[];

for (const nodeType of STANDARD_IO_NODES) {
  test.describe(`IO 节点: ${nodeType}`, () => {
    test(`${nodeType}: 渲染卡片`, async ({ page }) => {
      await createBoardAndOpenCanvas(page);
      await insertNodeByDblClick(page, nodeType);
    });
  });
}

// modbusRead：双端口（out + latest）+ PinKind 属性
test.describe('IO 节点: modbusRead（双端口）', () => {
  test('modbusRead: 渲染卡片 + out/latest 端口属性', async ({ page }) => {
    await createBoardAndOpenCanvas(page);
    const card = await insertNodeByDblClick(page, 'modbusRead');

    const outPort = card.locator('[data-port-id="out"]');
    const latestPort = card.locator('[data-port-id="latest"]');
    await expect(outPort).toBeVisible();
    await expect(latestPort).toBeVisible();

    for (const port of [outPort, latestPort]) {
      const kind = await port.getAttribute('data-port-pin-kind');
      expect(kind).not.toBeNull();
      expect(VALID_PIN_KINDS).toContain(kind);
    }
  });
});

// deviceSignalRead：同 modbusRead 双端口模式
test.describe('IO 节点: deviceSignalRead（双端口）', () => {
  test('deviceSignalRead: 渲染卡片 + out/latest 端口属性', async ({ page }) => {
    await createBoardAndOpenCanvas(page);
    const card = await insertNodeByDblClick(page, 'deviceSignalRead');

    const outPort = card.locator('[data-port-id="out"]');
    const latestPort = card.locator('[data-port-id="latest"]');
    await expect(outPort).toBeVisible();
    await expect(latestPort).toBeVisible();

    for (const port of [outPort, latestPort]) {
      const kind = await port.getAttribute('data-port-pin-kind');
      expect(kind).not.toBeNull();
      expect(VALID_PIN_KINDS).toContain(kind);
    }
  });
});
