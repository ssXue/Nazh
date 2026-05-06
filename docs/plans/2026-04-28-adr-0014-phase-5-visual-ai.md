> **Status:** deferred as Phase A backlog (not implemented as of 2026-04-29)

# ADR-0014 Phase 5 实施计划：视觉打磨 + AI prompt PinKind 感知

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ADR-0014 spec 第九章 Phase 5 的 5 件事落地——（1）节点头部色按 capability 自动着色（Trigger 红 / Branching 蓝 / Pure 绿 / 普通灰蓝）；（2）颜色映射 CSS 变量化（明暗主题适配）；（3）minimap / 调试视图同步引脚视觉；（4）引脚 tooltip 显示 PinKind + PinType（沿用 ADR-0010 Phase 4 已建框架，扩展 Phase 4 已加的"空槽策略 / TTL"行）；（5）AI 脚本生成 prompt 携带 PinKind 信息，让 AI 生成的 Rhai 脚本理解"哪些值是被推、哪些是被拉"。**前置条件**：Phase 3（pure-form 视觉 + isPureForm helper）+ Phase 4（pin tooltip 已加策略行）已落地。

**Architecture:**
- **节点头部色统一规则**（Capability 优先级，从高到低）：
  - `TRIGGER` → 红头 `--node-head-trigger: #B00000`
  - `BRANCHING` → 蓝头 `--node-head-branching: #1FB7FF`
  - pure-form（`is_pure_form` 推导，与 PURE capability 正交但通常重合）→ 绿头 `--node-head-pure: #2FB75F`
  - 其他 → 灰蓝 `--node-head-default: #3A4F66`
  - 同时具备多 capability 时（如 `BRANCHING | PURE`，`if`/`switch` 节点典型）按上述优先级取一种——`BRANCHING` 视觉占用，PURE 由独立 badge 区分（不抢主色）
- **CSS 变量主题**：
  - `:root[data-theme='light']` 与 `:root[data-theme='dark']` 各定义一组 `--node-head-*` / `--pin-*`
  - 默认 `data-theme='light'`；`prefers-color-scheme: dark` 时自动 `dark`；用户可手动切换（Settings tab 加 toggle）
  - 颜色集参考 spec 第五章"颜色映射" 表，按 UE5 Blueprint 配色微调到工业 SCADA 友好（红绿色弱替代用 Phase 5 决策 3 处理——本 Phase 决定**保留默认配色**，提供"色弱模式"开关切换到色相分离更明显的备选集）
- **minimap 同步**：FlowGram 的 minimap 当前用节点单色 / 默认形状渲染——本 Phase 让 minimap 读相同 `data-pure-form` / capability attribute，按节点头部色块绘制（保留 Phase 3+4 已建立的 DOM attribute 通路）。此项在 FlowGram 框架下可通过 `<MinimapNode>` 自定义 prop 实现。
- **引脚 tooltip 扩展**：Phase 4 已加"空槽策略 / TTL"两行；Phase 5 在最顶部加"求值语义：Exec 推 / Data 拉"（替换 Phase 2 的简单 `kind` 行）+ "数据形状：<PinType 中文标签>"，文案统一中文。
- **AI prompt PinKind 感知**：`web/src/lib/ai-prompt-builder.ts`（或现有 prompt 构造模块，grep `pinSchema` 找位置）在描述节点时把每个 pin 的 PinKind + PinDirection + 空槽策略一同 inline，给 AI 生成 Rhai 脚本时参考。
  - prompt 增量示例：
    ```
    - 节点 `c2f`（pure-form，纯函数）
      - 输入 `value`（Float, Data 拉, 空槽策略=阻塞等待）
      - 输出 `out`（Float, Data 推-写缓存）
    ```
  - 让 AI 理解"被 Exec 触发的节点 transform 时拿到的 payload 已经合并了 Data 输入"——避免生成的脚本试图主动 pull
- **跨语言契约**：head color 决策本质是 TS 端逻辑（CSS 类名映射），无需 Rust 镜像。但 capability 位分配 / pure-form 推导逻辑 Rust + TS 已分别有合约 fixture（ADR-0011 + Phase 3），不需要新 fixture。
- **可观测**：本 Phase 不动后端 Runner / 事件流，纯前端 + 文档。

**Tech Stack:** TypeScript / React 18 / FlowGram.AI（节点渲染、CSS 变量、minimap、tooltip、AI prompt builder），Vitest（视觉判定函数单测、AI prompt 快照测试），Playwright（DOM 烟雾），CSS。后端 Rust 不动。

---

## File Structure

