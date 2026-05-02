import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'humanLoop',
  catalog: { category: '流程控制', description: '暂停工作流等待人工审批响应' },
  fallbackLabel: '审批节点',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'human_loop',
      kind: 'humanLoop',
      label: '',
      timeoutMs: null,
      config: {
        title: '',
        description: '',
        form_schema: [],
        approval_timeout_ms: null,
        default_action: 'autoReject',
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('humanLoop', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
