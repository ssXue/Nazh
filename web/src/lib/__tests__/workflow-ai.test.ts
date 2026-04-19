import { describe, expect, it } from 'vitest';

import {
  applyGlobalAiConfigToWorkflowGraph,
  stripWorkflowNodeLocalAiConfig,
} from '../workflow-ai';
import type { AiConfigView, WorkflowGraph } from '../../types';

function createWorkflowGraph(): WorkflowGraph {
  return {
    name: '测试工作流',
    connections: [],
    nodes: {
      input: {
        type: 'native',
        config: {
          message: 'hello',
        },
      },
      code: {
        type: 'code',
        config: {
          script: 'payload["reply"] = ai_complete("hello"); payload',
          retries: 2,
          ai: {
            providerId: 'legacy-provider',
            model: 'legacy-model',
          },
        },
      },
      code_secondary: {
        type: 'code',
        config: {
          script: 'payload',
        },
      },
      branch: {
        type: 'if',
        config: {
          script: 'true',
          ai: {
            providerId: 'should-stay-untouched',
          },
        },
      },
    },
    edges: [
      { from: 'input', to: 'code' },
      { from: 'code', to: 'code_secondary' },
    ],
  } as WorkflowGraph;
}

function createAiConfig(): AiConfigView {
  return {
    version: 1,
    providers: [
      {
        id: 'deepseek',
        name: 'DeepSeek',
        baseUrl: 'https://api.deepseek.com/v1',
        defaultModel: 'deepseek-chat',
        extraHeaders: {},
        enabled: true,
        hasApiKey: true,
      },
    ],
    activeProviderId: 'deepseek',
    copilotParams: {
      temperature: 0.2,
      maxTokens: 512,
      topP: 0.9,
    },
    agentSettings: {
      systemPrompt: '你是测试助手',
      timeoutMs: 4000,
    },
  };
}

describe('stripWorkflowNodeLocalAiConfig', () => {
  it('会移除 code 节点上的本地 ai 配置，但保留其他配置', () => {
    const result = stripWorkflowNodeLocalAiConfig(createWorkflowGraph());

    expect(result.nodes['code']?.config).toEqual({
      script: 'payload["reply"] = ai_complete("hello"); payload',
      retries: 2,
    });
    expect(result.nodes['code_secondary']?.config).toEqual({
      script: 'payload',
    });
    expect(result.nodes['branch']?.config).toEqual({
      script: 'true',
      ai: {
        providerId: 'should-stay-untouched',
      },
    });
  });
});

describe('applyGlobalAiConfigToWorkflowGraph', () => {
  it('会把全局 AI 注入到 code 节点，并覆盖旧的本地 ai 配置', () => {
    const result = applyGlobalAiConfigToWorkflowGraph(
      createWorkflowGraph(),
      createAiConfig(),
    );

    expect(result.nodes['code']?.config).toEqual({
      script: 'payload["reply"] = ai_complete("hello"); payload',
      retries: 2,
      ai: {
        providerId: 'deepseek',
        model: 'deepseek-chat',
        systemPrompt: '你是测试助手',
        temperature: 0.2,
        maxTokens: 512,
        topP: 0.9,
        timeoutMs: 4000,
      },
    });
    expect(result.nodes['code_secondary']?.config).toEqual({
      script: 'payload',
      ai: {
        providerId: 'deepseek',
        model: 'deepseek-chat',
        systemPrompt: '你是测试助手',
        temperature: 0.2,
        maxTokens: 512,
        topP: 0.9,
        timeoutMs: 4000,
      },
    });
    expect(result.nodes['branch']?.config).toEqual({
      script: 'true',
      ai: {
        providerId: 'should-stay-untouched',
      },
    });
  });

  it('没有全局 AI 时只清理脚本节点本地 ai 配置', () => {
    const result = applyGlobalAiConfigToWorkflowGraph(createWorkflowGraph(), null);

    expect(result.nodes['code']?.config).toEqual({
      script: 'payload["reply"] = ai_complete("hello"); payload',
      retries: 2,
    });
    expect(result.nodes['code_secondary']?.config).toEqual({
      script: 'payload',
    });
  });
});
