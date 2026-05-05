import { describe, expect, it } from 'vitest';
import type { WorkflowNodeJSON } from '@flowgram.ai/free-layout-editor';

import {
  createFlowgramNodeRegistries,
  getFlowgramPaletteSections,
  getNodeCatalogInfo,
  normalizeFlowgramNodeJson,
  normalizeNodeKind,
  normalizeNodeConfig,
  getLogicNodeBranchDefinitions,
  getRoutingBranchDefinitions,
  validateNodeRegistry,
  type FlowgramConnectionDefaults,
} from '../../components/flowgram/flowgram-node-library';

const CONNECTION_DEFAULTS: FlowgramConnectionDefaults = {
  any: null,
  modbus: null,
  serial: null,
  mqtt: null,
  http: null,
  bark: null,
  can: null,
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
            model: 'deepseek-v4-flash',
            systemPrompt: '你是测试助手',
            temperature: 0.2,
            maxTokens: 128,
            topP: 0.85,
            thinking: { type: 'enabled' },
            reasoningEffort: 'max',
            timeoutMs: 5000,
          },
        },
      },
    };

    const normalized = normalizeFlowgramNodeJson(json, CONNECTION_DEFAULTS);

    expect((normalized.data as { config?: { ai?: unknown } })?.config?.ai).toEqual({
      providerId: 'deepseek',
      model: 'deepseek-v4-flash',
      systemPrompt: '你是测试助手',
      temperature: 0.2,
      maxTokens: 128,
      topP: 0.85,
      thinking: { type: 'enabled' },
      reasoningEffort: 'max',
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

  it('data label 缺失时显示名称回退到节点类型默认名而不是 node id', () => {
    const json: WorkflowNodeJSON = {
      id: 'loop_1',
      type: 'loop',
      meta: { position: { x: 0, y: 0 } },
      data: {
        nodeType: 'loop',
        config: {},
      },
    };

    const normalized = normalizeFlowgramNodeJson(json, CONNECTION_DEFAULTS);

    expect((normalized.data as { label?: string })?.label).toBe('Loop Node');
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

  it('barkPush 节点会剥离传输配置并补齐通知字段默认值', () => {
    const result = normalizeNodeConfig('barkPush', {
      device_key: 'demo-key',
      content_mode: 'markdown',
    });

    expect(result).toMatchObject({
      content_mode: 'markdown',
      level: 'active',
    });
    expect(result).not.toHaveProperty('server_url');
    expect(result).not.toHaveProperty('device_key');
    expect(result).not.toHaveProperty('request_timeout_ms');
  });

  it('httpClient 节点会剥离传输配置并保留消息模板配置', () => {
    const result = normalizeNodeConfig('httpClient', {
      url: 'https://example.com/hook',
      method: 'POST',
      body_mode: 'template',
    });

    expect(result).toMatchObject({
      body_mode: 'template',
      title_template: expect.any(String),
    });
    expect(result).not.toHaveProperty('url');
    expect(result).not.toHaveProperty('method');
    expect(result).not.toHaveProperty('request_timeout_ms');
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

describe('NodeDefinition registry', () => {
  it('启动期注册表校验通过', () => {
    expect(() => validateNodeRegistry()).not.toThrow();
  });

  it('palette 从定义派生并隐藏子图桥接节点', () => {
    const sections = getFlowgramPaletteSections();
    const items = sections.flatMap((section) => section.items);
    const keys = items.map((item) => item.seed.kind);

    expect(keys).toContain('timer');
    expect(keys).toContain('humanLoop');
    expect(keys).not.toContain('subgraphInput');
    expect(keys).not.toContain('subgraphOutput');
  });

  it('PluginPanel catalog 对第三方运行时节点落到其他分类', () => {
    expect(getNodeCatalogInfo('opencv/detect')).toEqual({
      category: '其他',
      description: '运行时或第三方节点',
    });
    expect(getNodeCatalogInfo('timer')).toMatchObject({
      category: '流程控制',
    });
  });

  it('区分具名输出端口和运行时路由分支', () => {
    expect(getLogicNodeBranchDefinitions('modbusRead', {})).toEqual([
      { key: 'out', label: 'out' },
      { key: 'latest', label: 'latest' },
    ]);
    expect(getRoutingBranchDefinitions('modbusRead', {})).toEqual([]);
    expect(getRoutingBranchDefinitions('humanLoop', {})).toEqual([
      { key: 'approve', label: 'Approve', fixed: true },
      { key: 'reject', label: 'Reject', fixed: true },
    ]);
  });
});

describe('createFlowgramNodeRegistries', () => {
  it('loop 节点注册为容器并自带迭代桥接节点', () => {
    const registries = createFlowgramNodeRegistries(CONNECTION_DEFAULTS);
    const loopRegistry = registries.find((registry) => registry.type === 'loop');
    const meta = loopRegistry?.meta as
      | {
          isContainer?: boolean;
          size?: { width?: number; height?: number };
          defaultPorts?: Array<{ type?: string }>;
        }
      | undefined;

    expect(meta).toMatchObject({
      isContainer: true,
      size: { width: 344, height: 136 },
      defaultPorts: [{ type: 'input' }, { type: 'output' }],
    });

    const nodeJson = loopRegistry?.onAdd?.({} as never) as
      | {
          data?: { label?: string };
          blocks?: Array<{ id?: string; type?: string; meta?: { position?: { x?: number; y?: number } } }>;
        }
      | undefined;

    expect(nodeJson?.data?.label).toBe('Loop Node');
    expect(nodeJson?.blocks).toEqual([
      {
        id: 'loop-iterate',
        type: 'subgraphInput',
        meta: { position: { x: 0, y: 0 } },
        data: {
          label: 'Iterate',
          nodeType: 'subgraphInput',
          config: {},
        },
      },
      {
        id: 'loop-emit',
        type: 'subgraphOutput',
        meta: { position: { x: 200, y: 0 } },
        data: {
          label: 'Emit',
          nodeType: 'subgraphOutput',
          config: {},
        },
      },
    ]);
  });
});
