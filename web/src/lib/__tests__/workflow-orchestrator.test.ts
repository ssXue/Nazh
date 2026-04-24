import { describe, expect, it, vi } from 'vitest';

import type { WorkflowGraph } from '../../types';
import {
  applyWorkflowOrchestrationOperation,
  buildWorkflowOrchestrationPrompt,
  createEmptyWorkflowDraft,
  createWorkflowOrchestrationState,
  streamWorkflowOrchestration,
} from '../workflow-orchestrator';

vi.mock('../../lib/tauri', () => ({
  copilotCompleteStream: vi.fn(),
}));

describe('buildWorkflowOrchestrationPrompt', () => {
  it('create 模式会要求输出 JSON Lines 流式编排协议', () => {
    const messages = buildWorkflowOrchestrationPrompt({
      mode: 'create',
      requirement: '做一个定时采集并记录 SQLite 的温度工作流',
    });

    expect(messages).toHaveLength(2);
    expect(messages[0].content).toContain('JSON Lines');
    expect(messages[0].content).toContain('不要等全部设计完再统一输出');
    expect(messages[0].content).toContain('timer');
    expect(messages[1].content).toContain('create（从空白开始编排新工作流）');
    expect(messages[1].content).toContain('做一个定时采集并记录 SQLite 的温度工作流');
  });

  it('edit 模式会带上当前工作流上下文', () => {
    const draft = createEmptyWorkflowDraft('现有工程');
    draft.description = '待优化';
    draft.graph = {
      ...draft.graph,
      nodes: {
        code_clean: {
          type: 'code',
          config: {
            script: 'payload',
          },
        },
      },
    } as WorkflowGraph;

    const messages = buildWorkflowOrchestrationPrompt({
      mode: 'edit',
      requirement: '把输出改成 HTTP 上报',
      baseDraft: draft,
    });

    expect(messages[1].content).toContain('edit（基于当前工作流流式修改）');
    expect(messages[1].content).toContain('"code_clean"');
    expect(messages[1].content).toContain('把输出改成 HTTP 上报');
  });
});

describe('applyWorkflowOrchestrationOperation', () => {
  it('可以逐步生成节点和边', () => {
    let state = createWorkflowOrchestrationState(createEmptyWorkflowDraft('AI 草稿'));

    state = applyWorkflowOrchestrationOperation(state, {
      type: 'project',
      name: '锅炉巡检',
      description: 'AI 生成的巡检流程',
      payloadText: '{"manual":true}',
    });
    state = applyWorkflowOrchestrationOperation(state, {
      type: 'upsert_node',
      id: 'timer_trigger',
      nodeType: 'timer',
      label: '定时触发',
      config: {
        interval_ms: 5000,
        immediate: true,
      },
    });
    state = applyWorkflowOrchestrationOperation(state, {
      type: 'upsert_node',
      id: 'debug_console',
      nodeType: 'debugConsole',
      label: '调试输出',
      config: {
        label: 'ai-preview',
      },
    });
    state = applyWorkflowOrchestrationOperation(state, {
      type: 'upsert_edge',
      from: 'timer_trigger',
      to: 'debug_console',
    });

    expect(state.draft.name).toBe('锅炉巡检');
    expect(state.draft.description).toBe('AI 生成的巡检流程');
    expect(state.draft.payloadText).toContain('"manual": true');
    expect(state.draft.graph.nodes['timer_trigger']?.type).toBe('timer');
    expect(state.draft.graph.nodes['debug_console']?.type).toBe('debugConsole');
    expect(state.nodeLabels['timer_trigger']).toBe('定时触发');
    expect(state.draft.graph.edges).toEqual([
      {
        from: 'timer_trigger',
        to: 'debug_console',
        source_port_id: undefined,
        target_port_id: undefined,
      },
    ]);
    expect(
      state.draft.graph.editor_graph?.nodes.find((node) => node.id === 'timer_trigger')?.data,
    ).toMatchObject({
      label: '定时触发',
    });
  });

  it('编辑节点时不会因为缺少 connectionId 而清空已有绑定', () => {
    let state = createWorkflowOrchestrationState({
      name: '已有工程',
      description: '已有说明',
      payloadText: '{}',
      graph: {
        name: '已有工程',
        connections: [],
        nodes: {
          http_alarm: {
            type: 'httpClient',
            connection_id: 'alarm-http',
            config: {
              body_mode: 'json',
            },
          },
        },
        edges: [],
      },
    });

    state = applyWorkflowOrchestrationOperation(state, {
      type: 'upsert_node',
      id: 'http_alarm',
      nodeType: 'httpClient',
      config: {
        body_mode: 'template',
        body_template: '告警：{{payload}}',
      },
    });

    expect(state.draft.graph.nodes['http_alarm']?.connection_id).toBe('alarm-http');
    expect(state.draft.graph.nodes['http_alarm']?.config).toMatchObject({
      body_mode: 'template',
      body_template: '告警：{{payload}}',
    });
  });
});

