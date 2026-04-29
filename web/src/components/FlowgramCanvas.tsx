import {
  EditorRenderer,
  FreeLayoutEditorProvider,
  FlowNodeBaseType,
  FlowNodeEntity,
  WorkflowContentChangeType,
  WorkflowLineEntity,
  type WorkflowLinesManager,
  type WorkflowJSON as FlowgramWorkflowJSON,
  type WorkflowContentChangeEvent,
  WorkflowNodeRenderer,
  type InteractiveType as EditorInteractiveType,
  useClientContext,
  usePlaygroundTools,
  useService,
  type FreeLayoutPluginContext,
} from '@flowgram.ai/free-layout-editor';
import { PanelManager } from '@flowgram.ai/panel-manager-plugin';
import {
  type CSSProperties,
  type ReactNode,
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { MinimapRender } from '@flowgram.ai/minimap-plugin';
import { SubCanvasRender } from '@flowgram.ai/free-container-plugin';

import {
  AutoLayoutIcon,
  DownloadIcon,
  FileImageIcon,
  FileJsonIcon,
  FileVectorIcon,
  FitViewIcon,
  LockClosedIcon,
  LockOpenIcon,
  MinimapIcon,
  MouseModeIcon,
  RunActionIcon,
  RedoActionIcon,
  StopActionIcon,
  TriggerActionIcon,
  UndoActionIcon,
  TrackpadModeIcon,
} from './app/AppIcons';
import { FlowgramNodeAddPanel } from './flowgram/FlowgramNodeAddPanel';
import type { ThemeMode } from './app/types';
import { FLOWGRAM_NODE_SETTINGS_PANEL_KEY } from './flowgram/FlowgramNodeSettingsPanel';
import {
  FlowgramNodeGlyph,
  getFlowgramDisplayLabel,
  normalizeFlowgramDisplayType,
} from './flowgram/FlowgramNodeGlyph';
import {
  getLogicNodeBranchDefinitions,
  normalizeNodeKind,
  resolveNodeData,
  resolveNodeDisplayLabel,
  type FlowgramConnectionDefaults,
  type NodeSeed,
} from './flowgram/flowgram-node-library';
import { handleFlowgramDragLineEnd } from './flowgram/flowgram-line-panel';
import { useFlowgramEditorProps } from './flowgram/useFlowgramEditorProps';
import {
  formatWorkflowGraph,
  toFlowgramWorkflowJson,
  toNazhWorkflowGraph,
} from '../lib/flowgram';
import { FlowDownloadFormat, FlowDownloadService } from '@flowgram.ai/export-plugin';
import {
  configToRecord,
  getPortTooltip,
  getNodePinSchema,
  invalidateNodePinSchema,
  refreshNodePinSchema,
  resolvePinKind,
  resolvePinTypeKind,
} from '../lib/pin-schema-cache';
import { isPureForm } from '../lib/pin-compat';
import {
  type ConnectionRejection,
  checkConnection,
  formatRejection,
} from '../lib/pin-validator';
import { hasTauriRuntime, saveFlowgramExportFile } from '../lib/tauri';
import { refreshCapabilitiesCache } from '../lib/node-capabilities-cache';
import type {
  AiGenerationParams,
  AiProviderView,
  ConnectionDefinition,
  WorkflowGraph,
  WorkflowRuntimeState,
  WorkflowWindowStatus,
} from '../types';

export interface FlowgramCanvasResources {
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
}

export interface FlowgramCanvasRuntime {
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  canTestRun?: boolean;
}

export interface FlowgramCanvasAppearance {
  accentHex: string;
  themeMode: ThemeMode;
  nodeCodeColor: string;
}

export interface FlowgramCanvasExportTarget {
  workspacePath?: string;
  workflowName?: string | null;
}

export interface FlowgramCanvasActions {
  onRunRequested?: () => void;
  onStopRequested?: () => void;
  onTestRunRequested?: () => void;
  onGraphChange: (nextAstText: string) => void;
  onError?: (title: string, detail?: string | null) => void;
  onStatusMessage?: (message: string) => void;
}

interface FlowgramCanvasProps {
  graph: WorkflowGraph | null;
  resources: FlowgramCanvasResources;
  runtime: FlowgramCanvasRuntime;
  appearance: FlowgramCanvasAppearance;
  exportTarget?: FlowgramCanvasExportTarget;
  actions: FlowgramCanvasActions;
}

export interface FlowgramCanvasHandle {
  isReady: () => boolean;
  getCurrentWorkflowGraph: () => WorkflowGraph | null;
  loadWorkflowGraph: (graph: WorkflowGraph) => void;
}

interface FlowgramNodeMaterialProps {
  node: FlowNodeEntity;
  activated?: boolean;
  runtimeStatus?: RuntimeNodeStatus;
  accentHex: string;
  nodeCodeColor: string;
}

type RuntimeNodeStatus = 'idle' | 'running' | 'completed' | 'failed' | 'output';

type FlowgramInteractiveType = 'MOUSE' | 'PAD';

interface FlowgramToolbarProps {
  canRun: boolean;
  canTestRun: boolean;
  isWorkflowActive: boolean;
  minimapVisible: boolean;
  onToggleMinimap: () => void;
  onRun?: () => void;
  onStop?: () => void;
  onTestRun?: () => void;
  onDownload: (format: FlowDownloadFormat) => void | Promise<void>;
}

interface InternalFlowExportImageService {
  export: (options: { format: FlowDownloadFormat; watermarkSVG?: string }) => Promise<string | undefined>;
}

interface InternalFlowDownloadService {
  download: (params: { format: FlowDownloadFormat }) => Promise<void>;
  document: {
    toJSON: () => unknown;
  };
  exportImageService: InternalFlowExportImageService;
  options?: {
    watermarkSVG?: string;
  };
  formatDataContent: (
    json: unknown,
    format: FlowDownloadFormat,
  ) => Promise<{
    content: string;
    mimeType: string;
  }>;
  setDownloading: (value: boolean) => void;
}

function normalizedConnectionType(connectionType: string): string {
  return connectionType.trim().toLowerCase();
}

function isSerialConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'serial':
    case 'serialport':
    case 'serial_port':
    case 'uart':
    case 'rs232':
    case 'rs485':
      return true;
    default:
      return false;
  }
}

function isModbusConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'modbus':
    case 'modbus_tcp':
      return true;
    default:
      return false;
  }
}

function isMqttConnectionType(connectionType: string): boolean {
  return normalizedConnectionType(connectionType) === 'mqtt';
}

function isHttpConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'http':
    case 'http_sink':
      return true;
    default:
      return false;
  }
}

function isBarkConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'bark':
    case 'bark_push':
      return true;
    default:
      return false;
  }
}

