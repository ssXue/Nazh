# AI 脚本生成功能实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 code/rhai 节点设置面板中增加「AI 生成」按钮，允许用户通过自然语言需求调用全局 AI 配置自动生成 Rhai 脚本代码。

**Architecture:** 前端直调已有 `copilotComplete()` IPC 命令，新增独立 prompt 构建 + 生成逻辑模块，弹窗组件用于需求输入，设置面板集成按钮触发弹窗并替换脚本内容。

**Tech Stack:** React 18, TypeScript, Tauri IPC (copilotComplete), FlowGram.AI free-layout-editor API

---

## 文件结构

| 操作 | 文件 | 职责 |
|------|------|------|
| 新增 | `web/src/lib/script-generation.ts` | Prompt 构建、节点上下文提取、`generateScript` 函数 |
| 新增 | `web/src/lib/__tests__/script-generation.test.ts` | 对 script-generation 的单元测试 |
| 新增 | `web/src/components/flowgram/AiScriptGenerator.tsx` | 需求输入弹窗组件 |
| 修改 | `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx` | 集成 AI 生成按钮与弹窗 |

---

### Task 1: 节点上下文提取与 Prompt 构建

**Files:**
- Create: `web/src/lib/script-generation.ts`
- Test: `web/src/lib/__tests__/script-generation.test.ts`

- [ ] **Step 1: 创建 script-generation.ts，定义类型与上下文提取函数**

```typescript
import type { AiMessage } from '../generated/AiMessage';
import type { FlowNodeEntity } from '@flowgram.ai/free-layout-editor';

export interface NodeContextInfo {
  nodeType: string;
  label: string;
  aiDescription: string;
}

export interface NodeContext {
  current: NodeContextInfo;
  upstream: NodeContextInfo[];
  downstream: NodeContextInfo[];
}

function extractNodeInfo(node: FlowNodeEntity): NodeContextInfo {
  const extInfo = (node.getExtInfo() ?? {}) as {
    label?: string;
    nodeType?: string;
    aiDescription?: string | null;
  };
  return {
    nodeType: extInfo.nodeType ?? node.flowNodeType,
    label: extInfo.label ?? node.id,
    aiDescription: extInfo.aiDescription ?? '',
  };
}

export function getNodeContext(node: FlowNodeEntity): NodeContext {
  const inputNodes = node.lines.inputNodes as FlowNodeEntity[];
  const outputNodes = node.lines.outputNodes as FlowNodeEntity[];
  return {
    current: extractNodeInfo(node),
    upstream: inputNodes.map(extractNodeInfo),
    downstream: outputNodes.map(extractNodeInfo),
  };
}
```

- [ ] **Step 2: 添加 buildScriptGenerationPrompt 函数**

在 `web/src/lib/script-generation.ts` 中追加：

```typescript
const SYSTEM_PROMPT = `你是工业边缘计算工作流的脚本编写助手。根据用户需求生成 Rhai 脚本代码。
规则：
- 只输出可执行的 Rhai 脚本，不要输出解释文字
- 脚本可通过 ctx.payload() 获取输入数据
- 脚本可通过 ctx.set_output(value) 设置输出
- 如需调用 AI，使用 ai_complete("prompt") 函数
- 不要使用 print() 等调试语句
- 保持简洁，专注于数据处理和转换逻辑`;

export function buildScriptGenerationPrompt(
  requirement: string,
  context: NodeContext,
): AiMessage[] {
  const upstreamText =
    context.upstream.length > 0
      ? context.upstream
          .map(
            (n) =>
              `  - ${n.label}（类型: ${n.nodeType}${n.aiDescription ? `，描述: ${n.aiDescription}` : ''}）`,
          )
          .join('\n')
      : '  无';
  const downstreamText =
    context.downstream.length > 0
      ? context.downstream
          .map(
            (n) =>
              `  - ${n.label}（类型: ${n.nodeType}${n.aiDescription ? `，描述: ${n.aiDescription}` : ''}）`,
          )
          .join('\n')
      : '  无';

  const userMessage = `节点类型：${context.current.nodeType}
节点名称：${context.current.label}
节点描述：${context.current.aiDescription || '无'}

上下游信息：
- 上游节点：
${upstreamText}
- 下游节点：
${downstreamText}

