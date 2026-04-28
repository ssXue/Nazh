import {
  WorkflowNodeEntity,
  FlowNodeTransformData,
} from '@flowgram.ai/free-layout-editor';
import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

/** 桥接节点尺寸 */
export const LOOP_BRIDGE_SIZE = { width: 48, height: 48 };

/** 容器内部 padding（展开态） */
export const LOOP_PADDING = { top: 56, bottom: 32, left: 48, right: 48 };

/** iterate 桥接在内部坐标系的位置 */
export const LOOP_IN_POS = { x: 0, y: 0 };

/** emit 桥接在内部坐标系的位置 */
export const LOOP_OUT_POS = { x: 200, y: 0 };

function computeLoopContainerSize() {
  const contentWidth = Math.max(
    LOOP_IN_POS.x + LOOP_BRIDGE_SIZE.width,
    LOOP_OUT_POS.x + LOOP_BRIDGE_SIZE.width,
  );
  const contentHeight = Math.max(
    LOOP_IN_POS.y + LOOP_BRIDGE_SIZE.height,
    LOOP_OUT_POS.y + LOOP_BRIDGE_SIZE.height,
  );
  return {
    width: contentWidth + LOOP_PADDING.left + LOOP_PADDING.right,
    height: contentHeight + LOOP_PADDING.top + LOOP_PADDING.bottom,
  };
}

export const definition: NodeDefinition = {
  kind: 'loop',
  catalog: { category: '流程控制', description: '循环迭代与逐项分发' },
  fallbackLabel: 'Loop Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'loop_node',
      kind: 'loop',
      label: 'Loop',
      timeoutMs: 1000,
      config: { script: '[payload]' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('loop', config);
  },

  getNodeSize() {
    return computeLoopContainerSize();
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
        return { ...LOOP_PADDING };
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
};
