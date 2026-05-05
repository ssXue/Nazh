import { type NodeDefinition, type NodeSeed, type NodeValidation, type NodeValidationContext, isRecord } from '../shared';

function normalizeImplementation(value: unknown) {
  if (!isRecord(value)) {
    return {
      type: 'script',
      content: 'payload',
    };
  }

  return {
    ...value,
    type: typeof value.type === 'string' && value.type.trim() ? value.type : 'script',
  };
}

export const definition = {
  kind: 'capabilityCall' as const,
  catalog: { category: '硬件接口', description: '调用已审查 Capability DSL 的设备能力快照' },
  fallbackLabel: 'Capability Call',
  palette: { title: 'Capability Call', badge: 'Cap' },
  ai: {
    hint:
      '调用设备能力；config 必须含 capability_id, device_id, implementation, args。implementation 从 Capability DSL 快照转换而来。',
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'capability_call',
      kind: 'capabilityCall' as const,
      label: '',
      timeoutMs: 1000,
      config: {
        capability_id: '',
        device_id: '',
        implementation: {
          type: 'script',
          content: 'payload',
        },
        args: {},
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      capability_id:
        typeof rawConfig.capability_id === 'string' ? rawConfig.capability_id : '',
      device_id: typeof rawConfig.device_id === 'string' ? rawConfig.device_id : '',
      implementation: normalizeImplementation(rawConfig.implementation),
      args: isRecord(rawConfig.args) ? rawConfig.args : {},
    };
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },

  validate(ctx: NodeValidationContext): NodeValidation[] {
    const diagnostics: NodeValidation[] = [];
    if (!ctx.draft.capabilityId.trim()) {
      diagnostics.push({ tone: 'warning', message: '未绑定 capability_id。' });
    }
    if (!ctx.draft.capabilityDeviceId.trim()) {
      diagnostics.push({ tone: 'warning', message: '未绑定 device_id。' });
    }
    return diagnostics;
  },
} satisfies NodeDefinition;
