// ADR-0014 Phase 2 Task 8：modbusRead 双引脚 DOM 烟雾测试。
//
// 范围：仅证明 Task 7 的 wiring（modbusRead 加入 useDynamicPort 族 + 端口
// 渲染加 data-port-pin-kind / data-port-pin-type attribute）在真实运行环境
// 真的 hook 上去——也就是 React 渲染时 JSX 调用了 resolvePinKind /
// resolvePinTypeKind，并把结果作为 data-port-* 属性写入了 DOM。
//
// 不在范围内：
// - Playwright 画布拖拽脆弱性已知（FlowGram SVG + portal）；跨 Kind 拒连接
//   的逻辑断言由 Vitest 守护（web/src/lib/__tests__/pin-validator.test.ts
//   的 4 个 PinKind 跨 Kind 用例 + formatRejection 文案断言）。
// - 视觉外观（Data 引脚空心冷色样式）由用户手动验证 + Phase 5 视觉打磨阶段
//   再统一收口。
// - PinKind / PinType 的具体 IPC 取值（如 latest=`data`、out=`exec`）：
//   Playwright `webServer` 启动 Tauri dev，但 Playwright 仍以 HTTP 浏览器
//   方式连 1420 端口，即 hasTauriRuntime() 为 false——此模式下
//   `describe_node_pins` IPC 不可达，pin-schema-cache 一律走 fallback
//   `Any/Any/exec`。具体取值由
//     - Vitest：web/src/lib/__tests__/pin-schema-cache.test.ts 全覆盖
//       resolvePinKind / resolvePinTypeKind 的 cache hit / miss / 多端口
//       场景；
//     - Rust 集成测试：crates/nodes-io 的 modbusRead 单测验证 Rust
//       端 PinDefinition 形态（Phase 2 Task 4）；
//   两层守住。E2E 这里仅证明 attribute 真的写到了 DOM 上、port id 正确。
//
// 实现选择：Path A（UI flow）。
//   `board-create` 创建的新工程默认模板（projects.ts:717 buildStarterWorkflow）
//   只含 timer + debugConsole——没有 modbusRead。所以点击左侧 FlowgramNodeAddPanel
//   "Modbus Read" 卡片的双击路径（onDoubleClick → handleInsertNode →
//   createWorkflowNodeByType）插入一个 modbusRead 节点。这条路径不依赖
//   FlowGram 的画布拖拽，比 onMouseDown 的 startDragCard 在 Playwright 下
//   稳定得多。
//
//   面板 `is-hover-reveal` 类只有鼠标进入左侧 200px 触发区时才挂载（见
//   FlowgramNodeAddPanel.tsx:55-79）；面板未 reveal 时被 FlowGram 画布层
//   遮盖，物理坐标的 dblclick 会落到画布上。改用 `dispatchEvent('dblclick')`
//   直接对按钮派发合成事件——React onDoubleClick 通过事件委托捕获即可，
//   不需要按钮处于最上层。

import { expect, test } from '@playwright/test';

import { navigateToSection, waitForAppReady } from './helpers';

test.describe('ADR-0014 Phase 2: modbusRead 双引脚 DOM wiring', () => {
  test('modbusRead 节点在画布上同时显示 out (Exec) 与 latest (Data) 端口', async ({
    page,
  }) => {
    await waitForAppReady(page);
    await navigateToSection(page, 'boards');

    await page.locator('[data-testid="board-create"]').click();
    await expect(page.locator('[data-testid="deploy-button"]')).toBeVisible({
      timeout: 10_000,
    });

    // 通过节点添加面板插入 modbusRead——FlowgramNodeAddPanel 渲染按钮 class
    // `flowgram-add-card--modbusRead`（FlowgramNodeAddPanel.tsx:138）。
    // 该按钮提供 onDoubleClick → onInsertSeed(seed, 'standalone' | 'downstream')
    // 的非拖拽插入路径，绕开 FlowGram 画布拖拽。
    const modbusAddCard = page.locator('.flowgram-add-card--modbusRead').first();
    await expect(modbusAddCard).toBeVisible({ timeout: 10_000 });
    await modbusAddCard.dispatchEvent('dblclick');

    // 节点插入后 FlowgramCanvas 渲染出 .flowgram-card--modbusRead 卡片。
    const modbusCard = page.locator('.flowgram-card--modbusRead').first();
    await expect(modbusCard).toBeVisible({ timeout: 10_000 });

    // Task 7 把 modbusRead 加入 useDynamicPort 族——分支 row 由
    // `getLogicNodeBranchDefinitions('modbusRead')` 输出 `out` + `latest` 两项。
    // 两个输出 row 都渲染，并且都带上 data-port-id / data-port-pin-kind /
    // data-port-pin-type 属性，证明 Task 7 的 JSX wiring（FlowgramCanvas.tsx:725-740）
    // 真的连到了 React 渲染树。
    const outPort = modbusCard.locator('[data-port-id="out"]');
    const latestPort = modbusCard.locator('[data-port-id="latest"]');

    await expect(outPort).toBeVisible();
    await expect(latestPort).toBeVisible();

    // 两个端口都必须有 data-port-pin-kind 属性——具体取值依赖 IPC，浏览器
    // 预览模式下走 fallback `'exec'`，Tauri 模式才能拿到 `'data'`（latest）。
    // 故仅断言属性存在 + 取值在合法枚举集合内。
    for (const port of [outPort, latestPort]) {
      const kind = await port.getAttribute('data-port-pin-kind');
      expect(kind).not.toBeNull();
      expect(['exec', 'data']).toContain(kind);

      const pinType = await port.getAttribute('data-port-pin-type');
      expect(pinType).not.toBeNull();
      // PinType.kind 枚举：any / json / bool / number / string / bytes / array / custom
      expect([
        'any',
        'json',
        'bool',
        'number',
        'string',
        'bytes',
        'array',
        'custom',
      ]).toContain(pinType);
    }
  });
});