用户需求：
${requirement}`;

  return [
    { role: 'system', content: SYSTEM_PROMPT },
    { role: 'user', content: userMessage },
  ];
}
```

- [ ] **Step 3: 添加 generateScript 函数**

在 `web/src/lib/script-generation.ts` 中追加：

```typescript
import { copilotComplete } from './tauri';
import type { AiCompletionRequest } from '../generated/AiCompletionRequest';
import type { AiGenerationParams } from '../generated/AiGenerationParams';

export interface GenerateScriptOptions {
  providerId: string;
  model?: string;
  timeoutMs?: number;
}

export async function generateScript(
  requirement: string,
  context: NodeContext,
  options: GenerateScriptOptions,
): Promise<string> {
  const messages = buildScriptGenerationPrompt(requirement, context);
  const params: AiGenerationParams = {
    temperature: 0.2,
    maxTokens: 2048,
    topP: 0.9,
  };
  const request: AiCompletionRequest = {
    providerId: options.providerId,
    model: options.model,
    messages,
    params,
    timeoutMs: options.timeoutMs ?? BigInt(60000),
  };
  const response = await copilotComplete(request);
  return response.content.trim();
}
```

- [ ] **Step 4: 编写单元测试**

创建 `web/src/lib/__tests__/script-generation.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import { buildScriptGenerationPrompt, getNodeContext, type NodeContext } from '../script-generation';

describe('buildScriptGenerationPrompt', () => {
  it('生成包含 system 和 user 两条消息', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '数据转换', aiDescription: '将温度值转为华氏度' },
      upstream: [{ nodeType: 'native', label: '传感器输入', aiDescription: '读取 Modbus 温度' }],
      downstream: [{ nodeType: 'httpClient', label: '上报数据', aiDescription: '' }],
    };
    const messages = buildScriptGenerationPrompt('将摄氏温度转为华氏温度', context);
    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe('system');
    expect(messages[0].content).toContain('Rhai');
    expect(messages[1].role).toBe('user');
    expect(messages[1].content).toContain('数据转换');
    expect(messages[1].content).toContain('传感器输入');
    expect(messages[1].content).toContain('上报数据');
    expect(messages[1].content).toContain('将摄氏温度转为华氏温度');
  });

  it('无上下游时输出"无"', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '独立节点', aiDescription: '' },
      upstream: [],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('空脚本', context);
    expect(messages[1].content).toContain('上游节点：\n  无');
    expect(messages[1].content).toContain('下游节点：\n  无');
  });

  it('节点描述为空时显示"无"', () => {
    const context: NodeContext = {
      current: { nodeType: 'rhai', label: '测试节点', aiDescription: '' },
      upstream: [],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('测试需求', context);
    expect(messages[1].content).toContain('节点描述：无');
  });

  it('上游节点含 aiDescription 时包含描述信息', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '处理节点', aiDescription: '处理数据' },
      upstream: [{ nodeType: 'native', label: '输入', aiDescription: '读取传感器' }],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('需求', context);
    expect(messages[1].content).toContain('描述: 读取传感器');
  });

  it('上游节点无 aiDescription 时不包含描述字段', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '处理节点', aiDescription: '' },
      upstream: [{ nodeType: 'native', label: '输入', aiDescription: '' }],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('需求', context);
    expect(messages[1].content).not.toContain('描述:');
  });
});
```

- [ ] **Step 5: 运行测试确认通过**

Run: `npm --prefix web run test -- --run web/src/lib/__tests__/script-generation.test.ts`
Expected: All tests PASS

- [ ] **Step 6: 提交**

```bash
git add web/src/lib/script-generation.ts web/src/lib/__tests__/script-generation.test.ts
git commit -s -m "feat: 添加脚本生成 prompt 构建与节点上下文提取逻辑"
```

---

### Task 2: 需求输入弹窗组件

**Files:**
- Create: `web/src/components/flowgram/AiScriptGenerator.tsx`

- [ ] **Step 1: 创建 AiScriptGenerator 弹窗组件**