function sanitizeExportFileSegment(value: string): string {
  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/[\\/:*?"<>|]+/g, '-')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-+|-+$/g, '');

  return normalized || 'flowgram';
}

function buildFlowgramExportFileName(
  workflowName: string | null | undefined,
  format: FlowDownloadFormat,
): string {
  const baseName = sanitizeExportFileSegment(workflowName ?? 'flowgram-workflow');
  const timestamp = new Date()
    .toISOString()
    .replace(/[:]/g, '-')
    .replace(/\.\d{3}Z$/, 'Z');

  return `${baseName}-${timestamp}.${format}`;
}

const FLOWGRAM_BUTTON_STYLE: CSSProperties = {
  border: '0',
  borderRadius: '8px',
  cursor: 'pointer',
  padding: '0',
  minHeight: 32,
  height: 32,
  minWidth: 32,
  lineHeight: 1,
  fontSize: 'var(--font-callout)',
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  whiteSpace: 'nowrap',
  color: 'var(--toolbar-text)',
  background: 'transparent',
  boxShadow: 'none',
  transform: 'none',
  transition: 'background 160ms ease, color 160ms ease, opacity 160ms ease',
};

const FLOWGRAM_TOOLS_STYLE: CSSProperties = {
  position: 'absolute',
  zIndex: 20,
  left: '50%',
  transform: 'translateX(-50%)',
  bottom: 16,
  display: 'flex',
  alignItems: 'center',
  gap: 8,
  maxWidth: 'calc(100% - 48px)',
  pointerEvents: 'none',
};

const FLOWGRAM_TOOLS_SECTION_STYLE: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 2,
  minHeight: 40,
  padding: '0 4px',
  border: '1px solid var(--toolbar-border)',
  borderRadius: 10,
  background: 'var(--panel-strong)',
  boxShadow: 'var(--shadow-low)',
  backdropFilter: 'blur(16px)',
  pointerEvents: 'auto',
};

const FLOWGRAM_ZOOM_STYLE: CSSProperties = {
  cursor: 'default',
  minWidth: 42,
  height: 24,
  minHeight: 24,
  padding: '0 6px',
  borderRadius: 8,
  border: '1px solid var(--toolbar-border)',
  background: 'var(--surface-muted)',
  color: 'var(--toolbar-text)',
  fontSize: 'var(--font-subheadline)',
};

const FLOWGRAM_MINIMAP_CANVAS_WIDTH = 110;
const FLOWGRAM_MINIMAP_CANVAS_HEIGHT = 76;
const FLOWGRAM_MINIMAP_PANEL_PADDING = 4;

const FLOWGRAM_MINIMAP_CONTAINER_STYLE: CSSProperties = {
  pointerEvents: 'auto',
  position: 'relative',
  top: 'unset',
  right: 'unset',
  bottom: 'unset',
  left: 'unset',
};

const FLOWGRAM_MINIMAP_PANEL_STYLE: CSSProperties = {
  width: 118,
  height: FLOWGRAM_MINIMAP_CANVAS_HEIGHT + FLOWGRAM_MINIMAP_PANEL_PADDING * 2,
  padding: FLOWGRAM_MINIMAP_PANEL_PADDING,
  boxSizing: 'border-box',
};

const FLOWGRAM_MINIMAP_INACTIVE_STYLE = {
  opacity: 1,
  scale: 1,
  translateX: 0,
  translateY: 0,
} as const;

const FLOWGRAM_INTERACTIVE_CACHE_KEY = 'workflow_prefer_interactive_type';

function isMacLikePlatform() {
  if (typeof navigator === 'undefined') {
    return false;
  }

  return /(Macintosh|MacIntel|MacPPC|Mac68K|iPad)/.test(navigator.userAgent);
}

function getPreferredInteractiveType(): FlowgramInteractiveType {
  if (typeof window === 'undefined') {
    return isMacLikePlatform() ? 'PAD' : 'MOUSE';
  }

  try {
    const stored = window.localStorage.getItem(FLOWGRAM_INTERACTIVE_CACHE_KEY);
    if (stored === 'MOUSE' || stored === 'PAD') {
      return stored;
    }
  } catch {
    // Ignore storage failures and fall back to platform defaults.
  }

  return isMacLikePlatform() ? 'PAD' : 'MOUSE';
}

function setPreferredInteractiveType(nextType: FlowgramInteractiveType) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(FLOWGRAM_INTERACTIVE_CACHE_KEY, nextType);
  } catch {
    // Ignore storage failures.
  }
}

function getCanvasWorkflowStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'preview':
      return '预览';
    case 'idle':
      return '未运行';
    case 'deployed':
      return '已部署';
    case 'running':
      return '运行中';
    case 'completed':
      return '已完成';
    case 'failed':
      return '失败';
  }
}

function normalizeCanvasCoordinate(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return null;
  }

  return Math.round(value * 100) / 100;
}

function normalizeFlowgramEdge(
  edge: FlowgramWorkflowJSON['edges'][number],
) {
  return {
    sourceNodeID: edge.sourceNodeID,
    targetNodeID: edge.targetNodeID,
    sourcePortID: edge.sourcePortID ?? null,
    targetPortID: edge.targetPortID ?? null,
  };
}

interface NormalizedFlowgramNode {
  id: string;
  type: string | number;
  meta: unknown;
  data: unknown;
  blocks: NormalizedFlowgramNode[];
  edges: ReturnType<typeof normalizeFlowgramEdge>[];
}

function normalizeFlowgramValue(value: unknown): unknown {
  if (typeof value === 'number') {
    return normalizeCanvasCoordinate(value);
  }

  if (Array.isArray(value)) {
    return value.map(normalizeFlowgramValue);
  }

  if (!value || typeof value !== 'object') {
    return value ?? null;
  }

  return Object.keys(value)
    .sort((left, right) => left.localeCompare(right))
    .reduce<Record<string, unknown>>((acc, key) => {
      acc[key] = normalizeFlowgramValue((value as Record<string, unknown>)[key]);
      return acc;
    }, {});
}

function normalizeFlowgramNode(
  node: FlowgramWorkflowJSON['nodes'][number],
): NormalizedFlowgramNode {
  return {
    id: node.id,
    type: node.type,
    meta: normalizeFlowgramValue(node.meta ?? {}),
    data: normalizeFlowgramValue(node.data ?? {}),
    blocks: (node.blocks ?? [])
      .map(normalizeFlowgramNode)
      .sort((left, right) => left.id.localeCompare(right.id)),
    edges: (node.edges ?? [])
      .map(normalizeFlowgramEdge)
      .sort((left, right) => {
        const sourceCompare = left.sourceNodeID.localeCompare(right.sourceNodeID);
        if (sourceCompare !== 0) {
          return sourceCompare;
        }

        return left.targetNodeID.localeCompare(right.targetNodeID);
      }),
  };
}

function buildFlowgramGraphSignature(graph: FlowgramWorkflowJSON): string {
  const normalizedNodes = graph.nodes
    .map(normalizeFlowgramNode)
    .sort((left, right) => left.id.localeCompare(right.id));
  const normalizedEdges = [...graph.edges]
    .map(normalizeFlowgramEdge)
    .sort((left, right) => {
      const sourceCompare = left.sourceNodeID.localeCompare(right.sourceNodeID);
      return sourceCompare !== 0 ? sourceCompare : left.targetNodeID.localeCompare(right.targetNodeID);
    });

  return JSON.stringify({
    nodes: normalizedNodes,
    edges: normalizedEdges,
  });
}

