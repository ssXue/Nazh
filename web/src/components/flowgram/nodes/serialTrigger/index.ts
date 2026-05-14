import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'serialTrigger' as const,
  catalog: { category: '硬件接口', description: '【调试/适配器】接收串口原始帧；业务编排优先用"设备能力"分组' },
  fallbackLabel: 'Serial Trigger',
  palette: { title: 'Serial Trigger', badge: 'Serial' },
  ai: { hint: '串口触发；通常不填写 connectionId，等待用户后续绑定。' },
  requiresConnection: true,

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'serial_trigger',
      kind: 'serialTrigger' as const,
      label: '',
      timeoutMs: null,
      config: { inject: {} },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      inject: isRecord(rawConfig.inject) ? rawConfig.inject : {},
    };
  },

  getOutputPorts() {
    return [{ key: 'out', label: 'out' }];
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    // 触发器节点：输出端口受控渲染以匹配 Rust 端 pin id。
    // 保留默认 input port——FlowGram 在无边时仍需要 input port 实体。
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
      useDynamicPort: true,
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