describe('streamWorkflowOrchestration', () => {
  it('能够从流式 JSON Lines 中逐步解析并归约工作流', async () => {
    const { copilotCompleteStream } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotCompleteStream);
    mocked.mockImplementationOnce(async (_request, onDelta) => {
      const lines = [
        '{"type":"project","name":"AI 巡检工程","description":"流式生成","payloadText":"{\\"manual\\":true}"}\n',
        '{"type":"upsert_node","id":"timer_trigger","nodeType":"timer","label":"定时触发","config":{"interval_ms":3000,"immediate":true}}\n',
        '{"type":"upsert_node","id":"debug_console","nodeType":"debugConsole","label":"调试输出","config":{"label":"stream"}}\n',
        '{"type":"upsert_edge","from":"timer_trigger","to":"debug_console"}\n',
        '{"type":"done","summary":"完成"}',
      ];
      let accumulated = '';
      for (const line of lines) {
        accumulated += line;
        onDelta(accumulated);
      }
      return {
        text: accumulated,
        finishReason: 'stop',
      };
    });

    const seenOperations: string[] = [];
    const result = await streamWorkflowOrchestration({
      mode: 'create',
      requirement: '做一个最小流式示例',
      providerId: 'test-provider',
      onOperation: (operation) => {
        seenOperations.push(operation.type);
      },
    });

    expect(seenOperations).toEqual([
      'project',
      'upsert_node',
      'upsert_node',
      'upsert_edge',
      'done',
    ]);
    expect(result.draft.name).toBe('AI 巡检工程');
    expect(result.summary).toBe('完成');
    expect(result.draft.graph.edges).toHaveLength(1);
    expect(result.draft.graph.nodes['timer_trigger']?.type).toBe('timer');
  });

  it('流结束但缺少 done 操作时会抛出中断错误，并保留已解析的草稿', async () => {
    const { copilotCompleteStream } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotCompleteStream);
    mocked
      .mockImplementationOnce(async (_request, onDelta) => {
        const lines = [
          '{"type":"project","name":"未完成工程"}\n',
          '{"type":"upsert_node","id":"timer_trigger","nodeType":"timer","label":"定时触发","config":{"interval_ms":3000}}\n',
        ];
        let accumulated = '';
        for (const line of lines) {
          accumulated += line;
          onDelta(accumulated);
        }
        return {
          text: accumulated,
          finishReason: 'length',
        };
      })
      .mockImplementationOnce(async (_request, onDelta) => {
        const lines = [
          '{"type":"upsert_node","id":"debug_console","nodeType":"debugConsole","label":"调试输出","config":{"label":"resume"}}\n',
          '{"type":"upsert_edge","from":"timer_trigger","to":"debug_console"}\n',
          '{"type":"done","summary":"续传完成"}',
        ];
        let accumulated = '';
        for (const line of lines) {
          accumulated += line;
          onDelta(accumulated);
        }
        return {
          text: accumulated,
          finishReason: 'stop',
        };
      });

    const retries: Array<'retry' | 'resume' | 'restart'> = [];
    const result = await streamWorkflowOrchestration({
      mode: 'create',
      requirement: '做一个会中断的示例',
      providerId: 'test-provider',
      onRetry: (_attempt, _error, _state, strategy) => {
        retries.push(strategy);
      },
    });

    expect(retries).toEqual(['resume']);
    expect(result.summary).toBe('续传完成');
    expect(result.draft.graph.nodes['timer_trigger']?.type).toBe('timer');
    expect(result.draft.graph.nodes['debug_console']?.type).toBe('debugConsole');
    expect(result.draft.graph.edges).toEqual([
      {
        from: 'timer_trigger',
        to: 'debug_console',
        source_port_id: undefined,
        target_port_id: undefined,
      },
    ]);
  });

  it('只有思考流中断且尚未进入协议输出时，会原位重试当前请求', async () => {
    const { copilotCompleteStream } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotCompleteStream);
    mocked
      .mockImplementationOnce(async (_request, _onDelta, onThinking) => {
        onThinking?.('先想一下');
        throw new Error('AI 网络错误: error decoding response body');
      })
      .mockImplementationOnce(async (_request, onDelta) => {
        const lines = [
          '{"type":"project","name":"重试成功工程"}\n',
          '{"type":"upsert_node","id":"timer_trigger","nodeType":"timer","label":"定时触发","config":{"interval_ms":1000}}\n',
          '{"type":"done","summary":"重试成功"}',
        ];
        let accumulated = '';
        for (const line of lines) {
          accumulated += line;
          onDelta(accumulated);
        }
        return {
          text: accumulated,
          finishReason: 'stop',
        };
      });

    const retries: Array<'retry' | 'resume' | 'restart'> = [];
    const thinkingSnapshots: string[] = [];
    const result = await streamWorkflowOrchestration({
      mode: 'create',
      requirement: '做一个先思考后输出的示例',
      providerId: 'test-provider',
      onThinking: (thinking) => {
        thinkingSnapshots.push(thinking);
      },
      onRetry: (_attempt, _error, _state, strategy) => {
        retries.push(strategy);
      },
    });

    expect(retries).toEqual(['retry']);
    expect(thinkingSnapshots).toContain('先想一下');
    expect(thinkingSnapshots).toContain('');
    expect(result.draft.name).toBe('重试成功工程');
    expect(result.summary).toBe('重试成功');
  });

  it('续传阶段若只发生传输中断，会原位重试同一个 continuation 请求', async () => {
    const { copilotCompleteStream } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotCompleteStream);
    const requestMessages: string[] = [];
    mocked
      .mockImplementationOnce(async (request, onDelta) => {
        requestMessages.push(JSON.stringify(request.messages));
        const lines = [
          '{"type":"project","name":"续传工程"}\n',
          '{"type":"upsert_node","id":"timer_trigger","nodeType":"timer","label":"定时触发","config":{"interval_ms":3000}}\n',
        ];
        let accumulated = '';
        for (const line of lines) {
          accumulated += line;
          onDelta(accumulated);
        }
        return {
          text: accumulated,
          finishReason: 'length',
        };
      })
      .mockImplementationOnce(async (request, _onDelta, onThinking) => {
        requestMessages.push(JSON.stringify(request.messages));
        onThinking?.('继续补全中');
        throw new Error('AI 网络错误: error decoding response body');
      })
      .mockImplementationOnce(async (request, onDelta) => {
        requestMessages.push(JSON.stringify(request.messages));
        const lines = [
          '{"type":"upsert_node","id":"debug_console","nodeType":"debugConsole","label":"调试输出","config":{"label":"resume"}}\n',
          '{"type":"upsert_edge","from":"timer_trigger","to":"debug_console"}\n',
          '{"type":"done","summary":"续传完成"}',
        ];
        let accumulated = '';
        for (const line of lines) {
          accumulated += line;
          onDelta(accumulated);
        }
        return {
          text: accumulated,
          finishReason: 'stop',
        };
      });

    const retries: Array<'retry' | 'resume' | 'restart'> = [];
    const result = await streamWorkflowOrchestration({
      mode: 'create',
      requirement: '做一个续传后仍可能波动的示例',
      providerId: 'test-provider',
      onRetry: (_attempt, _error, _state, strategy) => {
        retries.push(strategy);
      },
    });

    expect(retries).toEqual(['resume', 'retry']);
    expect(requestMessages).toHaveLength(3);
    expect(requestMessages[1]).toBe(requestMessages[2]);
    expect(requestMessages[0]).not.toBe(requestMessages[1]);
    expect(result.summary).toBe('续传完成');
    expect(result.draft.graph.nodes['debug_console']?.type).toBe('debugConsole');
  });
});