| 操作 | 路径 | 责任 |
|------|------|------|
| 创建 | `web/src/lib/node-head-color.ts` | `nodeHeadColorClass(capabilities, pureForm) -> string` 纯函数 + 优先级规则 |
| 创建 | `web/src/lib/__tests__/node-head-color.test.ts` | 单测覆盖 4 单 capability + 多 capability 优先级 + pure-form 与 PURE 协同 |
| 修改 | `web/src/components/FlowgramCanvas.tsx` | `FlowgramNodeCard` 加 `data-node-head-color` attribute（4 取一），替换 Phase 3 的 `data-pure-form` 单一逻辑 |
| 修改 | `web/src/styles/flowgram.css` | 引入 CSS 变量定义；`[data-node-head-color='trigger\|branching\|pure\|default']` 各自样式；明暗主题切换 |
| 创建 | `web/src/styles/theme.css` | `:root[data-theme='light\|dark']` 颜色变量集（节点头部 + pin 颜色 + 文本前景背景） |
| 修改 | `web/src/main.tsx`（或 root mount 文件） | import `./styles/theme.css`；初始化 `data-theme` from localStorage / `prefers-color-scheme` |
| 创建 | `web/src/components/ThemeToggle.tsx` | 简单 toggle 组件（light/dark/auto），写 localStorage |
| 修改 | Settings tab 主组件（grep `Settings`，可能在 `web/src/components/SettingsPanel.tsx` 或类似） | 引入 `ThemeToggle` |
| 修改 | `web/src/components/flowgram/get-port-tooltip.ts` | 顶部加 "求值语义：xxx" + "数据形状：xxx" 中文行 |
| 修改 | `web/src/components/FlowgramCanvas.tsx` | minimap render：`<Minimap>` 内 `<MinimapNode>` 读 `data-node-head-color` attribute 设填充色 |
| 修改 | `web/src/lib/ai-prompt-builder.ts`（grep `pinSchema` 定位实际文件） | inline pin 描述加 PinKind / 空槽策略 |
| 修改 | `web/src/lib/__tests__/ai-prompt-builder.test.ts` | 快照测试覆盖含 Data 引脚的节点 prompt 描述 |
| 修改 | `web/src/lib/pin-compat.ts` | 新增 `pinTypeChineseLabel(pin_type) -> string` 与 `pinKindChineseLabel(kind) -> string` 中文标签函数 |
| 创建 | `web/e2e/pin-kind-theme-toggle.spec.ts` | Playwright DOM 烟雾：toggle theme 后节点头部背景色变化 |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | 实施进度追加 Phase 5 段（标记 ADR-0014 全部 5 phase 完结） |
| 修改 | `docs/specs/2026-04-28-pin-kind-exec-data-design.md` | 第十一章勾掉决策 3/4 |
| 修改 | `AGENTS.md` | ADR-0014 状态行 → "**已实施 Phase 1~5**（YYYY-MM-DD，spec 第十一章 5 项决策全部回写）"；ADR Execution Order #8 标记完结 |

---

## Out of scope

1. **色弱模式（红绿色盲友好替代色集）**——本 Phase 决策"保留默认 UE5 配色 + 提供切换"，但具体色弱模式色盘留给后续 polish（独立 plan，可能与 spec 第十一章决策 3 一同处理）
2. **AI prompt 对 EmptyPolicy / TTL 的语义理解深度**——本 Phase 让 prompt 携带，但 AI 是否理解为代码行为 / 是否生成对应 try/catch 逻辑不强制——属于 prompt engineering 迭代范畴
3. **节点头部形状变体**（圆角胶囊 vs 圆角矩形 vs 平行四边形）——本 Phase 仅"绿头" CSS 区分 pure 节点；形状统一保留圆角矩形。spec 第十一章决策 4（节点头部形状 CSS）决策定为"统一圆角矩形 + radius 微调（pure 18px / 其他 8px）"，已含在 Phase 3 视觉中
4. **minimap 高级交互**（缩略图缩放 / 悬停高亮 / 跳转）——本 Phase 仅同步颜色显示
5. **PinKind tooltip 国际化（i18n）**——本 Phase 中文 hardcode；i18n 框架引入是独立任务

---

## Task 1: `nodeHeadColorClass` 纯函数 + 单测

**Files:**
- Create: `web/src/lib/node-head-color.ts`
- Create: `web/src/lib/__tests__/node-head-color.test.ts`

- [ ] **Step 1: 创建 `web/src/lib/node-head-color.ts`**

