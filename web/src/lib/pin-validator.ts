// 连接期校验的核心纯函数。
//
// FlowGram canAddLine 钩子的判断逻辑提到这里，方便单测——
// FlowgramCanvas 那一侧只是薄胶水（读 entity / 设 hasError）。
//
// 实现策略（ADR-0014 Phase 2 之后）：
// 1. 从 pin schema 缓存查 (fromNodeId, fromPortId, output) 与
//    (toNodeId, toPortId, input) 两端的 PinDefinition
// 2. 若任一端缓存未命中（schema 还没拉到 / 节点没注册）→ 放行
//    （部署期 pin_validator 作为 backstop 兜底）
// 3. **PinKind 闸门优先**：跨 Kind（Exec ↔ Data）是结构性不兼容，
//    无须再看 PinType——直接拒。详见 ADR-0014。
// 4. Kind 一致后再调 isCompatibleWith → 返回 boolean

import type { PinKind, PinType } from '../types';

import { findPin, formatPinType } from './pin-schema-cache';
import { isCompatibleWith, isKindCompatible } from './pin-compat';

/**
 * 连接被拒的原因，判别联合：
 * - `incompatible-kinds`：两端 PinKind 不一致（Exec ↔ Data）。结构性问题，
 *   无论 PinType 是否匹配都拒。ADR-0014 引入。
 * - `incompatible-types`：PinKind 一致但 PinType 不兼容（如 Bool → Json）。
 *   走既有 `isCompatibleWith` 兼容矩阵。
 */
export type ConnectionRejection =
  | {
      kind: 'incompatible-types';
      fromNodeId: string;
      fromPortId: string;
      toNodeId: string;
      toPortId: string;
      fromType: PinType;
      toType: PinType;
    }
  | {
      kind: 'incompatible-kinds';
      fromNodeId: string;
      fromPortId: string;
      toNodeId: string;
      toPortId: string;
      fromKind: PinKind;
      toKind: PinKind;
    };

export interface ConnectionCheckResult {
  /** 允许连接（true = 通过校验或缓存未命中放行）。 */
  allow: boolean;
  /** 不允许时的拒收原因；`allow=true` 时为 null。 */
  rejection: ConnectionRejection | null;
}

const ALLOW: ConnectionCheckResult = { allow: true, rejection: null };

/**
 * 判断"`from` 节点的输出端口 → `to` 节点的输入端口"是否可连。
 *
 * 缓存未命中时**放行**——`describe_node_pins` IPC 还没拉到不是错误，
 * 让用户暂时能拖边，部署期 backstop 兜底。
 *
 * 校验顺序（ADR-0014 Phase 2 起）：
 * 1. 缓存命中检查
 * 2. **PinKind 闸门**——结构性，先报
 * 3. PinType 兼容矩阵——形态匹配，后报
 *
 * Kind 比 Type 更"结构"——Exec / Data 两路求值语义根本不同，跨 Kind
 * 连边即使 PinType 完全相同也是错的；优先报 Kind 错给用户最直接的反馈。
 */
export function checkConnection(
  fromNodeId: string,
  fromPortId: string | number,
  toNodeId: string,
  toPortId: string | number,
): ConnectionCheckResult {
  const fromPin = findPin(fromNodeId, fromPortId, 'output');
  const toPin = findPin(toNodeId, toPortId, 'input');

  if (!fromPin || !toPin) {
    return ALLOW;
  }

  // 1) PinKind 闸门：跨 Kind（Exec ↔ Data）是结构性不兼容，无须再看 PinType
  if (!isKindCompatible(fromPin.kind, toPin.kind)) {
    return {
      allow: false,
      rejection: {
        kind: 'incompatible-kinds',
        fromNodeId,
        fromPortId: String(fromPortId),
        toNodeId,
        toPortId: String(toPortId),
        fromKind: fromPin.kind,
        toKind: toPin.kind,
      },
    };
  }

  // 2) Kind 一致后再校验 PinType（沿用既有逻辑）
  if (isCompatibleWith(fromPin.pin_type, toPin.pin_type)) {
    return ALLOW;
  }

  return {
    allow: false,
    rejection: {
      kind: 'incompatible-types',
      fromNodeId,
      fromPortId: String(fromPortId),
      toNodeId,
      toPortId: String(toPortId),
      fromType: fromPin.pin_type,
      toType: toPin.pin_type,
    },
  };
}

/**
 * 把拒收原因格式化成给 console.warn / toast 用的中文短消息。
 *
 * 用 `formatPinType` 渲染类型，保留 `array<json>` / `custom(name)` 的
 * 完整信息——直接打印 `pinType.kind` 会丢内层类型 / 自定义名。
 *
 * `incompatible-kinds` 文案显式提"求值语义不同"——避免用户误以为只是
 * 类型问题（改类型也修不好，必须换连法）。
 */
export function formatRejection(rejection: ConnectionRejection): string {
  if (rejection.kind === 'incompatible-kinds') {
    return `连接不兼容（求值语义不同）：${rejection.fromNodeId}.${rejection.fromPortId} (${rejection.fromKind}) → ${rejection.toNodeId}.${rejection.toPortId} (${rejection.toKind})。Exec 引脚只能连 Exec，Data 引脚只能连 Data。`;
  }
  const { fromNodeId, fromPortId, toNodeId, toPortId, fromType, toType } = rejection;
  return `连接不兼容：${fromNodeId}.${fromPortId} (${formatPinType(fromType)}) → ${toNodeId}.${toPortId} (${formatPinType(toType)})`;
}
