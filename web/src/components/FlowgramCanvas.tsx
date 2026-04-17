import {
  EditorRenderer,
  FreeLayoutEditorProvider,
  FlowNodeBaseType,
  FlowNodeEntity,
  WorkflowContentChangeType,
  WorkflowLineEntity,
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

import {
  AutoLayoutIcon,
  DownloadIcon,
  FileImageIcon,
  FileJsonIcon,
  FileVectorIcon,
  FileYamlIcon,
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
import { FLOWGRAM_NODE_SETTINGS_PANEL_KEY } from './flowgram/FlowgramNodeSettingsPanel';
import {
  FlowgramNodeGlyph,
  getFlowgramDisplayLabel,
  normalizeFlowgramDisplayType,
} from './flowgram/FlowgramNodeGlyph';
import {
  getLogicNodeBranchDefinitions,
  resolveNodeData,
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
import type {
  AiProviderView,
  ConnectionDefinition,
  WorkflowGraph,
  WorkflowRuntimeState,
  WorkflowWindowStatus,
} from '../types';

interface FlowgramCanvasProps {
  graph: WorkflowGraph | null;
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  accentHex: string;
  nodeRhaiColor: string;
  onRunRequested?: () => void;
  onStopRequested?: () => void;
  onDispatchRequested?: () => void;
  canDispatchPayload?: boolean;
  onGraphChange: (nextAstText: string) => void;
  onError?: (title: string, detail?: string | null) => void;
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
  nodeRhaiColor: string;
}

type RuntimeNodeStatus = 'idle' | 'running' | 'completed' | 'failed' | 'output';

type FlowgramInteractiveType = 'MOUSE' | 'PAD';

interface FlowgramToolbarProps {
  canRun: boolean;
  canSave: boolean;
  canDispatch: boolean;
  isWorkflowActive: boolean;
  minimapVisible: boolean;
  onToggleMinimap: () => void;
  onRun?: () => void;
  onStop?: () => void;
  onDispatch?: () => void;
  onSave: () => void;
  onDownload: (format: FlowDownloadFormat) => void | Promise<void>;
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
  left: 16,
  bottom: 16,
  display: 'flex',
  alignItems: 'center',
  gap: 8,
  maxWidth: 'calc(100% - 164px)',
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

  return (
    rawData.nodeType === 'timer' ||
    rawData.nodeType === 'serialTrigger' ||
    rawData.nodeType === 'modbusRead' ||
    rawData.nodeType === 'code' ||
    rawData.nodeType === 'native' ||
    rawData.nodeType === 'rhai' ||
    rawData.nodeType === 'if' ||
    rawData.nodeType === 'switch' ||
    rawData.nodeType === 'tryCatch' ||
    rawData.nodeType === 'loop' ||
    rawData.nodeType === 'httpClient' ||
    rawData.nodeType === 'sqlWriter' ||
    rawData.nodeType === 'debugConsole' ||
    node.flowNodeType === 'timer' ||
    node.flowNodeType === 'serialTrigger' ||
    node.flowNodeType === 'modbusRead' ||
    node.flowNodeType === 'code' ||
    node.flowNodeType === 'native' ||
    node.flowNodeType === 'rhai' ||
    node.flowNodeType === 'if' ||
    node.flowNodeType === 'switch' ||
    node.flowNodeType === 'tryCatch' ||
    node.flowNodeType === 'loop' ||
    node.flowNodeType === 'httpClient' ||
    node.flowNodeType === 'sqlWriter' ||
    node.flowNodeType === 'debugConsole'
  );
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
  nodeRhaiColor: string,
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
    case 'sqlWriter':
      return 'color-mix(in srgb, var(--success) 68%, var(--accent) 32%)';
    case 'debugConsole':
      return 'var(--muted)';
    case 'code':
    case 'rhai':
      return nodeRhaiColor;
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
  const nodeType = String(rawData.nodeType ?? node.flowNodeType);
  return getLogicNodeBranchDefinitions(nodeType, rawData.config)[0]?.key;
}

function FlowgramNodeCard(props: FlowgramNodeMaterialProps) {
  const rawData = props.node.getExtInfo() as
    | {
        label?: string;
        nodeType?: string;
        displayType?: string;
        connectionId?: string | null;
        aiDescription?: string | null;
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
          table?: string;
          database_path?: string;
          label?: string;
        };
      }
    | undefined;

  const nodeType = rawData?.nodeType ?? props.node.flowNodeType;
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
      : nodeType === 'code' || nodeType === 'rhai'
        ? rawData?.config?.script ?? 'Transform payload'
      : nodeType === 'if'
        ? rawData?.config?.script ?? 'return boolean'
      : nodeType === 'switch'
        ? rawData?.config?.script ?? 'return branch key'
      : nodeType === 'loop'
        ? rawData?.config?.script ?? 'return array or count'
        : nodeType === 'httpClient'
          ? rawData?.config?.webhook_kind === 'dingtalk'
            ? `钉钉报警 · ${rawData?.config?.method ?? 'POST'}`
            : `${rawData?.config?.method ?? 'POST'} ${rawData?.config?.url ?? ''}`.trim()
        : nodeType === 'sqlWriter'
          ? `${rawData?.config?.table ?? 'workflow_logs'} → ${rawData?.config?.database_path ?? './nazh-local.sqlite3'}`
        : nodeType === 'debugConsole'
          ? rawData?.config?.label ?? 'Console output'
        : rawData?.config?.script ?? 'Guarded script';

  return (
    <WorkflowNodeRenderer
      node={props.node}
      className={`flowgram-card flowgram-card--${nodeType} flowgram-card--display-${displayType} flowgram-card--${runtimeStatus} ${props.activated ? 'is-activated' : ''}`}
      portClassName="flowgram-card__port"
      portBackgroundColor="var(--panel-strong)"
      portPrimaryColor={resolveNodePortColor(displayType, props.accentHex, props.nodeRhaiColor)}
      portSecondaryColor="var(--surface-elevated)"
      portErrorColor="var(--danger)"
    >
      <div data-flow-editor-selectable="false" className="flowgram-card__body" draggable={false}>
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
        <strong>{rawData?.label ?? props.node.id}</strong>
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
                />
              </div>
            ))}
          </div>
        ) : null}
        <div className="flowgram-card__meta">
          <span>{rawData?.connectionId ? `conn: ${rawData.connectionId}` : 'logic-only node'}</span>
          <span>{rawData?.timeoutMs ? `${rawData.timeoutMs} ms timeout` : 'no timeout'}</span>
        </div>
        {rawData?.aiDescription ? <p className="flowgram-card__hint">{rawData.aiDescription}</p> : null}
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
  canSave,
  canDispatch,
  isWorkflowActive,
  minimapVisible,
  onToggleMinimap,
  onRun,
  onStop,
  onDispatch,
  onSave,
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
  const [savedPulse, setSavedPulse] = useState(false);
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
    if (!savedPulse) {
      return;
    }

    const timer = window.setTimeout(() => setSavedPulse(false), 1200);
    return () => window.clearTimeout(timer);
  }, [savedPulse]);

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
            <button
              type="button"
              className="flowgram-tools__menu-item"
              disabled={isDownloading || isReadonly}
              onClick={() => {
                void onDownload(FlowDownloadFormat.YAML);
                closeMenu(downloadMenuRef);
              }}
            >
              {renderMenuLabel(<FileYamlIcon width={14} height={14} />, 'YAML')}
            </button>
          </div>
        </details>

        <button
          type="button"
          className={savedPulse ? 'flowgram-tools__action is-saved' : 'flowgram-tools__action'}
          onClick={() => {
            onSave();
            setSavedPulse(true);
          }}
          disabled={!canSave || isReadonly}
        >
          {savedPulse ? '已保存' : '保存'}
        </button>

        {canDispatch ? (
          <FlowgramToolButton label="手动触发" data-testid="dispatch-button" onClick={() => onDispatch?.()}>
            <TriggerActionIcon width={16} height={16} />
          </FlowgramToolButton>
        ) : null}

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
  connections,
  aiProviders,
  activeAiProviderId,
  runtimeState,
  workflowStatus,
  accentHex,
  nodeRhaiColor,
  onRunRequested,
  onStopRequested,
  onDispatchRequested,
  canDispatchPayload = false,
  onGraphChange,
  onError,
}, ref) {
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

    return {
      any: anyConnectionId,
      modbus: modbusConnectionId,
      serial: serialConnectionId,
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

  const renderNodeCard = useCallback(
    (props: FlowgramNodeMaterialProps) => (
      <FlowgramNodeCard
        {...props}
        runtimeStatus={resolveNodeRuntimeStatus(props.node.id)}
        accentHex={accentHex}
        nodeRhaiColor={nodeRhaiColor}
      />
    ),
    [accentHex, nodeRhaiColor, resolveNodeRuntimeStatus],
  );
  const materials = useMemo(
    () => ({
      renderDefaultNode: renderNodeCard,
    }),
    [renderNodeCard],
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

        selectedNodeRef.current = nextBusinessNode;
        setHasSelection(Boolean(nextBusinessNode));

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
    [activeAiProviderId, aiProviders, connectionOptions, reportFlowgramError],
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
        }).get?.<FlowDownloadService>(FlowDownloadService);
        if (!downloadService) {
          return;
        }

        await downloadService.download({ format });
      } catch (error) {
        reportFlowgramError('FlowGram 导出失败', error);
      }
    },
    [editorCtx, reportFlowgramError],
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

  function handleContentChange(
    ctx: FreeLayoutPluginContext,
    event: WorkflowContentChangeEvent,
  ) {
    try {
      if (applyingExternalGraphRef.current) {
        return;
      }

      if (event.type === WorkflowContentChangeType.META_CHANGE) {
        return;
      }

      if (
        event.type === WorkflowContentChangeType.DELETE_NODE ||
        event.type === WorkflowContentChangeType.DELETE_LINE
      ) {
        ctx.playground.flush();
      }

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
    } catch (error) {
      reportFlowgramError('FlowGram 内容同步失败', error);
    }
  }

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
    connectionDefaults,
    materials,
    isFlowingLine,
    isErrorLine,
    setLineClassName,
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
                  canSave={Boolean(editorCtx && resolvedGraph)}
                  canDispatch={canDispatchPayload}
                  isWorkflowActive={isWorkflowActive}
                  minimapVisible={minimapVisible}
                  onToggleMinimap={() => setMinimapVisible((visible) => !visible)}
                  onRun={onRunRequested}
                  onStop={onStopRequested}
                  onDispatch={onDispatchRequested}
                  onSave={handleSaveCurrentGraph}
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