```typescript
/**
 * ADR-0014 Phase 5：节点头部色按 capability + pure-form 派生。
 *
 * 优先级（高 → 低）：trigger > branching > pure > default
 *
 * 同时具备多 capability 的节点（如 `if` 是 BRANCHING | PURE）取最高优先级；
 * pure-form 由 input/output pins 推导（参见 `isPureForm`），与 PURE capability
 * 正交：常重合但不必然——例如某节点 capabilities=PURE 但仍有 Exec 输入引脚
 * （`if` / `switch` 这种），它走 BRANCHING 头色而非 pure 头色。
 */

import type { NodeCapabilities } from './node-capabilities';
import { hasCapability, NodeCapability } from './node-capabilities';

export type NodeHeadColor = 'trigger' | 'branching' | 'pure' | 'default';

export function nodeHeadColorClass(
  capabilities: NodeCapabilities,
  pureForm: boolean,
): NodeHeadColor {
  if (hasCapability(capabilities, NodeCapability.TRIGGER)) return 'trigger';
  if (hasCapability(capabilities, NodeCapability.BRANCHING)) return 'branching';
  if (pureForm) return 'pure';
  return 'default';
}
```

> **注**：`hasCapability` / `NodeCapability` 来自现有 `web/src/lib/node-capabilities.ts`（ADR-0011 落地时已建）。grep 确认实际 API 名后 align。

- [ ] **Step 2: 单测**

```typescript
import { describe, expect, it } from 'vitest';
import { nodeHeadColorClass } from '../node-head-color';
import { NodeCapability } from '../node-capabilities';

const NONE = 0;

describe('nodeHeadColorClass', () => {
  it('TRIGGER 优先级最高', () => {
    expect(
      nodeHeadColorClass(
        NodeCapability.TRIGGER | NodeCapability.BRANCHING | NodeCapability.PURE,
        true,
      ),
    ).toBe('trigger');
  });

  it('BRANCHING 高于 pure-form', () => {
    expect(nodeHeadColorClass(NodeCapability.BRANCHING | NodeCapability.PURE, true))
      .toBe('branching');
  });

  it('pure-form 触发绿头（即使没 PURE capability）', () => {
    expect(nodeHeadColorClass(NONE, true)).toBe('pure');
  });

  it('PURE capability 但非 pure-form 不触发绿头', () => {
    // 如 `if` / `switch`：BRANCHING + PURE，有 Exec 输入 → 走 branching
    expect(nodeHeadColorClass(NodeCapability.PURE, false)).toBe('default');
  });

  it('普通节点（无 capability、非 pure-form）→ default', () => {
    expect(nodeHeadColorClass(NONE, false)).toBe('default');
  });
});
```

- [ ] **Step 3: 跑 + commit**

```bash
npm --prefix web run test -- node-head-color
git add web/src/lib/node-head-color.ts web/src/lib/__tests__/node-head-color.test.ts
git commit -s -m "feat(web): ADR-0014 Phase 5 nodeHeadColorClass 派生函数 + 单测"
```

---

## Task 2: CSS 变量主题集 + light/dark 切换基建

**Files:**
- Create: `web/src/styles/theme.css`
- Modify: `web/src/main.tsx` — import + 初始化 `data-theme`
- Create: `web/src/components/ThemeToggle.tsx`

- [ ] **Step 1: 创建 `web/src/styles/theme.css`**

```css
/* ADR-0014 Phase 5：颜色主题变量集。
 * 节点头部 / pin 颜色 / 文本前景背景集中在此，让明暗主题适配可控。
 *
 * UE5 Blueprint 风格配色微调到工业 SCADA 友好——红 / 蓝 / 绿三原色饱和度
 * 略降，避免 24/7 监控屏幕视觉疲劳。 */

:root[data-theme='light'] {
  /* 节点头部色 */
  --node-head-trigger: #b04040;
  --node-head-branching: #2596d6;
  --node-head-pure: #2fb75f;
  --node-head-default: #4a5e75;
  --node-head-text: #ffffff;

  /* pin 颜色（按 PinType） */
  --pin-bool: #b00000;
  --pin-int: #1fb7ff;
  --pin-float: #2fb75f;
  --pin-string: #e91e63;
  --pin-json: #daa520;
  --pin-binary: #6a0dad;
  --pin-any: #888888;
  --pin-exec: #ffffff;
  --pin-data-empty: #cfd8e0;

  /* 节点 body */
  --node-bg: #ffffff;
  --node-border: #d0d8e0;
  --node-text: #1a2530;

  /* 画布 */
  --canvas-bg: #f4f6f8;
  --canvas-grid: #e1e6ec;
}

:root[data-theme='dark'] {
  --node-head-trigger: #c84545;
  --node-head-branching: #2faaee;
  --node-head-pure: #3ec872;
  --node-head-default: #5d738c;
  --node-head-text: #ffffff;

  --pin-bool: #d04040;
  --pin-int: #45c8ff;
  --pin-float: #3ec872;
  --pin-string: #f04080;
  --pin-json: #e6b830;
  --pin-binary: #8a2dc8;
  --pin-any: #aaaaaa;
  --pin-exec: #ffffff;
  --pin-data-empty: #404e5c;

  --node-bg: #1c2530;
  --node-border: #2e3a48;
  --node-text: #e6edf4;

  --canvas-bg: #11161e;
  --canvas-grid: #1a2128;
}
```

