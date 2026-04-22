import { describe, expect, it } from 'vitest';
import type { WorkflowNodeJSON } from '@flowgram.ai/free-layout-editor';

import {
  normalizeFlowgramNodeJson,
  normalizeNodeKind,
  normalizeNodeConfig,
  type FlowgramConnectionDefaults,
} from '../../components/flowgram/flowgram-node-library';

const CONNECTION_DEFAULTS: FlowgramConnectionDefaults = {
  any: null,
  modbus: null,
  serial: null,
  mqtt: null,
};

describe('normalizeFlowgramNodeJson', () => {
  it('保留 code 节点的 AI 配置', () => {
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

  it('保留 code 节点的 script 属性', () => {
    const json: WorkflowNodeJSON = {
      id: 'code_1',
      type: 'code',
      meta: { position: { x: 0, y: 0 } },
      data: {
        label: 'Code Node',
        nodeType: 'code',
        config: {
          script: 'payload["x"] = 1; payload',
        },
      },
    };

    const normalized = normalizeFlowgramNodeJson(json, CONNECTION_DEFAULTS);
    const config = (normalized.data as { config?: { script?: string } })?.config;

    expect(config?.script).toBe('payload["x"] = 1; payload');
  });

  it('code 节点无 script 时回退到默认值 payload', () => {
    const json: WorkflowNodeJSON = {
      id: 'code_1',
      type: 'code',
      meta: { position: { x: 0, y: 0 } },
      data: {
        label: 'Code Node',
        nodeType: 'code',
        config: {},
      },
    };

    const normalized = normalizeFlowgramNodeJson(json, CONNECTION_DEFAULTS);
    const config = (normalized.data as { config?: { script?: string } })?.config;

    expect(config?.script).toBe('payload');
  });

  it('data 缺失时仍能正确归一化 code 节点', () => {
    const json: WorkflowNodeJSON = {
      id: 'code_1',
      type: 'code',
      meta: { position: { x: 0, y: 0 } },
    };

    const normalized = normalizeFlowgramNodeJson(json, CONNECTION_DEFAULTS);

    expect(normalized.type).toBe('code');
    const config = (normalized.data as { config?: { script?: string } })?.config;
    expect(config?.script).toBe('payload');
  });
});

describe('normalizeNodeConfig', () => {
  it('code 节点保留 script 字符串', () => {
    const result = normalizeNodeConfig('code', { script: 'payload + 1' });
    expect(result).toEqual({ script: 'payload + 1' });
  });

  it('code 节点 config 为空对象时回退到 payload', () => {
    const result = normalizeNodeConfig('code', {});
    expect(result).toEqual({ script: 'payload' });
  });

  it('code 节点 config 为 undefined 时回退到 payload', () => {
    const result = normalizeNodeConfig('code', undefined);
    expect(result).toEqual({ script: 'payload' });
  });

  it('barkPush 节点会补齐默认 Bark 配置', () => {
    const result = normalizeNodeConfig('barkPush', {
      device_key: 'demo-key',
      content_mode: 'markdown',
    });

    expect(result).toMatchObject({
      server_url: 'https://api.day.app',
      device_key: 'demo-key',
      content_mode: 'markdown',
      level: 'active',
      request_timeout_ms: 4000,
    });
  });
});

describe('normalizeNodeKind', () => {
  it('code 类型保持不变', () => {
    expect(normalizeNodeKind('code')).toBe('code');
  });

  it('barkPush 类型保持不变', () => {
    expect(normalizeNodeKind('barkPush')).toBe('barkPush');
  });

  it('未知类型回退到 native', () => {
    expect(normalizeNodeKind('rhai')).toBe('native');
    expect(normalizeNodeKind('unknown')).toBe('native');
  });
});
