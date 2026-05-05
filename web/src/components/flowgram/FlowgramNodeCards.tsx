/**
 * Flowgram 节点卡片渲染组件。
 *
 * 包含 FlowgramContainerCard（子图/循环容器）和 FlowgramNodeCard（普通业务节点）
 * 两个子组件，从 FlowgramCanvas.tsx 拆出以降低单文件复杂度。
 */

import { FlowNodeEntity, WorkflowNodeRenderer } from '@flowgram.ai/free-layout-editor';
import { SubCanvasRender } from '@flowgram.ai/free-container-plugin';
import { useLayoutEffect, useMemo } from 'react';

import { FlowgramNodeGlyph, getFlowgramDisplayLabel, normalizeFlowgramDisplayType } from './FlowgramNodeGlyph';
import {
  getLogicNodeBranchDefinitions,
  normalizeNodeKind,
  resolveNodeDisplayLabel,
} from './flowgram-node-library';
import { isPureForm } from '../../lib/pin-compat';
import {
  getPortTooltip,
  getNodePinSchema,
  resolvePinKind,
  resolvePinTypeKind,
} from '../../lib/pin-schema-cache';
import { getCachedCapabilities } from '../../lib/node-capabilities-cache';
import { hasCapability } from '../../lib/node-capabilities';
import { resolveNodePortColor } from './flowgram-canvas-utils';

export type RuntimeNodeStatus = 'idle' | 'running' | 'completed' | 'failed' | 'output';

export interface FlowgramNodeMaterialProps {
  node: FlowNodeEntity;
  activated?: boolean;
  runtimeStatus?: RuntimeNodeStatus;
  accentHex: string;
  nodeCodeColor: string;
}

/** 容器节点渲染：标题栏 + SubCanvasRender 内嵌画布区域。 */
const SUBCANVAS_HEADER_OFFSET = -48;

export function FlowgramContainerCard(props: FlowgramNodeMaterialProps) {
  const rawData = props.node.getExtInfo() as
    | { label?: string; nodeType?: string; config?: { script?: string } }
    | undefined;
  const rawNodeType = rawData?.nodeType ?? props.node.flowNodeType;
  const nodeType = normalizeNodeKind(rawNodeType);
  const displayType = normalizeFlowgramDisplayType(nodeType);
  const runtimeStatus = props.runtimeStatus ?? 'idle';
  const containerClass = nodeType === 'loop' ? 'loop' : 'subgraph';
  const displayLabel = resolveNodeDisplayLabel(rawNodeType, rawData?.label);

  return (
    <WorkflowNodeRenderer
      node={props.node}
      className={`flowgram-card flowgram-card--${containerClass} flowgram-card--${runtimeStatus} ${props.activated ? 'is-activated' : ''}`}
      portClassName="flowgram-card__port"
      portBackgroundColor="var(--panel-strong)"
      portPrimaryColor="var(--accent)"
      portSecondaryColor="var(--surface-elevated)"
      portErrorColor="var(--danger)"
    >
      <div className="flowgram-subgraph__header">
        <div className="flowgram-subgraph__header-left">
          <span className={`flowgram-card__icon flowgram-card__icon--${displayType}`}>
            <FlowgramNodeGlyph displayType={displayType} width={14} height={14} />
          </span>
          <strong>{displayLabel}</strong>
        </div>
        {runtimeStatus !== 'idle' ? (
          <span className={`flowgram-card__runtime flowgram-card__runtime--${runtimeStatus}`}>
            {runtimeStatus}
          </span>
        ) : null}
      </div>
      <SubCanvasRender offsetY={SUBCANVAS_HEADER_OFFSET} />
    </WorkflowNodeRenderer>
  );
}