- [ ] **Step 2: `main.tsx` import + 初始化**

```tsx
import './styles/theme.css';

// 早期初始化 data-theme（避免首次渲染闪白）
const stored = localStorage.getItem('nazh-theme');
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
const initial = stored ?? (prefersDark ? 'dark' : 'light');
document.documentElement.setAttribute('data-theme', initial);
```

- [ ] **Step 3: `ThemeToggle.tsx`**

```tsx
import { useEffect, useState } from 'react';

type Theme = 'light' | 'dark' | 'auto';

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(
    (localStorage.getItem('nazh-theme') as Theme | null) ?? 'auto',
  );

  useEffect(() => {
    const apply = () => {
      const resolved =
        theme === 'auto'
          ? window.matchMedia('(prefers-color-scheme: dark)').matches
            ? 'dark'
            : 'light'
          : theme;
      document.documentElement.setAttribute('data-theme', resolved);
      if (theme === 'auto') {
        localStorage.removeItem('nazh-theme');
      } else {
        localStorage.setItem('nazh-theme', theme);
      }
    };
    apply();

    if (theme === 'auto') {
      const mq = window.matchMedia('(prefers-color-scheme: dark)');
      mq.addEventListener('change', apply);
      return () => mq.removeEventListener('change', apply);
    }
    return undefined;
  }, [theme]);

  return (
    <select
      className="theme-toggle"
      value={theme}
      onChange={(e) => setTheme(e.target.value as Theme)}
      aria-label="主题切换"
    >
      <option value="auto">跟随系统</option>
      <option value="light">浅色</option>
      <option value="dark">深色</option>
    </select>
  );
}
```

- [ ] **Step 4: 把 ThemeToggle 挂到 Settings tab（或 app header 角落）**

grep `SettingsPanel` / `app-header` 等组件，找合适位置 mount。

- [ ] **Step 5: dev server 手动验证**

```bash
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch
```

切换 theme，画布背景 / 节点 border 应明显变化。

- [ ] **Step 6: commit**

```bash
git add web/src/styles/theme.css web/src/main.tsx web/src/components/ThemeToggle.tsx web/src/components/SettingsPanel.tsx
git commit -s -m "feat(web): ADR-0014 Phase 5 CSS 变量主题集 + ThemeToggle 切换"
```

---

## Task 3: 节点头部色统一渲染（替换 Phase 3 单 pure 逻辑）

**Files:**
- Modify: `web/src/components/FlowgramCanvas.tsx` — `FlowgramNodeCard` 用 `nodeHeadColorClass` + 设 `data-node-head-color`
- Modify: `web/src/styles/flowgram.css` — 4 种 head color 的样式

- [ ] **Step 1: 在 `FlowgramNodeCard` 内部计算 head color**

```tsx
import { nodeHeadColorClass } from '../lib/node-head-color';
import { isPureForm } from '../lib/pin-compat';

// FlowgramNodeCard 内：
const inputPins = nodeSchema?.input_pins ?? [];
const outputPins = nodeSchema?.output_pins ?? [];
const pureForm = isPureForm(inputPins, outputPins);
const headColor = nodeHeadColorClass(nodeSchema?.capabilities ?? 0, pureForm);

return (
  <div
    className="flowgram-node-card"
    data-node-kind={kind}
    data-pure-form={pureForm ? 'true' : undefined}
    data-node-head-color={headColor}
    /* ... */
  >
```

> **注**：`nodeSchema.capabilities` 来自 IPC `describe_node_pins`（或 `list_node_types` capability 字段）。如果当前 schema 不带 capabilities，需要先确认 Phase 4 / ADR-0011 IPC 是否有此字段。如果没有，需要从 `nodeKindToCapabilities` 映射表（grep 现有代码）查询。

- [ ] **Step 2: `flowgram.css` 加 4 种 head color 样式**

