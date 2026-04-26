// 连接期校验的核心纯函数。
//
// FlowGram canAddLine 钩子的判断逻辑提到这里，方便单测——
// FlowgramCanvas 那一侧只是薄胶水（读 entity / 设 hasError）。
//
// 实现策略：
// 1. 从 pin schema 缓存查 (fromNodeId, fromPortId, output) 与
//    (toNodeId, toPortId, input) 两端的 PinDefinition
// 2. 若任一端缓存未命中（schema 还没拉到 / 节点没注册）→ 放行
//    （部署期 pin_validator 作为 backstop 兜底）
// 3. 两端都命中时调 isCompatibleWith → 返回 boolean

import type { PinType } from '../types';

import { findPin, formatPinType } from './pin-schema-cache';
import { isCompatibleWith } from './pin-compat';

export interface ConnectionRejection {
  kind: 'incompatible-types';
  fromNodeId: string;
  fromPortId: string;
  toNodeId: string;
  toPortId: string;
  fromType: PinType;
  toType: PinType;
}

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
 */
export function formatRejection(rejection: ConnectionRejection): string {
  const { fromNodeId, fromPortId, toNodeId, toPortId, fromType, toType } = rejection;
  return `连接不兼容：${fromNodeId}.${fromPortId} (${formatPinType(fromType)}) → ${toNodeId}.${toPortId} (${formatPinType(toType)})`;
}