```tsx
import { useCallback, useState } from 'react';

export interface AiScriptGeneratorProps {
  open: boolean;
  loading: boolean;
  error: string | null;
  onGenerate: (requirement: string) => void;
  onClose: () => void;
}

export function AiScriptGenerator({
  open,
  loading,
  error,
  onGenerate,
  onClose,
}: AiScriptGeneratorProps) {
  const [requirement, setRequirement] = useState('');

  const handleGenerate = useCallback(() => {
    const trimmed = requirement.trim();
    if (!trimmed) {
      return;
    }
    onGenerate(trimmed);
  }, [requirement, onGenerate]);

  const handleCancel = useCallback(() => {
    if (loading) {
      return;
    }
    setRequirement('');
    onClose();
  }, [loading, onClose]);

  if (!open) {
    return null;
  }

  return (
    <div className="flowgram-overlay" onClick={handleCancel}>
      <section className="flowgram-modal" onClick={(e) => e.stopPropagation()}>
        <div className="flowgram-modal__header">
          <h4>AI 生成脚本</h4>
        </div>
        <div className="flowgram-form">
          <label>
            <span>需求描述</span>
            <textarea
              value={requirement}
              onChange={(e) => setRequirement(e.target.value)}
              placeholder="描述你希望脚本实现的功能..."
              disabled={loading}
              rows={5}
              autoFocus
            />
          </label>
        </div>
        {error ? (
          <div className="flowgram-notes">
            <article className="flowgram-note flowgram-note--danger">{error}</article>
          </div>
        ) : null}
        <div className="flowgram-modal__actions">
          <button type="button" className="ghost" onClick={handleCancel} disabled={loading}>
            取消
          </button>
          <button
            type="button"
            onClick={handleGenerate}
            disabled={loading || !requirement.trim()}
          >
            {loading ? '生成中...' : '生成'}
          </button>
        </div>
      </section>
    </div>
  );
}
```

- [ ] **Step 2: 提交**

```bash
git add web/src/components/flowgram/AiScriptGenerator.tsx
git commit -s -m "feat: 添加 AI 脚本生成需求输入弹窗组件"
```

---

### Task 3: 在设置面板中集成 AI 生成按钮与弹窗

**Files:**
- Modify: `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx`

- [ ] **Step 1: 添加导入与状态**

在 FlowgramNodeSettingsPanel.tsx 顶部导入区域追加：

```typescript
import { AiScriptGenerator } from './AiScriptGenerator';
import { generateScript, getNodeContext } from '../../lib/script-generation';
```

在组件函数体内部（`const [draft, setDraft]` 行之后）追加状态：

```typescript
const [aiGeneratorOpen, setAiGeneratorOpen] = useState(false);
const [aiGenerating, setAiGenerating] = useState(false);
const [aiGenerateError, setAiGenerateError] = useState<string | null>(null);
```

- [ ] **Step 2: 添加生成触发函数**

在 `closePanel` 的 `useCallback` 之后追加：

```typescript
const hasAiProvider = aiProviders.length > 0 && !!activeAiProviderId;

const handleAiGenerate = useCallback(
  async (requirement: string) => {
    if (!node || !activeAiProviderId) {
      return;
    }
    setAiGenerating(true);
    setAiGenerateError(null);
    try {
      const context = getNodeContext(node);
      const script = await generateScript(requirement, context, {
        providerId: activeAiProviderId,
      });
      if (!script) {
        setAiGenerateError('AI 未返回有效代码。');
        return;
      }
      updateDraft({ script });
      setAiGeneratorOpen(false);
      setRequirement('');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setAiGenerateError(message || '生成失败，请重试。');
    } finally {
      setAiGenerating(false);
    }
  },
  [node, activeAiProviderId, updateDraft],
);

const handleAiGeneratorClose = useCallback(() => {
  if (aiGenerating) {
    return;
  }
  setAiGeneratorOpen(false);
  setAiGenerateError(null);
}, [aiGenerating]);
```

注意：上面 `setRequirement('')` 不存在，应替换为弹窗关闭时由弹窗组件自身管理。应在 `handleAiGenerate` 成功时仅关闭弹窗：

修正版 `handleAiGenerate`：

```typescript
const handleAiGenerate = useCallback(
  async (requirement: string) => {
    if (!node || !activeAiProviderId) {
      return;
    }
    setAiGenerating(true);
    setAiGenerateError(null);
    try {
      const context = getNodeContext(node);
      const script = await generateScript(requirement, context, {
        providerId: activeAiProviderId,
      });
      if (!script) {
        setAiGenerateError('AI 未返回有效代码。');
        return;
      }
      updateDraft({ script });
      setAiGeneratorOpen(false);
      setAiGenerateError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setAiGenerateError(message || '生成失败，请重试。');
    } finally {
      setAiGenerating(false);
    }
  },
  [node, activeAiProviderId, updateDraft],
);
```