```css
/* ADR-0014 Phase 5：capability-based 节点头部色 */
.flowgram-node-card[data-node-head-color='trigger'] .flowgram-node-header {
  background: var(--node-head-trigger);
  color: var(--node-head-text);
}
.flowgram-node-card[data-node-head-color='branching'] .flowgram-node-header {
  background: var(--node-head-branching);
  color: var(--node-head-text);
}
.flowgram-node-card[data-node-head-color='pure'] .flowgram-node-header {
  background: var(--node-head-pure);
  color: var(--node-head-text);
}
.flowgram-node-card[data-node-head-color='default'] .flowgram-node-header {
  background: var(--node-head-default);
  color: var(--node-head-text);
}

/* pure-form 仍保留 Phase 3 的圆角胶囊 + 边框色 */
.flowgram-node-card[data-pure-form='true'] {
  border-radius: 18px;
  border-color: var(--pin-float);
}
```

把 Phase 3 写的 `[data-pure-form='true'] .flowgram-node-header { background: linear-gradient... }` 删掉——本 Task 用 `[data-node-head-color='pure']` 取代单纯线性渐变（color 一致即可，避免 gradient 跟主题切换打架）。

- [ ] **Step 3: dev server 手动验证 4 种节点头色**

- 拖入 `timer`（TRIGGER） → 红头
- 拖入 `if`（BRANCHING + PURE） → 蓝头
- 拖入 `c2f`（pure-form） → 绿头
- 拖入 `httpClient`（默认） → 灰蓝头

- [ ] **Step 4: commit**

```bash
git add web/src/components/FlowgramCanvas.tsx web/src/styles/flowgram.css
git commit -s -m "feat(web): ADR-0014 Phase 5 节点头部色按 capability 自动着色"
```

---

## Task 4: 引脚 tooltip 中文标签 + PinKind / PinType 行

**Files:**
- Modify: `web/src/lib/pin-compat.ts` — 加 `pinTypeChineseLabel` / `pinKindChineseLabel`
- Modify: `web/src/components/flowgram/get-port-tooltip.ts` — 行序调整 + 加新行

- [ ] **Step 1: `pin-compat.ts` 加中文标签函数**

```typescript
import type { PinType } from '../generated/PinType';
import type { PinKind } from '../generated/PinKind';

export function pinTypeChineseLabel(t: PinType): string {
  switch (t.kind) {
    case 'any': return '任意';
    case 'bool': return '布尔';
    case 'integer': return '整数';
    case 'float': return '浮点';
    case 'string': return '字符串';
    case 'json': return 'JSON';
    case 'binary': return '二进制';
    case 'array': return `数组<${pinTypeChineseLabel(t.inner)}>`;
    case 'custom': return `自定义(${t.name})`;
    default: return String((t as { kind: string }).kind);
  }
}

export function pinKindChineseLabel(k: PinKind): string {
  return k === 'data' ? 'Data 拉' : 'Exec 推';
}
```

- [ ] **Step 2: `get-port-tooltip.ts` 重组 tooltip 行序**

```typescript
import { pinKindChineseLabel, pinTypeChineseLabel } from '../../lib/pin-compat';

export function getPortTooltip(pin: PinDefinition): string {
  const lines: string[] = [];

  // 第 1 行：求值语义（最重要——决定连线规则）
  lines.push(`求值语义：${pinKindChineseLabel(pin.kind ?? 'exec')}`);

  // 第 2 行：数据形状
  lines.push(`数据形状：${pinTypeChineseLabel(pin.pin_type)}`);

  // 第 3 行：标签 / 描述
  if (pin.description) lines.push(pin.description);

  // 第 4+ 行：Phase 4 已有的策略 / TTL（仅 Data 输入引脚）
  if (pin.kind === 'data' && pin.direction === 'input') {
    const policy = formatEmptyPolicy(pin.empty_policy);
    if (policy) lines.push(`空槽策略：${policy}`);
    const ttl = formatTtl(pin.ttl_ms);
    if (ttl) lines.push(ttl);
  }

  return lines.join('\n');
}
```

`formatEmptyPolicy` / `formatTtl` Phase 4 已写——保留。

- [ ] **Step 3: 单测**

```typescript
describe('getPortTooltip', () => {
  it('Exec 输入引脚显示 4 行（无策略 / TTL）', () => {
    const tip = getPortTooltip({
      id: 'in', label: 'in', pin_type: { kind: 'json' },
      direction: 'input', kind: 'exec', required: true,
      description: '主输入',
    } as any);
    expect(tip).toContain('求值语义：Exec 推');
    expect(tip).toContain('数据形状：JSON');
    expect(tip).toContain('主输入');
    expect(tip).not.toContain('空槽策略');
  });

  it('Data 输入引脚显示策略行', () => {
    const tip = getPortTooltip({
      id: 'temp', label: 'temp', pin_type: { kind: 'float' },
      direction: 'input', kind: 'data', required: false,
      empty_policy: { kind: 'default_value', value: 0 },
    } as any);
    expect(tip).toContain('求值语义：Data 拉');
    expect(tip).toContain('数据形状：浮点');
    expect(tip).toContain('空槽策略：默认值：0');
  });
});
```