function isBusinessFlowNode(node: FlowNodeEntity | null): node is FlowNodeEntity {
  if (!node || node.flowNodeType === FlowNodeBaseType.GROUP) {
    return false;
  }

  const rawData = (node.getExtInfo() ?? {}) as {
    nodeType?: string;
  };
  const nodeType = normalizeNodeKind(rawData.nodeType ?? node.flowNodeType);

  return (
    nodeType === 'timer' ||
    nodeType === 'serialTrigger' ||
    nodeType === 'modbusRead' ||
    nodeType === 'code' ||
    nodeType === 'native' ||
    nodeType === 'if' ||
    nodeType === 'switch' ||
    nodeType === 'tryCatch' ||
    nodeType === 'loop' ||
    nodeType === 'httpClient' ||
    nodeType === 'barkPush' ||
    nodeType === 'sqlWriter' ||
    nodeType === 'debugConsole' ||
    nodeType === 'subgraph' ||
    nodeType === 'subgraphInput' ||
    nodeType === 'subgraphOutput'
  );
}

/** 拒收一条连接时给用户的视觉 + 诊断反馈。
 *
 * `toPort.hasError = true` 让 FlowGram 默认 `isErrorPort` 渲染红色，
 * 1.5s 后自动复位。`canAddLine` 返回 false 后边其实不会真创建线，
 * 这里的红色仅作"刚才被拒"的瞬时反馈。 */
function applyConnectionRejectionFeedback(
  toPort: { hasError?: boolean },
  rejection: ConnectionRejection,
): void {
  toPort.hasError = true;
  window.setTimeout(() => {
    toPort.hasError = false;
  }, 1500);
  console.warn(`[pin-validator] ${formatRejection(rejection)}`);
}

