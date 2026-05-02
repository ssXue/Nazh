import {
  WorkflowNodeEntity,
  FlowNodeTransformData,
} from '@flowgram.ai/free-layout-editor';
import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  isRecord,
} from '../shared';

/** 桥接节点尺寸 */
export const BRIDGE_SIZE = { width: 48, height: 48 };

/** 容器内部 padding（展开态） */
export const CONTAINER_PADDING = { top: 56, bottom: 32, left: 48, right: 48 };

/** sg-in 在内部坐标系的位置 */
export const SG_IN_POS = { x: 0, y: 0 };

/** sg-out 在内部坐标系的位置 */
export const SG_OUT_POS = { x: 200, y: 0 };

function computeContainerSize() {
  const contentWidth = Math.max(
    SG_IN_POS.x + BRIDGE_SIZE.width,
    SG_OUT_POS.x + BRIDGE_SIZE.width,
  );
  const contentHeight = Math.max(
    SG_IN_POS.y + BRIDGE_SIZE.height,
    SG_OUT_POS.y + BRIDGE_SIZE.height,
  );
  return {
    width: contentWidth + CONTAINER_PADDING.left + CONTAINER_PADDING.right,
    height: contentHeight + CONTAINER_PADDING.top + CONTAINER_PADDING.bottom,
  };
}

export const definition = {
  kind: 'subgraph' as const,
  catalog: {
    category: '子图封装',
    description: '封装子拓扑为单节点，支持嵌套和参数化',
  },
  fallbackLabel: 'Subgraph',
  palette: { title: 'Subgraph', badge: 'Subgraph' },
  ai: { editorOnly: true, hint: '编辑器容器节点，不应作为运行时节点输出到工作流。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'subgraph',
      kind: 'subgraph' as const,
      label: '',
      timeoutMs: null,
      config: {
        parameterBindings: {},
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      parameterBindings: isRecord(rawConfig.parameterBindings)
        ? rawConfig.parameterBindings
        : {},
    };
  },

  getNodeSize() {
    return computeContainerSize();
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      isContainer: true,
      size: this.getNodeSize(),
      padding(transform: unknown) {
        const t = transform as FlowNodeTransformData;
        if (!t.isContainer) {
          return { top: 0, bottom: 0, left: 0, right: 0 };
        }
        return { ...CONTAINER_PADDING };
      },
      selectable(node: unknown, mousePos?: unknown) {
        if (!mousePos) return true;
        const n = node as WorkflowNodeEntity;
        const m = mousePos as { x: number; y: number };
        const transform = n.getData<FlowNodeTransformData>(FlowNodeTransformData);
        return !transform.bounds.contains(m.x, m.y);
      },
      wrapperStyle: {
        minWidth: 'unset',
        width: '100%',
      },
      defaultPorts: [{ type: 'input' as const }, { type: 'output' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