- [ ] **Step 3: 在脚本编辑区添加 AI 生成按钮**

找到设置面板中 `isScriptNode(draft.nodeType)` 的脚本 textarea 标签（约第 873-878 行），将整个 `<label>` 替换为：

```tsx
{isScriptNode(draft.nodeType) ? (
  <label>
    <span>
      {getPrimaryEditorLabel(draft.nodeType)}
      {(draft.nodeType === 'code' || draft.nodeType === 'rhai') ? (
        <button
          type="button"
          className="ghost"
          disabled={!hasAiProvider || aiGenerating}
          onClick={() => {
            setAiGeneratorOpen(true);
            setAiGenerateError(null);
          }}
          title={!hasAiProvider ? '请先在 AI 配置中添加提供商' : 'AI 生成脚本'}
          style={{ marginLeft: '0.5em', fontSize: '0.85em', padding: '0.15em 0.5em' }}
        >
          {aiGenerating ? '生成中...' : 'AI 生成'}
        </button>
      ) : null}
    </span>
    <textarea value={draft.script} onChange={(event) => updateDraft({ script: event.target.value })} />
  </label>
) : null}
```

- [ ] **Step 4: 在面板 JSX 末尾添加弹窗组件**

在 `</section>` 结束标签（整个面板的最外层闭合标签前，即第 1352 行 `</section>` 之前）追加：

```tsx
<AiScriptGenerator
  open={aiGeneratorOpen}
  loading={aiGenerating}
  error={aiGenerateError}
  onGenerate={handleAiGenerate}
  onClose={handleAiGeneratorClose}
/>
```

- [ ] **Step 5: 添加弹窗与 overlay 的 CSS 样式**

在项目现有 CSS 文件中（找到 flowgram 相关样式文件的位置）追加：

```css
.flowgram-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 999;
}

.flowgram-modal {
  background: var(--color-surface, #fff);
  border-radius: 8px;
  padding: 1rem;
  min-width: 360px;
  max-width: 480px;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.15);
}

.flowgram-modal__header {
  margin-bottom: 0.75rem;
}

.flowgram-modal__actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  margin-top: 0.75rem;
}
```

注意：需先检查项目中是否有已有 overlay/modal 样式可复用，如有则优先复用。

- [ ] **Step 6: 运行前端构建确认无编译错误**

Run: `npm --prefix web run build`
Expected: 构建成功，无 TypeScript 错误

- [ ] **Step 7: 运行全部前端测试**

Run: `npm --prefix web run test -- --run`
Expected: 所有测试通过

- [ ] **Step 8: 提交**

```bash
git add web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx web/src/components/flowgram/AiScriptGenerator.tsx
git commit -s -m "feat: 集成 AI 脚本生成按钮与需求输入弹窗到节点设置面板"
```

---

### Task 4: 样式验证与集成测试

**Files:** 无新增

- [ ] **Step 1: 检查现有 CSS 文件中的 overlay/modal 样式**

搜索项目 CSS 中是否已有 overlay 或 modal 相关样式类。如果有，调整 Task 3 中的 CSS 类名以复用现有样式；如果没有，确认新增样式已生效。

- [ ] **Step 2: 手动集成验证**

启动 `cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch`，在运行中的桌面应用执行以下检查：
1. 选中一个 code/rhai 节点 → 设置面板中脚本编辑区旁出现「AI 生成」按钮
2. 全局未配置 AI → 按钮灰显，hover 显示 "请先在 AI 配置中添加提供商"
3. 全局已配置 AI → 点击按钮弹出需求输入弹窗
4. 输入需求并点击"生成" → 按钮显示"生成中..." → 成功后脚本内容被替换
5. 生成失败 → 弹窗内显示错误信息，可重试
6. 非code/rhai节点 → 不显示 AI 生成按钮

- [ ] **Step 3: 提交（如有样式调整）**

```bash
git add -A
git commit -s -m "fix: 调整 AI 脚本生成弹窗样式"
```