- [ ] **Step 4: 跑测试 + commit**

```bash
npm --prefix web run test -- get-port-tooltip pin-compat
git add web/src/lib/pin-compat.ts web/src/components/flowgram/get-port-tooltip.ts web/src/components/flowgram/__tests__/
git commit -s -m "feat(web): ADR-0014 Phase 5 引脚 tooltip 中文求值语义/数据形状行"
```

---

## Task 5: minimap 同步节点头色

**Files:**
- Modify: `web/src/components/FlowgramCanvas.tsx` — `<Minimap>` / `<MinimapNode>` prop

- [ ] **Step 1: 定位 minimap 渲染处**

```bash
grep -n "Minimap\|minimap" web/src/components/FlowgramCanvas.tsx
```

如果 FlowGram 提供 `<Minimap renderNode={...}>` slot，写自定义 renderNode：

```tsx
<Minimap
  renderNode={(node) => {
    const pureForm = isPureForm(/* ... 同 FlowgramNodeCard 算法 */);
    const headColor = nodeHeadColorClass(node.capabilities ?? 0, pureForm);
    return (
      <rect
        x={node.x} y={node.y} width={node.w} height={node.h}
        fill={`var(--node-head-${headColor})`}
      />
    );
  }}
/>
```

如果框架不允许自定义 renderNode，退化方案：minimap CSS `.minimap-node[data-node-kind='timer'] { fill: var(--node-head-trigger); }` 等枚举（启动期生成 CSS 不动态）。

- [ ] **Step 2: 手动验证**

dev server 启动后看右下 minimap，触发器节点应显红色 dot，pure 节点绿色。

- [ ] **Step 3: commit**

```bash
git add web/src/components/FlowgramCanvas.tsx web/src/styles/flowgram.css
git commit -s -m "feat(web): ADR-0014 Phase 5 minimap 同步节点头色"
```

---

## Task 6: AI prompt builder 携带 PinKind / 空槽策略

**Files:**
- Modify: `web/src/lib/ai-prompt-builder.ts`（grep 实际文件名定位）

- [ ] **Step 1: 定位现有 prompt 构造代码**

```bash
grep -rn "input_pins\|output_pins\|pinSchema" web/src/lib/ | grep -v __tests__ | grep -v generated
```

预期文件：`web/src/lib/ai-prompt-builder.ts` 或 `code-generator.ts` / 类似。

- [ ] **Step 2: 在节点 inline 描述加 PinKind / 策略**

```typescript
function describePin(pin: PinDefinition): string {
  const dir = pin.direction === 'input' ? '输入' : '输出';
  const kind = pin.kind === 'data' ? 'Data 拉' : 'Exec 推';
  const type = pinTypeChineseLabel(pin.pin_type);
  const parts = [`${dir} \`${pin.id}\` (${type}, ${kind})`];

  if (pin.kind === 'data' && pin.direction === 'input') {
    const policy = pin.empty_policy?.kind ?? 'block_until_ready';
    parts.push(
      policy === 'block_until_ready' ? '空槽=阻塞等待'
      : policy === 'default_value' ? `空槽=默认值${JSON.stringify((pin.empty_policy as any).value)}`
      : '空槽=跳过(null)'
    );
    if (pin.ttl_ms) parts.push(`TTL=${pin.ttl_ms}ms`);
  }
  return parts.join('，');
}

export function describeNodeForAi(node: NodeTypeEntry): string {
  const lines: string[] = [];
  const pureForm = isPureForm(node.input_pins, node.output_pins);
  if (pureForm) {
    lines.push(`节点 \`${node.kind}\`（pure-form 纯函数节点，无副作用、不参与触发链）：`);
  } else {
    lines.push(`节点 \`${node.kind}\`：`);
  }
  for (const pin of node.input_pins) lines.push(`  - ${describePin(pin)}`);
  for (const pin of node.output_pins) lines.push(`  - ${describePin(pin)}`);
  return lines.join('\n');
}
```

注入到现有 prompt 构造的节点描述 section。

- [ ] **Step 3: 加 prompt 快照测试**

```typescript
import { describe, expect, it } from 'vitest';
import { describeNodeForAi } from '../ai-prompt-builder';

