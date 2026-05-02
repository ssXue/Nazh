import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  isRecord,
} from '../shared';

const HUMAN_LOOP_BRANCHES = [
  { key: 'approve', label: 'Approve', fixed: true },
  { key: 'reject', label: 'Reject', fixed: true },
];

export const definition = {
  kind: 'humanLoop' as const,
  catalog: { category: '流程控制', description: '暂停工作流等待人工审批响应' },
  fallbackLabel: '审批节点',
  palette: { title: '审批节点', badge: '审批' },
  ai: {
    hint:
      '人工审批节点；config 可含 title, description, form_schema, approval_timeout_ms, default_action；下游边 sourcePortId 只能是 approve / reject。',
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'human_loop',
      kind: 'humanLoop' as const,
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
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      title: typeof rawConfig.title === 'string' ? rawConfig.title : '',
      description: typeof rawConfig.description === 'string' ? rawConfig.description : '',
      form_schema: Array.isArray(rawConfig.form_schema) ? rawConfig.form_schema : [],
      approval_timeout_ms:
        typeof rawConfig.approval_timeout_ms === 'number' && Number.isFinite(rawConfig.approval_timeout_ms)
          ? Math.max(0, Math.round(rawConfig.approval_timeout_ms))
          : null,
      default_action:
        rawConfig.default_action === 'autoApprove' || rawConfig.default_action === 'autoReject'
          ? rawConfig.default_action
          : 'autoReject',
    };
  },

  getOutputPorts() {
    return HUMAN_LOOP_BRANCHES;
  },

  getRoutingBranches() {
    return HUMAN_LOOP_BRANCHES;
  },

  getNodeSize() {
    return { width: 214, height: 132 };
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
