import { describe, expect, it } from 'vitest';
import type { WorkflowNodeJSON } from '@flowgram.ai/free-layout-editor';

import {
  normalizeFlowgramNodeJson,
  type FlowgramConnectionDefaults,
} from './flowgram-node-library';

const CONNECTION_DEFAULTS: FlowgramConnectionDefaults = {
  any: null,
  modbus: null,
  serial: null,
};

describe('normalizeFlowgramNodeJson', () => {
  it('保留 code/rhai 节点的 AI 配置', () => {
    const json: WorkflowNodeJSON = {
      id: 'code_1',
      type: 'code',
      meta: {
        position: { x: 120, y: 48 },
      },
      data: {
        label: 'Code Node',
        nodeType: 'code',
        config: {
          script: 'payload["reply"] = ai_complete("hello"); payload',
          ai: {
            providerId: 'deepseek',
            model: 'deepseek-chat',
            systemPrompt: '你是测试助手',
            temperature: 0.2,
            maxTokens: 128,
            topP: 0.85,
            timeoutMs: 5000,
          },
        },
      },
    };

    const normalized = normalizeFlowgramNodeJson(json, CONNECTION_DEFAULTS);

    expect((normalized.data as { config?: { ai?: unknown } })?.config?.ai).toEqual({
      providerId: 'deepseek',
      model: 'deepseek-chat',
      systemPrompt: '你是测试助手',
      temperature: 0.2,
      maxTokens: 128,
      topP: 0.85,
      timeoutMs: 5000,
    });
  });
});