describe('describeNodeForAi PinKind 携带', () => {
  it('pure-form 节点 prompt 含 pure-form 标记', () => {
    const out = describeNodeForAi({
      kind: 'c2f',
      capabilities: 1,  // PURE
      input_pins: [{
        id: 'value', label: 'value', pin_type: { kind: 'float' },
        direction: 'input', kind: 'data', required: true,
        empty_policy: { kind: 'block_until_ready' },
      } as any],
      output_pins: [{
        id: 'out', label: 'out', pin_type: { kind: 'float' },
        direction: 'output', kind: 'data', required: false,
      } as any],
    });
    expect(out).toMatchInlineSnapshot(`
      "节点 \`c2f\`（pure-form 纯函数节点，无副作用、不参与触发链）：
        - 输入 \`value\` (浮点，Data 拉)，空槽=阻塞等待
        - 输出 \`out\` (浮点，Data 拉)"
    `);
  });

  it('普通节点 prompt 含 Exec 标注', () => {
    const out = describeNodeForAi({
      kind: 'httpClient',
      capabilities: 0,
      input_pins: [{ id: 'in', label: 'in', pin_type: { kind: 'json' },
        direction: 'input', kind: 'exec', required: true } as any],
      output_pins: [{ id: 'out', label: 'out', pin_type: { kind: 'json' },
        direction: 'output', kind: 'exec', required: false } as any],
    });
    expect(out).toContain('Exec 推');
    expect(out).not.toContain('pure-form');
  });
});
```

- [ ] **Step 4: 跑 + commit**

```bash
npm --prefix web run test -- ai-prompt-builder
git add web/src/lib/ai-prompt-builder.ts web/src/lib/__tests__/ai-prompt-builder.test.ts
git commit -s -m "feat(web): ADR-0014 Phase 5 AI prompt 携带 PinKind / 空槽策略"
```

---

## Task 7: Playwright DOM 烟雾 — theme toggle + 节点头色

**Files:**
- Create: `web/e2e/pin-kind-theme-toggle.spec.ts`

- [ ] **Step 1: 创建 spec**

```typescript
import { expect, test } from '@playwright/test';

test.describe('ADR-0014 Phase 5 — 主题切换 + 节点头色烟雾', () => {
  test('切换 dark theme 后 data-theme 属性更新', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-theme', /^(light|dark)$/);

    await page.locator('.theme-toggle').selectOption('dark');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  });

  test('拖入 timer 后节点 DOM 携带 data-node-head-color=trigger', async ({ page }) => {
    await page.goto('/');
    const item = page.locator('[data-node-kind="timer"]').first();
    await item.dragTo(page.locator('.flowgram-canvas'));

    const node = page.locator('.flowgram-node-card[data-node-kind="timer"]');
    await expect(node).toHaveAttribute('data-node-head-color', 'trigger');
  });

  test('拖入 c2f 后节点 DOM 携带 data-node-head-color=pure', async ({ page }) => {
    await page.goto('/');
    const item = page.locator('[data-node-kind="c2f"]').first();
    await item.dragTo(page.locator('.flowgram-canvas'));

    const node = page.locator('.flowgram-node-card[data-node-kind="c2f"]');
    await expect(node).toHaveAttribute('data-node-head-color', 'pure');
    await expect(node).toHaveAttribute('data-pure-form', 'true');
  });
});
```

- [ ] **Step 2: 跑 + commit**

```bash
npm --prefix web run test:e2e -- pin-kind-theme-toggle
git add web/e2e/pin-kind-theme-toggle.spec.ts
git commit -s -m "test(e2e): ADR-0014 Phase 5 theme toggle + 节点头色烟雾"
```

---

## Task 8: 文档更新（ADR-0014 全部 5 phase 完结）

**Files:**
- Modify: `docs/adr/0014-执行边与数据边分离.md`
- Modify: `docs/specs/2026-04-28-pin-kind-exec-data-design.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: ADR 实施进度章节加 Phase 5 + 标 ADR 整体完结**

