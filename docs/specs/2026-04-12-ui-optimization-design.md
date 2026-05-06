# UI 优化修复设计文档

## 日期：2026-04-12

## 问题概述

Nazh 前端存在多项 UI 设计缺陷，主要影响：
1. Light Mode 用户体验（Runtime Log 完全不可用）
2. 表单可用性（缺少 label、loading 状态）
3. 响应式布局（侧边栏固定宽度）
4. 键盘无障碍（Toolbar 无焦点环）

## 修复方案

### P0: Runtime Log 硬编码颜色修复

**问题**：`styles.css` 第 6181-6283 行中，日志面板所有颜色硬编码为深色值，Light Mode 下完全不可见。

**修复**：替换为 CSS 变量，适配 Light/Dark 双主题。

| 元素 | 之前 | 之后 |
|------|------|------|
| `.runtime-log` | `background: #101114` | `background: var(--surface-elevated)` |
| `.runtime-log` | `color: rgba(255,255,255,0.88)` | `color: var(--text)` |
| `.is-success` | `color: #d9f2e6` | `color: var(--success-ink)` |
| `.is-error` | `color: #ffd5da` | `color: var(--danger-ink)` |
| `.is-warn` | `color: #f4e2bf` | `color: var(--warning-ink)` |
| `.runtime-log__time` | `color: rgba(255,255,255,0.48)` | `color: var(--muted)` |
| Dashboard gauge `::after` | `background: rgba(255,255,255,0.9)` | `background: var(--surface-elevated)` |

### P1-1: ConnectionStudio 表单 Label + Loading 状态

**问题**：
- 表单 `<input>` 仅使用 `placeholder` 作为视觉提示，屏幕阅读器无法识别
- Loading 状态仅显示文字，与空状态视觉无法区分

**修复**：
1. 在 `<label>` 内添加 `<span>` 作为可见标签，`input` 添加 `aria-label`
2. Loading 状态增加 spinner 图标

```tsx
<label>
  <span>连接 ID</span>
  <input aria-label="连接 ID" value={...} onChange={...} />
</label>

// Loading 状态
{isLoading ? (
  <div className="connection-empty">
    <SpinnerIcon />
    <p>正在加载连接资源…</p>
  </div>
) : ...}
```

### P1-2: PayloadPanel JSON 验证反馈

**问题**：Payload JSON 无实时验证，用户须部署后才能发现格式错误。

**修复**：
1. 增加 `isValidJson` state
2. `onChange` 时尝试 `JSON.parse`
3. 无效时 input 边框变红

```tsx
const [isValidJson, setIsValidJson] = useState(true);

onChange={(value) => {
  try { JSON.parse(value); setIsValidJson(true); }
  catch { setIsValidJson(false); }
  onPayloadTextChange(value);
}}

// className: isValidJson ? '' : 'is-invalid'
```

### P2-1: 侧边栏响应式宽度

**问题**：侧边栏固定 216px，在 1366x768 等常见笔记本分辨率下画布空间不足。

**修复**：
- 改为 `minmax(180px, 216px)`，允许收缩至 180px
- 断点（1280px/1080px）调整为 `minmax(160px, 196px)`

### P2-2: Toolbar 键盘焦点环

**问题**：`FlowgramToolbar` 按钮无可见焦点状态，键盘用户无法识别当前焦点。

**修复**：添加 CSS focus-visible 样式

```css
.flowgram-toolbar button:focus-visible {
  outline: 2px solid var(--accent-border);
  outline-offset: 2px;
}
```

## 执行顺序

1. P0: Runtime Log CSS 变量化
2. P1-1: ConnectionStudio label + spinner
3. P1-2: PayloadPanel JSON 验证
4. P2-1: Sidebar 响应式
5. P2-2: Toolbar 焦点环

## 验证方式

- Light/Dark 主题切换确认日志面板正常显示
- 表单字段屏幕阅读器朗读正常
- Payload 输入无效 JSON 时边框变红
- 侧边栏宽度随窗口收缩
- Tab 键导航时 Toolbar 焦点环可见
