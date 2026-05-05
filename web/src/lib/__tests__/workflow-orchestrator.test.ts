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
    expect(messages[0].content).toContain('真实 node id 由 Nazh 创建');
    expect(messages[0].content).toContain('"type":"create_node"');
    expect(messages[0].content).toContain('timer');
    expect(messages[0].content).toContain('switch: sourcePortId 使用该节点 config.branches[].key');
    expect(messages[0].content).toContain('humanLoop: sourcePortId 只能是 approve / reject');
    expect(messages[0].content).not.toContain('modbusRead: sourcePortId');
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
    expect(messages[1].content).toContain('"ref": "code_1"');
    expect(messages[1].content).not.toContain('"code_clean"');
    expect(messages[1].content).toContain('把输出改成 HTTP 上报');
  });

  it('会把设备和能力资产上下文交给 AI 编辑', () => {
    const messages = buildWorkflowOrchestrationPrompt({
      mode: 'edit',
      requirement: '用压机能力完成压装',
      assetContext: {
        devices: [
          {
            id: 'press_1',
            name: '压机',
            deviceType: 'hydraulic_press',
            version: 2,
            yamlFilePath: '/tmp/workspace/dsl/devices/press_1.device.yaml',
            yaml: 'id: press_1\ntype: hydraulic_press\nconnection:\n  type: modbus-tcp\n  id: press_modbus\n',
          },
        ],
        capabilities: [
          {
            id: 'press.apply_pressure',
            deviceId: 'press_1',
            name: 'apply_pressure',
            description: '加压',
            version: 1,
            yamlFilePath: null,
            yaml: 'id: press.apply_pressure\ndevice_id: press_1\nimplementation:\n  type: modbus-write\n  register: 40020\n  value: "${target}"\nsafety:\n  level: low\n',
          },
        ],
      },
    });

    expect(messages[0].content).toContain('优先使用这些已审查 Device DSL / Capability DSL');
    expect(messages[0].content).toContain('capabilityCall');
    expect(messages[1].content).toContain('Device DSL (1)');
    expect(messages[1].content).toContain('press.apply_pressure');
    expect(messages[1].content).toContain('press_modbus');
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
      type: 'create_node',
      ref: 'timer',
      nodeType: 'timer',
      label: '定时触发',
      config: {
        interval_ms: 5000,
        immediate: true,
      },
    });
    state = applyWorkflowOrchestrationOperation(state, {
      type: 'create_node',
      ref: 'debug',
      nodeType: 'debugConsole',
      label: '调试输出',
      config: {
        label: 'ai-preview',
      },
    });
    state = applyWorkflowOrchestrationOperation(state, {
      type: 'create_edge',
      fromRef: 'timer',
      toRef: 'debug',
    });

    expect(state.draft.name).toBe('锅炉巡检');
    expect(state.draft.description).toBe('AI 生成的巡检流程');
    expect(state.draft.payloadText).toContain('"manual": true');
    expect(state.nodeRefs.timer).toBe('timer_node_1');
    expect(state.nodeRefs.debug).toBe('debug_console_1');
    expect(state.draft.graph.nodes['timer_node_1']?.type).toBe('timer');
    expect(state.draft.graph.nodes['debug_console_1']?.type).toBe('debugConsole');
    expect(state.nodeLabels['timer_node_1']).toBe('定时触发');
    expect(state.draft.graph.edges).toEqual([
      {
        from: 'timer_node_1',
        to: 'debug_console_1',
        source_port_id: undefined,
        target_port_id: undefined,
      },
    ]);
    expect(
      state.draft.graph.editor_graph?.nodes.find((node) => node.id === 'timer_node_1')?.data,
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
      type: 'update_node',
      ref: 'http_client_1',
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

  it('AI 新增节点时由系统分配不冲突的真实 id', () => {
    let state = createWorkflowOrchestrationState({
      name: '已有工程',
      description: '已有说明',
      payloadText: '{}',
      graph: {
        name: '已有工程',
        connections: [],
        nodes: {
          timer_node_1: {
            type: 'timer',
            config: {
              interval_ms: 1000,
            },
          },
        },
        edges: [],
      },
    });

    state = applyWorkflowOrchestrationOperation(state, {
      type: 'create_node',
      ref: 'new_timer',
      nodeType: 'timer',
      config: {
        interval_ms: 5000,
      },
    });

    expect(state.nodeRefs.timer_1).toBe('timer_node_1');
    expect(state.nodeRefs.new_timer).toBe('timer_node_2');
    expect(state.draft.graph.nodes['timer_node_2']?.config).toMatchObject({
      interval_ms: 5000,
    });
  });

  it('AI 新增 loop 节点时补齐容器内部出入桥接点', () => {
    let state = createWorkflowOrchestrationState(createEmptyWorkflowDraft('AI 草稿'));

    state = applyWorkflowOrchestrationOperation(state, {
      type: 'create_node',
      ref: 'loop_items',
      nodeType: 'loop',
      label: '逐项处理',
      config: {
        script: '[payload]',
      },
    });

    const loopNode = state.draft.graph.editor_graph?.nodes.find((node) => node.id === 'loop_node_1');

    expect((loopNode?.data as { label?: string } | undefined)?.label).toBe('逐项处理');
    expect(loopNode?.blocks).toEqual([
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

  it('AI 新增节点未给 label 时显示名称回退到节点类型默认名', () => {
    let state = createWorkflowOrchestrationState(createEmptyWorkflowDraft('AI 草稿'));

    state = applyWorkflowOrchestrationOperation(state, {
      type: 'create_node',
      ref: 'loop_items',
      nodeType: 'loop',
      config: {
        script: '[payload]',
      },
    });

    const loopNode = state.draft.graph.editor_graph?.nodes.find((node) => node.id === 'loop_node_1');

    expect(state.nodeLabels.loop_node_1).toBe('Loop Node');
    expect((loopNode?.data as { label?: string } | undefined)?.label).toBe('Loop Node');
  });
});

describe('streamWorkflowOrchestration', () => {
  it('能够从流式 JSON Lines 中逐步解析并归约工作流', async () => {
    const { copilotCompleteStream } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotCompleteStream);
    mocked.mockImplementationOnce(async (_request, onDelta) => {
      const lines = [
        '{"type":"project","name":"AI 巡检工程","description":"流式生成","payloadText":"{\\"manual\\":true}"}\n',
        '{"type":"create_node","ref":"timer","nodeType":"timer","label":"定时触发","config":{"interval_ms":3000,"immediate":true}}\n',
        '{"type":"create_node","ref":"debug","nodeType":"debugConsole","label":"调试输出","config":{"label":"stream"}}\n',
        '{"type":"create_edge","fromRef":"timer","toRef":"debug"}\n',
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
      'create_node',
      'create_node',
      'create_edge',
      'done',
    ]);
    expect(result.draft.name).toBe('AI 巡检工程');
    expect(result.summary).toBe('完成');
    expect(result.draft.graph.edges).toHaveLength(1);
    expect(result.draft.graph.nodes['timer_node_1']?.type).toBe('timer');
  });

  it('流结束但缺少 done 操作时会抛出中断错误，并保留已解析的草稿', async () => {
    const { copilotCompleteStream } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotCompleteStream);
    mocked
      .mockImplementationOnce(async (_request, onDelta) => {
        const lines = [
          '{"type":"project","name":"未完成工程"}\n',
          '{"type":"create_node","ref":"timer","nodeType":"timer","label":"定时触发","config":{"interval_ms":3000}}\n',
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
          '{"type":"create_node","ref":"debug","nodeType":"debugConsole","label":"调试输出","config":{"label":"resume"}}\n',
          '{"type":"create_edge","fromRef":"timer","toRef":"debug"}\n',
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
    expect(result.draft.graph.nodes['timer_node_1']?.type).toBe('timer');
    expect(result.draft.graph.nodes['debug_console_1']?.type).toBe('debugConsole');
    expect(result.draft.graph.edges).toEqual([
      {
        from: 'timer_node_1',
        to: 'debug_console_1',
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
          '{"type":"create_node","ref":"timer","nodeType":"timer","label":"定时触发","config":{"interval_ms":1000}}\n',
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
          '{"type":"create_node","ref":"timer","nodeType":"timer","label":"定时触发","config":{"interval_ms":3000}}\n',
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
          '{"type":"create_node","ref":"debug","nodeType":"debugConsole","label":"调试输出","config":{"label":"resume"}}\n',
          '{"type":"create_edge","fromRef":"timer","toRef":"debug"}\n',
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
    expect(result.draft.graph.nodes['debug_console_1']?.type).toBe('debugConsole');
  });
});