export function FlowgramNodeCard(props: FlowgramNodeMaterialProps) {
  const rawData = props.node.getExtInfo() as
    | {
        label?: string;
        nodeType?: string;
        displayType?: string;
        connectionId?: string | null;
        timeoutMs?: number | null;
        config?: {
          message?: string;
          script?: string;
          branches?: Array<{
            key?: string;
            label?: string;
          }>;
          interval_ms?: number;
          register?: number;
          quantity?: number;
          url?: string;
          method?: string;
          webhook_kind?: string;
          body_mode?: string;
          device_key?: string;
          group?: string;
          level?: string;
          table?: string;
          database_path?: string;
          label?: string;
        };
      }
    | undefined;

  const rawNodeType = rawData?.nodeType ?? props.node.flowNodeType;
  const nodeType = normalizeNodeKind(rawNodeType);
  const displayType = normalizeFlowgramDisplayType(rawData?.displayType ?? nodeType);
  const runtimeStatus = props.runtimeStatus ?? 'idle';
  const branchDefinitions = useMemo(
    () => getLogicNodeBranchDefinitions(nodeType, rawData?.config),
    [nodeType, rawData?.config],
  );
  const branchSignature = branchDefinitions
    .map((branch) => `${branch.key}:${branch.label}`)
    .join('|');

  useLayoutEffect(() => {
    if (branchDefinitions.length === 0) {
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      props.node.ports.updateDynamicPorts();
    });

    return () => window.cancelAnimationFrame(frame);
  }, [branchDefinitions.length, branchSignature, props.node]);

  const preview =
    nodeType === 'timer'
      ? `${rawData?.config?.interval_ms ?? 5000} ms`
      : nodeType === 'serialTrigger'
        ? rawData?.connectionId
          ? `串口连接 · ${rawData.connectionId}`
          : '未绑定串口连接'
      : nodeType === 'modbusRead'
        ? `寄存器 ${rawData?.config?.register ?? 40001} · ${rawData?.config?.quantity ?? 1} 点`
      : nodeType === 'native'
      ? rawData?.config?.message ?? 'Native I/O passthrough'
      : nodeType === 'code'
        ? rawData?.config?.script ?? 'Transform payload'
        : nodeType === 'if'
          ? rawData?.config?.script ?? 'return boolean'
          : nodeType === 'switch'
            ? rawData?.config?.script ?? 'return branch key'
            : nodeType === 'loop'
              ? rawData?.config?.script ?? 'return array or count'
              : nodeType === 'httpClient'
                ? rawData?.connectionId
                  ? `Connection Studio · ${rawData.connectionId}`
                  : '未绑定 HTTP 连接'
                : nodeType === 'barkPush'
                  ? rawData?.connectionId
                    ? `Connection Studio · ${rawData.connectionId}`
                    : '未绑定 Bark 连接'
                  : nodeType === 'canRead'
                    ? rawData?.connectionId
                      ? `CAN · ${rawData.connectionId}`
                      : '未绑定 CAN 连接'
                    : nodeType === 'canWrite'
                      ? rawData?.connectionId
                        ? `CAN · ${rawData.connectionId}`
                        : '未绑定 CAN 连接'
                      : nodeType === 'sqlWriter'
                        ? `${rawData?.config?.table ?? 'workflow_logs'} → ${rawData?.config?.database_path ?? './nazh-local.sqlite3'}`
                        : nodeType === 'debugConsole'
                      ? rawData?.config?.label ?? 'Console output'
                      : nodeType === 'subgraphInput'
                        ? '输入桥接'
                        : nodeType === 'subgraphOutput'
                          ? '输出桥接'
                          : rawData?.config?.script ?? 'Guarded script';

  // 桥接节点（ADR-0013）：方形 icon 卡片 — 竖线 + 圆点
  if (nodeType === 'subgraphInput' || nodeType === 'subgraphOutput') {
    const isInput = nodeType === 'subgraphInput';
    return (
      <WorkflowNodeRenderer
        node={props.node}
        className={`flowgram-card flowgram-card--bridge flowgram-card--${nodeType} flowgram-card--${runtimeStatus}`}
        portClassName="flowgram-card__port"
        portBackgroundColor="var(--panel-strong)"
        portPrimaryColor="var(--accent)"
        portSecondaryColor="var(--surface-elevated)"
        portErrorColor="var(--danger)"
      >
        <div data-flow-editor-selectable="false" className="flowgram-bridge-icon" draggable={false}>
          <svg
            width="20"
            height="20"
            viewBox="0 0 20 20"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            aria-hidden="true"
          >
            {isInput ? (
              <>
                <line x1="14" y1="4" x2="14" y2="16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                <circle cx="6" cy="10" r="3" fill="currentColor" />
              </>
            ) : (
              <>
                <line x1="6" y1="4" x2="6" y2="16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                <circle cx="14" cy="10" r="3" fill="currentColor" />
              </>
            )}
          </svg>
        </div>
        <span className="sr-only">{preview}</span>
      </WorkflowNodeRenderer>
    );
  }

  const pinSchema = getNodePinSchema(props.node.id);
  const pureForm = pinSchema
    ? isPureForm(pinSchema.inputPins, pinSchema.outputPins)
    : false;

  const capabilityBits = getCachedCapabilities(nodeType);
  let nodeCapability: string;
  if (pureForm) {
    nodeCapability = 'pure'; // pure 优先级最高，但 CSS 已由 --pure-form 覆盖，此处仅做标记
  } else if (capabilityBits !== undefined && hasCapability(capabilityBits, 'TRIGGER')) {
    nodeCapability = 'trigger';
  } else if (capabilityBits !== undefined && hasCapability(capabilityBits, 'BRANCHING')) {
    nodeCapability = 'branching';
  } else {
    nodeCapability = 'default';
  }

  return (
    <WorkflowNodeRenderer
      node={props.node}
      className={`flowgram-card flowgram-card--${nodeType} flowgram-card--display-${displayType} flowgram-card--${runtimeStatus} ${props.activated ? 'is-activated' : ''} ${pureForm ? 'flowgram-card--pure-form' : ''}`}
      portClassName="flowgram-card__port"
      portBackgroundColor="var(--panel-strong)"
      portPrimaryColor={resolveNodePortColor(displayType, props.accentHex, props.nodeCodeColor)}
      portSecondaryColor="var(--surface-elevated)"
      portErrorColor="var(--danger)"
    >
      <div data-flow-editor-selectable="false" className="flowgram-card__body" draggable={false} data-pure-form={pureForm ? 'true' : undefined} data-node-capability={pureForm ? undefined : nodeCapability}>
        <div className="flowgram-card__topline">
          <div className="flowgram-card__identity">
            <span className={`flowgram-card__icon flowgram-card__icon--${displayType}`}>
              <FlowgramNodeGlyph displayType={displayType} width={14} height={14} />
            </span>
            <span className="flowgram-card__type">{getFlowgramDisplayLabel(displayType)}</span>
          </div>
          {runtimeStatus !== 'idle' ? (
            <span className={`flowgram-card__runtime flowgram-card__runtime--${runtimeStatus}`}>
              {runtimeStatus}
            </span>
          ) : null}
        </div>
        <strong>{resolveNodeDisplayLabel(rawNodeType, rawData?.label)}</strong>
        <p className="flowgram-card__preview">{preview}</p>
        {branchDefinitions.length > 0 ? (
          <div className="flowgram-card__branches">
            {branchDefinitions.map((branch) => (
              <div key={branch.key} className="flowgram-card__branch-row">
                <span className="flowgram-card__branch-label">{branch.label}</span>
                <span
                  className="flowgram-card__branch-port"
                  data-port-id={branch.key}
                  data-port-type="output"
                  data-port-location="right"
                  data-port-pin-type={resolvePinTypeKind(props.node.id, branch.key, 'output')}
                  data-port-pin-kind={resolvePinKind(props.node.id, branch.key, 'output')}
                  title={getPortTooltip(props.node.id, branch.key, 'output')}
                />
              </div>
            ))}
          </div>
        ) : null}
        <div className="flowgram-card__meta">
          <span>{rawData?.connectionId ? `conn: ${rawData.connectionId}` : 'logic-only node'}</span>
          <span>{rawData?.timeoutMs ? `${rawData.timeoutMs} ms timeout` : 'no timeout'}</span>
        </div>
      </div>
    </WorkflowNodeRenderer>
  );
}
