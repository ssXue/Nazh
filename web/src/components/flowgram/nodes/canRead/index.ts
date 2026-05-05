import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';
import { parseNonNegativeInteger, parsePositiveInteger } from '../settings-shared';

export const definition = {
  kind: 'canRead' as const,
  catalog: { category: '硬件接口', description: '通过 SLCAN 接收 CAN 帧' },
  fallbackLabel: 'CAN Read',
  palette: { title: 'CAN Read', badge: 'CAN' },
  ai: {
    hint:
      'CAN 读取节点；config 可含 can_id, is_extended, timeout_ms；需要绑定 can / can-slcan / slcan 连接。',
  },
  requiresConnection: true,

  fieldValidators: {
    canId: v => v.trim() && parseNonNegativeInteger(v) === null ? 'CAN ID 必须是大于等于 0 的整数。' : null,
    canReadTimeoutMs: v => parsePositiveInteger(v) === null ? 'CAN 接收超时必须是大于 0 的整数。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'can_read',
      kind: 'canRead' as const,
      label: '',
      timeoutMs: 1000,
      config: {
        can_id: null,
        is_extended: false,
        timeout_ms: 1000,
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      can_id:
        typeof rawConfig.can_id === 'number' && Number.isFinite(rawConfig.can_id)
          ? Math.max(0, Math.round(rawConfig.can_id))
          : null,
      is_extended:
        typeof rawConfig.is_extended === 'boolean' ? rawConfig.is_extended : false,
      timeout_ms:
        typeof rawConfig.timeout_ms === 'number' && Number.isFinite(rawConfig.timeout_ms)
          ? Math.max(1, Math.round(rawConfig.timeout_ms))
          : 1000,
    };
  },

  getOutputPorts() {
    return [
      { key: 'out', label: 'out' },
    ];
  },

  getNodeSize() {
    return { width: 200, height: 100 };
  },

  buildRegistryMeta() {
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
