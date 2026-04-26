// ADR-0010 Phase 2：连接期校验的核心纯函数。
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
//
// 失败原因（reason）走结构化 enum——便于上层分类记录 / 多语化提示。

import type { PinType } from '../types';

import { findPin } from './pin-schema-cache';
import { isCompatibleWith } from './pin-compat';

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
      kind: 'unknown-pin';
      nodeId: string;
      portId: string;
      direction: 'input' | 'output';
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
 */
export function checkConnection(
  fromNodeId: string,
  fromPortId: string,
  toNodeId: string,
  toPortId: string,
): ConnectionCheckResult {
  const fromPin = findPin(fromNodeId, fromPortId, 'output');
  const toPin = findPin(toNodeId, toPortId, 'input');

  // 缓存未命中——放行（fallback Any/Any 已在 cache 层处理；这里多一道保险）
  if (!fromPin) {
    return ALLOW;
  }
  if (!toPin) {
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
      fromPortId,
      toNodeId,
      toPortId,
      fromType: fromPin.pin_type,
      toType: toPin.pin_type,
    },
  };
}

/** 把拒收原因格式化成给 console.warn / toast 用的中文短消息。 */
export function formatRejection(rejection: ConnectionRejection): string {
  switch (rejection.kind) {
    case 'incompatible-types':
      return `连接不兼容：${rejection.fromNodeId}.${rejection.fromPortId} (${rejection.fromType.kind}) → ${rejection.toNodeId}.${rejection.toPortId} (${rejection.toType.kind})`;
    case 'unknown-pin':
      return `未知端口：${rejection.nodeId}.${rejection.portId}（${rejection.direction === 'input' ? '输入' : '输出'}）`;
  }
}