function describeFlowgramError(error: unknown): string {
  if (error instanceof Error) {
    return error.stack ?? error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  try {
    return JSON.stringify(error);
  } catch {
    return '未知异常';
  }
}

function resolveNodePortColor(
  displayType: string,
  accentHex: string,
  nodeCodeColor: string,
): string {
  switch (displayType) {
    case 'timer':
      return 'color-mix(in srgb, var(--accent) 55%, var(--warning) 45%)';
    case 'serialTrigger':
      return 'color-mix(in srgb, var(--accent) 64%, var(--success) 36%)';
    case 'modbusRead':
      return 'color-mix(in srgb, var(--accent) 58%, var(--success) 42%)';
    case 'if':
      return 'var(--success)';
    case 'switch':
      return 'var(--warning)';
    case 'tryCatch':
      return 'var(--danger)';
    case 'loop':
      return 'color-mix(in srgb, var(--accent) 72%, var(--success) 28%)';
    case 'httpClient':
      return 'color-mix(in srgb, var(--warning) 56%, var(--danger) 44%)';
    case 'barkPush':
      return 'color-mix(in srgb, var(--danger) 34%, var(--accent) 66%)';
    case 'sqlWriter':
      return 'color-mix(in srgb, var(--success) 68%, var(--accent) 32%)';
    case 'debugConsole':
      return 'var(--muted)';
    case 'subgraph':
      return 'var(--accent)';
    case 'subgraphInput':
    case 'subgraphOutput':
      return 'var(--muted)';
    case 'code':
      return nodeCodeColor;
    case 'native':
    default:
      return accentHex;
  }
}

function getDefaultOutputPortId(node: FlowNodeEntity | null): string | undefined {
  if (!node) {
    return undefined;
  }

  const rawData = (node.getExtInfo() ?? {}) as {
    nodeType?: string;
    config?: unknown;
  };
  const nodeType = normalizeNodeKind(rawData.nodeType ?? node.flowNodeType);
  return getLogicNodeBranchDefinitions(nodeType, rawData.config)[0]?.key;
}

/** 容器节点渲染：标题栏 + SubCanvasRender 内嵌画布区域。 */
const SUBCANVAS_HEADER_OFFSET = -48;

function FlowgramContainerCard(props: FlowgramNodeMaterialProps) {
  const rawData = props.node.getExtInfo() as
    | { label?: string; nodeType?: string; config?: { script?: string } }
    | undefined;
  const nodeType = normalizeNodeKind(rawData?.nodeType ?? props.node.flowNodeType);
  const displayType = normalizeFlowgramDisplayType(nodeType);
  const runtimeStatus = props.runtimeStatus ?? 'idle';
  const containerClass = nodeType === 'loop' ? 'loop' : 'subgraph';
  const displayLabel = resolveNodeDisplayLabel(nodeType, rawData?.label);

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

function FlowgramNodeCard(props: FlowgramNodeMaterialProps) {
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

  const nodeType = normalizeNodeKind(rawData?.nodeType ?? props.node.flowNodeType);
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
      <div data-flow-editor-selectable="false" className="flowgram-card__body" draggable={false} data-pure-form={pureForm ? 'true' : undefined}>
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
        <strong>{resolveNodeDisplayLabel(nodeType, rawData?.label)}</strong>
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

function FlowgramToolButton({
  label,
  disabled,
  destructive = false,
  active = false,
  'data-testid': dataTestId,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  destructive?: boolean;
  active?: boolean;
  'data-testid'?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      data-testid={dataTestId}
      style={{
        ...FLOWGRAM_BUTTON_STYLE,
        cursor: disabled ? 'not-allowed' : 'pointer',
        color: disabled
          ? 'var(--toolbar-disabled)'
          : destructive
            ? 'var(--danger-ink)'
            : 'var(--toolbar-text)',
        background: active ? 'var(--surface-muted)' : 'transparent',
        opacity: disabled ? 0.7 : 1,
      }}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

function FlowgramToolbar({
  canRun,
  canTestRun,
  isWorkflowActive,
  minimapVisible,
  onToggleMinimap,
  onRun,
  onStop,
  onTestRun,
  onDownload,
}: FlowgramToolbarProps) {
  const { history, playground } = useClientContext();
  const downloadService = useService(FlowDownloadService);
  const tools = usePlaygroundTools({
    minZoom: 0.24,
    maxZoom: 2,
  });
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [isReadonly, setIsReadonly] = useState(playground.config.readonly);
  const [isDownloading, setIsDownloading] = useState(false);
  const [interactiveType, setInteractiveType] = useState<FlowgramInteractiveType>(
    () => getPreferredInteractiveType(),
  );
  const zoomMenuRef = useRef<HTMLDetailsElement | null>(null);
  const interactiveMenuRef = useRef<HTMLDetailsElement | null>(null);
  const downloadMenuRef = useRef<HTMLDetailsElement | null>(null);
  const minimapPopoverRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!history?.undoRedoService) {
      setCanUndo(false);
      setCanRedo(false);
      return;
    }

    const syncHistoryState = () => {
      setCanUndo(history.canUndo());
      setCanRedo(history.canRedo());
    };

    syncHistoryState();

    const disposable = history.undoRedoService.onChange(syncHistoryState);
    return () => disposable.dispose();
  }, [history]);

  useEffect(() => {
    setIsReadonly(playground.config.readonly);
    const dispose = playground.config.onReadonlyOrDisabledChange(({ readonly }) => {
      setIsReadonly(readonly);
    });
    return () => dispose.dispose();
  }, [playground]);

  useEffect(() => {
    setIsDownloading(downloadService.downloading);
    const dispose = downloadService.onDownloadingChange((value) => {
      setIsDownloading(value);
    });
    return () => dispose.dispose();
  }, [downloadService]);

  useEffect(() => {
    tools.setInteractiveType(interactiveType as EditorInteractiveType);
    setPreferredInteractiveType(interactiveType);
  }, [interactiveType, tools]);

  useEffect(() => {
    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node | null;
      if (
        interactiveMenuRef.current?.contains(target) ||
        zoomMenuRef.current?.contains(target) ||
        downloadMenuRef.current?.contains(target) ||
        minimapPopoverRef.current?.contains(target)
      ) {
        return;
      }

      closeMenu(interactiveMenuRef);
      closeMenu(zoomMenuRef);
      closeMenu(downloadMenuRef);
      if (minimapVisible) {
        onToggleMinimap();
      }
    }

    document.addEventListener('pointerdown', handlePointerDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
    };
  }, [minimapVisible, onToggleMinimap]);

  function closeMenu(ref: { current: HTMLDetailsElement | null }) {
    ref.current?.removeAttribute('open');
  }

  const canStop = isWorkflowActive && Boolean(onStop);
  const primaryActionLabel = canStop ? '停止' : '运行';
  const handlePrimaryAction = () => {
    if (canStop) {
      onStop?.();
      return;
    }

    onRun?.();
  };

  function renderMenuLabel(icon: ReactNode, label: string) {
    return (
      <>
        <span className="flowgram-tools__menu-item-icon">{icon}</span>
        <span>{label}</span>
      </>
    );
  }

  return (
    <div style={FLOWGRAM_TOOLS_STYLE} data-flow-editor-selectable="false">
      <div style={FLOWGRAM_TOOLS_SECTION_STYLE} className="flowgram-tools">
        <details ref={interactiveMenuRef} className="flowgram-tools__menu" data-no-window-drag>
          <summary
            className="flowgram-tools__icon-button"
            aria-label={interactiveType === 'PAD' ? '触控板模式' : '鼠标模式'}
            title={interactiveType === 'PAD' ? '触控板模式' : '鼠标模式'}
          >
            {interactiveType === 'PAD' ? (
              <TrackpadModeIcon width={16} height={16} />
            ) : (
              <MouseModeIcon width={16} height={16} />
            )}
          </summary>
          <div className="flowgram-tools__menu-panel">
            <button
              type="button"
              className={
                interactiveType === 'PAD'
                  ? 'flowgram-tools__menu-item is-active'
                  : 'flowgram-tools__menu-item'
              }
              onClick={() => {
                setInteractiveType('PAD');
                closeMenu(interactiveMenuRef);
              }}
            >
              {renderMenuLabel(<TrackpadModeIcon width={14} height={14} />, '触控板优先')}
            </button>
            <button
              type="button"
              className={
                interactiveType === 'MOUSE'
                  ? 'flowgram-tools__menu-item is-active'
                  : 'flowgram-tools__menu-item'
              }
              onClick={() => {
                setInteractiveType('MOUSE');
                closeMenu(interactiveMenuRef);
              }}
            >
              {renderMenuLabel(<MouseModeIcon width={14} height={14} />, '鼠标优先')}
            </button>
          </div>
        </details>

        <details ref={zoomMenuRef} className="flowgram-tools__menu" data-no-window-drag>
          <summary className="flowgram-tools__zoom" style={FLOWGRAM_ZOOM_STYLE}>
            {Math.floor(tools.zoom * 100)}%
          </summary>
          <div className="flowgram-tools__menu-panel">
            <button
              type="button"
              className="flowgram-tools__menu-item"
              onClick={() => {
                tools.zoomin();
                closeMenu(zoomMenuRef);
              }}
            >
              放大
            </button>
            <button
              type="button"
              className="flowgram-tools__menu-item"
              onClick={() => {
                tools.zoomout();
                closeMenu(zoomMenuRef);
              }}
            >
              缩小
            </button>
            <div className="flowgram-tools__menu-divider" />
            {[0.5, 1, 1.5, 2].map((zoomValue) => (
              <button
                key={zoomValue}
                type="button"
                className="flowgram-tools__menu-item"
                onClick={() => {
                  playground.config.updateZoom(zoomValue);
                  closeMenu(zoomMenuRef);
                }}
              >
                {`${Math.floor(zoomValue * 100)}%`}
              </button>
            ))}
          </div>
        </details>

        <FlowgramToolButton label="适配视图" onClick={() => tools.fitView()}>
          <FitViewIcon width={16} height={16} />
        </FlowgramToolButton>
        <FlowgramToolButton
          label="自动整理"
          disabled={isReadonly}
          onClick={() => {
            void tools.autoLayout();
          }}
        >
          <AutoLayoutIcon width={16} height={16} />
        </FlowgramToolButton>
        <div
          ref={minimapPopoverRef}
          className={`flowgram-tools__popover ${minimapVisible ? 'is-open' : ''}`}
          data-no-window-drag
        >
          <FlowgramToolButton
            label={minimapVisible ? '隐藏缩略图' : '显示缩略图'}
            active={minimapVisible}
            onClick={onToggleMinimap}
          >
            <MinimapIcon width={16} height={16} />
          </FlowgramToolButton>
          {minimapVisible ? (
            <div className="flowgram-tools__popover-panel flowgram-tools__popover-panel--minimap">
              <MinimapRender
                containerStyles={FLOWGRAM_MINIMAP_CONTAINER_STYLE}
                panelStyles={FLOWGRAM_MINIMAP_PANEL_STYLE}
                inactiveStyle={FLOWGRAM_MINIMAP_INACTIVE_STYLE}
              />
            </div>
          ) : null}
        </div>
        <FlowgramToolButton
          label={isReadonly ? '退出只读' : '只读模式'}
          onClick={() => {
            playground.config.readonly = !playground.config.readonly;
          }}
        >
          {isReadonly ? (
            <LockClosedIcon width={16} height={16} />
          ) : (
            <LockOpenIcon width={16} height={16} />
          )}
        </FlowgramToolButton>
        <FlowgramToolButton label="撤销" disabled={!canUndo} onClick={() => void history.undo()}>
          <UndoActionIcon width={16} height={16} />
        </FlowgramToolButton>
        <FlowgramToolButton label="重做" disabled={!canRedo} onClick={() => void history.redo()}>
          <RedoActionIcon width={16} height={16} />
        </FlowgramToolButton>

        <details ref={downloadMenuRef} className="flowgram-tools__menu" data-no-window-drag>
          <summary
            className="flowgram-tools__icon-button"
            aria-label={isDownloading ? '导出中' : '下载'}
            title={isDownloading ? '导出中' : '下载'}
          >
            <DownloadIcon width={16} height={16} />
          </summary>
          <div className="flowgram-tools__menu-panel">
            <button
              type="button"
              className="flowgram-tools__menu-item"
              disabled={isDownloading || isReadonly}
              onClick={() => {
                void onDownload(FlowDownloadFormat.PNG);
                closeMenu(downloadMenuRef);
              }}
            >
              {renderMenuLabel(<FileImageIcon width={14} height={14} />, 'PNG')}
            </button>
            <button
              type="button"
              className="flowgram-tools__menu-item"
              disabled={isDownloading || isReadonly}
              onClick={() => {
                void onDownload(FlowDownloadFormat.JPEG);
                closeMenu(downloadMenuRef);
              }}
            >
              {renderMenuLabel(<FileImageIcon width={14} height={14} />, 'JPEG')}
            </button>
            <button
              type="button"
              className="flowgram-tools__menu-item"
              disabled={isDownloading || isReadonly}
              onClick={() => {
                void onDownload(FlowDownloadFormat.SVG);
                closeMenu(downloadMenuRef);
              }}
            >
              {renderMenuLabel(<FileVectorIcon width={14} height={14} />, 'SVG')}
            </button>
            <div className="flowgram-tools__menu-divider" />
            <button
              type="button"
              className="flowgram-tools__menu-item"
              disabled={isDownloading || isReadonly}
              onClick={() => {
                void onDownload(FlowDownloadFormat.JSON);
                closeMenu(downloadMenuRef);
              }}
            >
              {renderMenuLabel(<FileJsonIcon width={14} height={14} />, 'JSON')}
            </button>
          </div>
        </details>

        <FlowgramToolButton label="测试运行" data-testid="test-run-button" onClick={() => onTestRun?.()} disabled={!canTestRun}>
          <TriggerActionIcon width={16} height={16} />
        </FlowgramToolButton>

        <button
          type="button"
          className={`flowgram-tools__action ${
            canStop
              ? 'flowgram-tools__action--stop'
              : 'flowgram-tools__action--run'
          }`}
          data-testid={canStop ? 'undeploy-button' : 'deploy-button'}
          onClick={handlePrimaryAction}
          disabled={canStop ? !onStop : !canRun}
        >
          {canStop ? <StopActionIcon width={14} height={14} /> : <RunActionIcon width={14} height={14} />}
          <span>{primaryActionLabel}</span>
        </button>
      </div>
    </div>
  );
}

export const FlowgramCanvas = forwardRef<FlowgramCanvasHandle, FlowgramCanvasProps>(function FlowgramCanvas({
  graph,
  resources,
  runtime,
  appearance,
  exportTarget,
  actions,
}, ref) {
  const { connections, aiProviders, activeAiProviderId, copilotParams } = resources;
  const { runtimeState, workflowStatus, canTestRun = false } = runtime;
  const { accentHex, themeMode, nodeCodeColor } = appearance;
  const { workspacePath, workflowName } = exportTarget ?? {};
  const {
    onRunRequested,
    onStopRequested,
    onTestRunRequested,
    onGraphChange,
    onError,
    onStatusMessage,
  } = actions;
  const [lastChange, setLastChange] = useState<string | null>(null);
  const [editorCtx, setEditorCtx] = useState<FreeLayoutPluginContext | null>(null);
  const [hasSelection, setHasSelection] = useState(false);
  const [minimapVisible, setMinimapVisible] = useState(false);
  const [isReadonlyMode, setIsReadonlyMode] = useState(false);
  const syncTimerRef = useRef<number | null>(null);
  const selectedNodeRef = useRef<FlowNodeEntity | null>(null);
  const latestGraphRef = useRef<WorkflowGraph | null>(graph);
  const applyingExternalGraphRef = useRef(false);
  const initialFlowgramDataRef = useRef<FlowgramWorkflowJSON | null>(null);
  const pendingFitViewRef = useRef(true);
  const flowgramData = useMemo(() => {
    if (!graph) {
      return null;
    }

    return toFlowgramWorkflowJson(graph);
  }, [graph]);
  const flowgramDataSignature = useMemo(
    () => (flowgramData ? buildFlowgramGraphSignature(flowgramData) : null),
    [flowgramData],
  );
  const latestFlowgramDataRef = useRef<FlowgramWorkflowJSON | null>(flowgramData);

  const reportFlowgramError = useCallback(
    (title: string, error: unknown) => {
      const detail = describeFlowgramError(error);
      console.error(title, error);
      onError?.(title, detail);
    },
    [onError],
  );

  useEffect(() => {
    if (graph) {
      latestGraphRef.current = graph;
    }
  }, [graph]);

  useEffect(() => {
    if (flowgramData) {
      latestFlowgramDataRef.current = flowgramData;
    }
  }, [flowgramData]);

  const resolvedGraph = graph ?? latestGraphRef.current;
  const resolvedFlowgramData = flowgramData ?? latestFlowgramDataRef.current;
  const connectionOptions = connections;
  const connectionDefaults = useMemo<FlowgramConnectionDefaults>(() => {
    const anyConnectionId = connections[0]?.id ?? null;
    const modbusConnectionId =
      connections.find((connection) => isModbusConnectionType(connection.type))?.id ?? null;
    const serialConnectionId =
      connections.find((connection) => isSerialConnectionType(connection.type))?.id ?? null;
    const mqttConnectionId =
      connections.find((connection) => isMqttConnectionType(connection.type))?.id ?? null;
    const httpConnectionId =
      connections.find((connection) => isHttpConnectionType(connection.type))?.id ?? null;
    const barkConnectionId =
      connections.find((connection) => isBarkConnectionType(connection.type))?.id ?? null;

    return {
      any: anyConnectionId,
      modbus: modbusConnectionId,
      serial: serialConnectionId,
      mqtt: mqttConnectionId,
      http: httpConnectionId,
      bark: barkConnectionId,
    };
  }, [connections]);

  useEffect(() => {
    if (!editorCtx && resolvedFlowgramData) {
      initialFlowgramDataRef.current = resolvedFlowgramData;
    }
  }, [editorCtx, resolvedFlowgramData]);

  if (!initialFlowgramDataRef.current && resolvedFlowgramData) {
    initialFlowgramDataRef.current = resolvedFlowgramData;
  }

  const activeNodeIds = useMemo(() => new Set(runtimeState.activeNodeIds), [runtimeState.activeNodeIds]);
  const completedNodeIds = useMemo(
    () => new Set(runtimeState.completedNodeIds),
    [runtimeState.completedNodeIds],
  );
  const failedNodeIds = useMemo(() => new Set(runtimeState.failedNodeIds), [runtimeState.failedNodeIds]);
  const outputNodeIds = useMemo(() => new Set(runtimeState.outputNodeIds), [runtimeState.outputNodeIds]);
  const isWorkflowRuntimeMapped =
    workflowStatus === 'running' || workflowStatus === 'completed' || workflowStatus === 'failed';
  const isWorkflowActive =
    workflowStatus === 'deployed' ||
    workflowStatus === 'running' ||
    workflowStatus === 'completed' ||
    workflowStatus === 'failed';
  const workflowStatusLabel = getCanvasWorkflowStatusLabel(workflowStatus);
  const runtimeContextLabel = useMemo(() => {
    if (!isWorkflowActive) {
      return '运行上下文: 未绑定';
    }

    if (runtimeState.traceId) {
      return `运行上下文: ${runtimeState.lastEventType ?? 'deployed'} @ ${runtimeState.lastNodeId ?? '--'}`;
    }

    if (workflowStatus === 'deployed') {
      return '运行上下文: 已绑定，等待触发';
    }

    return `运行上下文: ${workflowStatusLabel}`;
  }, [
    isWorkflowActive,
    runtimeState.lastEventType,
    runtimeState.lastNodeId,
    runtimeState.traceId,
    workflowStatus,
    workflowStatusLabel,
  ]);

  const resolveNodeRuntimeStatus = useCallback(
    (nodeId: string): RuntimeNodeStatus => {
      if (!isWorkflowRuntimeMapped) {
        return 'idle';
      }

      if (failedNodeIds.has(nodeId)) {
        return 'failed';
      }

      if (activeNodeIds.has(nodeId)) {
        return 'running';
      }

      if (outputNodeIds.has(nodeId)) {
        return 'output';
      }

      if (completedNodeIds.has(nodeId)) {
        return 'completed';
      }

      return 'idle';
    },
    [activeNodeIds, completedNodeIds, failedNodeIds, isWorkflowRuntimeMapped, outputNodeIds],
  );

  const resolveLineRuntimeStatus = useCallback(
    (line: WorkflowLineEntity): RuntimeNodeStatus => {
      if (!isWorkflowRuntimeMapped) {
        return 'idle';
      }

      const fromId = line.from?.id;
      const toId = line.to?.id;
      if (!fromId) {
        return 'idle';
      }

      if ((toId && failedNodeIds.has(toId)) || failedNodeIds.has(fromId)) {
        return 'failed';
      }

      if (activeNodeIds.has(fromId) || (toId && activeNodeIds.has(toId))) {
        return 'running';
      }

      if ((toId && outputNodeIds.has(toId)) || outputNodeIds.has(fromId)) {
        return 'output';
      }

      if ((toId && completedNodeIds.has(toId)) || completedNodeIds.has(fromId)) {
        return 'completed';
      }

      return 'idle';
    },
    [activeNodeIds, completedNodeIds, failedNodeIds, isWorkflowRuntimeMapped, outputNodeIds],
  );

  const isFlowingLine = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) =>
      resolveLineRuntimeStatus(line) === 'running',
    [resolveLineRuntimeStatus],
  );

  const isErrorLine = useCallback(
    (_ctx: FreeLayoutPluginContext, fromPort: { node: FlowNodeEntity }, toPort?: { node: FlowNodeEntity }) =>
      isWorkflowRuntimeMapped &&
      Boolean(
        (fromPort?.node?.id && failedNodeIds.has(fromPort.node.id)) ||
          (toPort?.node?.id && failedNodeIds.has(toPort.node.id)),
      ),
    [failedNodeIds, isWorkflowRuntimeMapped],
  );

  const setLineClassName = useCallback(
    (_ctx: FreeLayoutPluginContext, line: WorkflowLineEntity) => {
      const lineStatus = resolveLineRuntimeStatus(line);
      return lineStatus === 'idle' ? 'flowgram-line' : `flowgram-line flowgram-line--${lineStatus}`;
    },
    [resolveLineRuntimeStatus],
  );

  // 连接期 pin 类型校验：用户拖边瞬间被调用——pin schema 缓存命中且
  // 类型不兼容时返回 false 拒收，否则放行（缓存未命中也放行，部署期
  // pin_validator 作为 backstop 兜底）。
  const canAddLine = useCallback(
    (
      _ctx: FreeLayoutPluginContext,
      fromPort: { node: { id: string }; portID: string | number },
      toPort: { node: { id: string }; portID: string | number; hasError?: boolean },
      _lines: WorkflowLinesManager,
      silent?: boolean,
    ): boolean => {
      const result = checkConnection(
        fromPort.node.id,
        fromPort.portID,
        toPort.node.id,
        toPort.portID,
      );

      if (!result.allow && result.rejection && !silent) {
        applyConnectionRejectionFeedback(toPort, result.rejection);
      }

      return result.allow;
    },
    [],
  );

  const renderNodeCard = useCallback(
    (props: FlowgramNodeMaterialProps) => {
      const rawType = normalizeNodeKind(
        ((props.node.getExtInfo() ?? {}) as { nodeType?: string }).nodeType ?? props.node.flowNodeType,
      );
      if (rawType === 'subgraph' || rawType === 'loop') {
        return (
          <FlowgramContainerCard
            {...props}
            runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
            accentHex={accentHex}
            nodeCodeColor={nodeCodeColor}
          />
        );
      }
      return (
        <FlowgramNodeCard
          {...props}
          runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
          accentHex={accentHex}
          nodeCodeColor={nodeCodeColor}
        />
      );
    },
    [accentHex, nodeCodeColor, resolveNodeRuntimeStatus],
  );
  const materials = useMemo(
    () => ({
      renderDefaultNode: renderNodeCard,
    }),
    [renderNodeCard],
  );
  const buildCurrentWorkflowGraph = useCallback(
    (ctx: FreeLayoutPluginContext) => {
      if (!latestGraphRef.current) {
        return null;
      }

      const nextFlowgramGraph = ctx.document.toJSON();
      return toNazhWorkflowGraph(nextFlowgramGraph, latestGraphRef.current);
    },
    [],
  );

  const emitCurrentGraphChange = useCallback(
    (ctx: FreeLayoutPluginContext) => {
      try {
        const nextGraph = buildCurrentWorkflowGraph(ctx);
        if (!nextGraph) {
          return null;
        }

        const nextAstText = formatWorkflowGraph(nextGraph);
        onGraphChange(nextAstText);
        return nextAstText;
      } catch (error) {
        reportFlowgramError('FlowGram 当前工作流序列化失败', error);
        return null;
      }
    },
    [buildCurrentWorkflowGraph, onGraphChange, reportFlowgramError],
  );

  const syncSelectionState = useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      try {
        if (!ctx) {
          selectedNodeRef.current = null;
          setHasSelection(false);
          return;
        }

        const selectionService = ctx.document.selectServices;
        const selectedNodes = selectionService.selectedNodes;
        const nextSelectedNode = selectedNodes.length === 1 ? selectedNodes[0] : null;
        const nextBusinessNode = isBusinessFlowNode(nextSelectedNode) ? nextSelectedNode : null;
        const hadPreviousSelection = Boolean(selectedNodeRef.current);

        selectedNodeRef.current = nextBusinessNode;
        setHasSelection(Boolean(nextBusinessNode));

        // 从选中节点切换到无选中时（关闭设置面板），延迟同步图变更以避免渲染期 setState
        if (hadPreviousSelection && !nextBusinessNode) {
          setTimeout(() => emitCurrentGraphChange(ctx), 0);
        }

        const panelManager = (ctx as FreeLayoutPluginContext & {
          get?: <T>(token: unknown) => T;
        }).get?.<PanelManager>(PanelManager);

        if (!panelManager) {
          return;
        }

        if (ctx.playground.config.readonly) {
          panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right');
          return;
        }

        if (nextBusinessNode) {
          panelManager.open(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right', {
            props: {
              nodeId: nextBusinessNode.id,
              connections: connectionOptions,
              aiProviders,
              activeAiProviderId,
              copilotParams,
            },
          });
          return;
        }

        panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right');
      } catch (error) {
        reportFlowgramError('FlowGram 选择状态同步失败', error);
        return;
      }
    },
    [activeAiProviderId, aiProviders, connectionOptions, copilotParams, emitCurrentGraphChange, reportFlowgramError],
  );

  const applyExternalFlowgramGraph = useCallback(
    (ctx: FreeLayoutPluginContext, nextGraph: FlowgramWorkflowJSON) => {
      try {
        applyingExternalGraphRef.current = true;
        const operationContext = ctx as FreeLayoutPluginContext & {
          operation?: {
            fromJSON: (graph: FlowgramWorkflowJSON) => void;
          };
        };

        if (operationContext.operation) {
          operationContext.operation.fromJSON(nextGraph);
        } else {
          ctx.document.fromJSON(nextGraph);
        }

        syncSelectionState(ctx);
      } catch (error) {
        reportFlowgramError('FlowGram 外部数据载入失败', error);
      } finally {
        applyingExternalGraphRef.current = false;
      }
    },
    [reportFlowgramError, syncSelectionState],
  );

  const loadWorkflowGraph = useCallback(
    (nextGraph: WorkflowGraph) => {
      try {
        latestGraphRef.current = nextGraph;
        const nextFlowgramGraph = toFlowgramWorkflowJson(nextGraph);
        latestFlowgramDataRef.current = nextFlowgramGraph;

        if (syncTimerRef.current !== null) {
          window.clearTimeout(syncTimerRef.current);
          syncTimerRef.current = null;
        }

        if (!editorCtx) {
          initialFlowgramDataRef.current = nextFlowgramGraph;
          return;
        }

        pendingFitViewRef.current = true;
        applyExternalFlowgramGraph(editorCtx, nextFlowgramGraph);
      } catch (error) {
        reportFlowgramError('FlowGram 工作流导入失败', error);
      }
    },
    [applyExternalFlowgramGraph, editorCtx, reportFlowgramError],
  );

  useImperativeHandle(
    ref,
    () => ({
      isReady: () => Boolean(editorCtx),
      getCurrentWorkflowGraph: () =>
        editorCtx ? buildCurrentWorkflowGraph(editorCtx) : latestGraphRef.current,
      loadWorkflowGraph,
    }),
    [buildCurrentWorkflowGraph, editorCtx, loadWorkflowGraph],
  );

  const handleSaveCurrentGraph = useCallback(() => {
    if (!editorCtx) {
      return;
    }

    emitCurrentGraphChange(editorCtx);
  }, [editorCtx, emitCurrentGraphChange]);

  const handleDownloadCurrentGraph = useCallback(
    async (format: FlowDownloadFormat) => {
      if (!editorCtx) {
        return;
      }

      try {
        const downloadService = (editorCtx as FreeLayoutPluginContext & {
          get?: <T>(token: unknown) => T;
        }).get?.<FlowDownloadService>(FlowDownloadService) as unknown as
          | InternalFlowDownloadService
          | undefined;
        if (!downloadService) {
          return;
        }

        if (hasTauriRuntime()) {
          downloadService.setDownloading(true);

          try {
            const fileName = buildFlowgramExportFileName(workflowName, format);

            if (format === FlowDownloadFormat.JSON) {
              const json = downloadService.document.toJSON();
              const { content } = await downloadService.formatDataContent(json, format);
              const saved = await saveFlowgramExportFile(workspacePath ?? '', fileName, {
                text: content,
              });
              onStatusMessage?.(`已导出到 ${saved.filePath}`);
              return;
            }

            const imageUrl = await downloadService.exportImageService.export({
              format,
              watermarkSVG: downloadService.options?.watermarkSVG,
            });
            if (!imageUrl) {
              throw new Error('未能生成导出内容。');
            }

            const response = await fetch(imageUrl);
            if (!response.ok) {
              throw new Error(`导出内容读取失败: ${response.status}`);
            }

            const buffer = await response.arrayBuffer();
            const saved = await saveFlowgramExportFile(workspacePath ?? '', fileName, {
              bytes: Array.from(new Uint8Array(buffer)),
            });
            onStatusMessage?.(`已导出到 ${saved.filePath}`);
            return;
          } finally {
            downloadService.setDownloading(false);
          }
        }

        await downloadService.download({ format });
      } catch (error) {
        reportFlowgramError('FlowGram 导出失败', error);
      }
    },
    [editorCtx, onStatusMessage, reportFlowgramError, workflowName, workspacePath],
  );

  const handleEditorRef = useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      setEditorCtx(ctx);
    },
    [],
  );

  const handleAllLayersRendered = useCallback((ctx: FreeLayoutPluginContext) => {
    if (!pendingFitViewRef.current) {
      return;
    }

    pendingFitViewRef.current = false;
    void ctx.tools.fitView(true);
  }, []);
  const handleDragLineEnd = useCallback(
    async (ctx: FreeLayoutPluginContext, params: Parameters<typeof handleFlowgramDragLineEnd>[1]) => {
      try {
        await handleFlowgramDragLineEnd(ctx, params);
        syncSelectionState(ctx);
      } catch (error) {
        reportFlowgramError('FlowGram 连线处理失败', error);
      }
    },
    [reportFlowgramError, syncSelectionState],
  );

  useEffect(() => {
    void refreshCapabilitiesCache();
    return () => {
      if (syncTimerRef.current !== null) {
        window.clearTimeout(syncTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!editorCtx) {
      setIsReadonlyMode(false);
      return;
    }

    const selectionDisposable = editorCtx.document.selectServices.onSelectionChanged(() => {
      syncSelectionState(editorCtx);
    });

    syncSelectionState(editorCtx);

    return () => {
      selectionDisposable.dispose();
    };
  }, [editorCtx, syncSelectionState]);

  useEffect(() => {
    if (!editorCtx) {
      return;
    }

    setIsReadonlyMode(editorCtx.playground.config.readonly);
    const disposable = editorCtx.playground.config.onReadonlyOrDisabledChange(({ readonly }) => {
      setIsReadonlyMode(readonly);
      if (readonly) {
        syncSelectionState(editorCtx);
      }
    });

    return () => {
      disposable.dispose();
    };
  }, [editorCtx, syncSelectionState]);

  useEffect(() => {
    const nextFlowgramData = latestFlowgramDataRef.current;

    if (!editorCtx || !graph || !nextFlowgramData || !flowgramDataSignature) {
      return;
    }

    const currentGraphSignature = buildFlowgramGraphSignature(editorCtx.document.toJSON());
    if (currentGraphSignature === flowgramDataSignature) {
      return;
    }

    applyExternalFlowgramGraph(editorCtx, nextFlowgramData);
  }, [applyExternalFlowgramGraph, editorCtx, flowgramDataSignature, graph]);

  const handleContentChange = useCallback(
    (ctx: FreeLayoutPluginContext, event: WorkflowContentChangeEvent) => {
      try {
        if (applyingExternalGraphRef.current) {
          return;
        }

        if (event.type === WorkflowContentChangeType.META_CHANGE) {
          return;
        }

        // 节点生命周期事件触发 pin schema 缓存维护。refresh / invalidate
        // 都是 fire-and-forget——失败时缓存自动写 fallback Any/Any，部署期
        // 校验作为 backstop 兜底。
        if (
          event.type === WorkflowContentChangeType.ADD_NODE ||
          event.type === WorkflowContentChangeType.NODE_DATA_CHANGE
        ) {
          const entity = event.entity as { id?: string; getExtInfo?: () => unknown } | undefined;
          if (entity?.id && entity.getExtInfo) {
            const ext = (entity.getExtInfo() ?? {}) as {
              nodeType?: string;
              config?: unknown;
            };
            if (ext.nodeType) {
              void refreshNodePinSchema(
                entity.id,
                ext.nodeType,
                configToRecord(ext.config as never),
              );
            }
          }
        } else if (event.type === WorkflowContentChangeType.DELETE_NODE) {
          const entity = event.entity as { id?: string } | undefined;
          if (entity?.id) {
            invalidateNodePinSchema(entity.id);
          }
        }

        if (
          event.type === WorkflowContentChangeType.DELETE_NODE ||
          event.type === WorkflowContentChangeType.DELETE_LINE
        ) {
          ctx.playground.flush();
        }

        // 延迟状态更新，避免 FlowGram 在渲染期触发回调导致 setState 警告
        setTimeout(() => {
          setLastChange(event.type);
          syncSelectionState(ctx);

          if (!latestGraphRef.current) {
            return;
          }

          if (syncTimerRef.current !== null) {
            window.clearTimeout(syncTimerRef.current);
            syncTimerRef.current = null;
          }

          syncTimerRef.current = window.setTimeout(() => {
            emitCurrentGraphChange(ctx);
          }, 120);
        }, 0);
      } catch (error) {
        reportFlowgramError('FlowGram 内容同步失败', error);
      }
    },
    [syncSelectionState, emitCurrentGraphChange, reportFlowgramError],
  );

  function nextNodeId(prefix: string): string {
    const currentIds = new Set(
      editorCtx?.document.getAllNodes().map((node) => node.id) ?? Object.keys(resolvedGraph?.nodes ?? {}),
    );

    let index = 1;
    while (currentIds.has(`${prefix}_${index}`)) {
      index += 1;
    }

    return `${prefix}_${index}`;
  }

  function buildInsertionPosition(anchorNode: FlowNodeEntity | null) {
    if (!anchorNode) {
      return undefined;
    }

    const anchorPosition = anchorNode.getNodeMeta().position;
    if (!anchorPosition) {
      return undefined;
    }

    const branchOffset = anchorNode.lines.outputNodes.length * 168;

    return {
      x: anchorPosition.x + 320,
      y: anchorPosition.y + branchOffset,
    };
  }

  async function handleInsertNode(seed: NodeSeed, mode: 'standalone' | 'downstream') {
    if (!editorCtx || editorCtx.playground.config.readonly) {
      return;
    }

    try {
      const anchorNode = mode === 'downstream' ? selectedNodeRef.current : null;
      const nextId = nextNodeId(seed.idPrefix);
      const node = editorCtx.document.createWorkflowNodeByType(
        seed.kind,
        buildInsertionPosition(anchorNode),
        {
          id: nextId,
          type: seed.kind,
          data: resolveNodeData(seed, nextId, connectionDefaults),
        },
      );

      if (anchorNode) {
        const fromPort = getDefaultOutputPortId(anchorNode);
        editorCtx.document.linesManager.createLine({
          from: anchorNode.id,
          to: node.id,
          ...(fromPort ? { fromPort } : {}),
        });
      }

      editorCtx.document.selectServices.selectNode(node);
      await editorCtx.document.selectServices.selectNodeAndScrollToView(node, false);
      syncSelectionState(editorCtx);
    } catch (error) {
      reportFlowgramError('FlowGram 节点插入失败', error);
    }
  }

  const editorProps = useFlowgramEditorProps({
    initialData: initialFlowgramDataRef.current ??
      resolvedFlowgramData ?? {
        nodes: [],
        edges: [],
      },
    accentColor: accentHex,
    themeMode,
    connectionDefaults,
    materials,
    isFlowingLine,
    isErrorLine,
    setLineClassName,
    canAddLine,
    onContentChange: handleContentChange,
    onAllLayersRendered: handleAllLayersRendered,
    onDragLineEnd: handleDragLineEnd,
  });

  return (
    <section className="canvas-shell">
      {!resolvedGraph || !resolvedFlowgramData ? (
        <div className="canvas-empty">无有效流程</div>
      ) : (
        <FreeLayoutEditorProvider ref={handleEditorRef} {...editorProps}>
          <div className="flowgram-host">
            <div className="flowgram-workspace">
              <FlowgramNodeAddPanel
                connectionDefaults={connectionDefaults}
                hasSelection={hasSelection}
                disabled={isReadonlyMode}
                onInsertSeed={handleInsertNode}
              />

              <div className="flowgram-stage">
                <EditorRenderer className="flowgram-editor" />
                <FlowgramToolbar
                  canRun={Boolean(onRunRequested)}
                  canTestRun={canTestRun}
                  isWorkflowActive={isWorkflowActive}
                  minimapVisible={minimapVisible}
                  onToggleMinimap={() => setMinimapVisible((visible) => !visible)}
                  onRun={onRunRequested}
                  onStop={onStopRequested}
                  onTestRun={onTestRunRequested}
                  onDownload={handleDownloadCurrentGraph}
                />
                <div className="flowgram-overlay">
                  <span>{`工作流状态: ${workflowStatusLabel}`}</span>
                  <span>{lastChange ? `最近变更: ${lastChange}` : '未变更'}</span>
                  <span>{`${resolvedFlowgramData.nodes.length} nodes / ${resolvedFlowgramData.edges.length} edges`}</span>
                  <span>{runtimeContextLabel}</span>
                </div>
              </div>
            </div>
          </div>
        </FreeLayoutEditorProvider>
      )}
    </section>
  );
});