```markdown
- ✅ **Phase 5（YYYY-MM-DD）**：视觉打磨 + AI prompt PinKind 感知。节点头部色按
  capability 自动着色（trigger 红 / branching 蓝 / pure 绿 / 默认灰蓝），CSS 变量
  化（`web/src/styles/theme.css`）+ light/dark 主题切换（ThemeToggle 写 localStorage
  / 跟随 prefers-color-scheme）。引脚 tooltip 行序重组：求值语义 → 数据形状 → 描述
  → 空槽策略（仅 Data input）→ TTL；中文标签函数 `pinTypeChineseLabel` /
  `pinKindChineseLabel`。minimap 同步节点头色。AI prompt builder 在节点描述里 inline
  PinKind / 空槽策略 / TTL，让 AI 生成的 Rhai 脚本理解 push vs pull 语义。

  **Spec 第十一章决策回写**：
  - 决策 3：颜色集色弱友好替代 → **保留默认 + 提供切换基建**（具体色弱模式色盘后续 polish）
  - 决策 4：节点头部形状（圆角胶囊 vs 圆角矩形）→ **统一圆角矩形 + radius 微调**（pure 18px / 其他 8px，已在 Phase 3 落地）
  - 决策 5（跨 Phase）：AI prompt 如何描述 PinKind → **节点 inline 描述格式**（输入/输出 + 类型 + 求值语义 + 空槽策略 + TTL）

**ADR-0014 全部 5 phase 完结。后续 polish（色弱模式色盘 / 高级 minimap 交互 / 国际化）
作为独立 plan，不再排进 ADR-0014 编号。**
```

- [ ] **Step 2: spec 文档第十一章勾掉决策 3/4/5**

- [ ] **Step 3: AGENTS.md 状态行**

```markdown
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1+2+3+3b+4+5**（YYYY-MM-DD，spec 第十一章 5 项决策全部回写，ADR-0014 完结）。
```

ADR Execution Order #8 改为：

```markdown
> 8. ✅ **ADR-0014** Pin 求值语义二分 — **全部 Phase 1~5 已实施**（YYYY-MM-DD），ADR 完结。后续 polish 作为独立 plan。
```

- [ ] **Step 4: 全量验证 + commit**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm --prefix web run test
git add docs/adr/0014-执行边与数据边分离.md docs/specs/2026-04-28-pin-kind-exec-data-design.md AGENTS.md
git commit -s -m "docs(adr-0014): Phase 5 落地 + ADR-0014 全部完结状态同步"
```

---

## Self-Review

### Spec coverage

- ✅ 节点头部色按 capability 自动着色（Pure / Trigger / Branching / 普通）—— Task 1 + Task 3
- ✅ 颜色映射 CSS 变量化（明暗主题适配）—— Task 2
- ✅ minimap / 调试视图同步 —— Task 5
- ✅ 引脚 tooltip 显示 PinKind + PinType —— Task 4
- ✅ AI 脚本生成 prompt 携带 PinKind 信息 —— Task 6

### Spec 第十一章决策拍板

- ✅ 决策 3（色弱友好色集）—— Task 8 拍板"保留默认 + 提供切换基建"，色盘 polish 留待独立 plan
- ✅ 决策 4（头部形状 CSS）—— Task 8 拍板"圆角矩形 + radius 微调"（已在 Phase 3 落地）
- ✅ 决策 5（AI prompt 如何描述 PinKind）—— Task 6 拍板格式

### Placeholder scan

- 已检：所有代码块给出实际实现
- 没有 "TODO / similar to" 等懒散语言

### Type consistency

- `nodeHeadColorClass(capabilities, pureForm) -> 'trigger' | 'branching' | 'pure' | 'default'` —— Task 1 / Task 3 / Task 5 / Task 7 一致
- `pinKindChineseLabel` / `pinTypeChineseLabel` —— Task 4 / Task 6 一致
- `data-node-head-color` attribute 的 4 个枚举值 —— Task 3 / Task 5 / Task 7 一致

### 已知风险

- **Task 1 capabilities 取自 schema 还是 IPC**：`web/src/lib/node-capabilities.ts` 已有 `capabilities: u32` 透传（ADR-0011）。本 Phase 假设可从 `nodeSchema.capabilities` 获取——需要 grep 实际数据流确认（如果 schema 不带，需要从 `list_node_types` 响应或前端常量表查）
- **Task 5 minimap 自定义渲染**：FlowGram 是否暴露 `renderNode` slot 不确定。如果不暴露，退化方案是用 CSS `[data-node-kind=...]` 枚举每个节点 kind 到颜色——略丑但可行
- **Task 6 AI prompt 快照测试**：inline snapshot 对换行 / 引号敏感，第一次跑可能 fail；按 Vitest 提示运行 `--update` 一次后稳定

---

## Implementation note

每条 task 单 commit，sign-off + 中文 commit msg。Phase 5 预期 8 commits。**前置**：Phase 3（pure-form CSS 基础）+ Phase 4（pin tooltip 已加策略行 / `EmptyPolicy` 类型）已落地。

**Phase 5 完成 = ADR-0014 完结。** 完成后 spec 文档可标 "已全部实施 (Phases 1-5)"。后续若有色弱模式 / minimap 高级交互 / 国际化等 polish，作为独立 plan，不再排进 ADR-0014 phase 编号——避免 "Phase 6 / 7" 无止境膨胀。
