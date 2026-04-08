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
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { MinimapRender } from '@flowgram.ai/minimap-plugin';

import {
  DownloadIcon,
  FitViewIcon,
  LockClosedIcon,
  LockOpenIcon,
  MinimapIcon,
  MouseModeIcon,
  RedoActionIcon,
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
import type { WorkflowGraph, WorkflowRuntimeState, WorkflowWindowStatus } from '../types';

interface FlowgramCanvasProps {
  graph: WorkflowGraph | null;
  reloadVersion: number;
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  accentHex: string;
  nodeRhaiColor: string;
  onRunRequested?: () => void;
  onGraphChange: (nextAstText: string) => void;
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
  minimapVisible: boolean;
  onToggleMinimap: () => void;
  onRun?: () => void;
  onSave: () => void;
  onDownload: (format: FlowDownloadFormat) => void | Promise<void>;
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

const FLOWGRAM_MINIMAP_WRAPPER_STYLE: CSSProperties = {
  position: 'absolute',
  right: 16,
  bottom: 72,
  zIndex: 10,
  width: 118,
  height: FLOWGRAM_MINIMAP_CANVAS_HEIGHT + FLOWGRAM_MINIMAP_PANEL_PADDING * 2,
};

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

function resolveNodePortColor(
  displayType: string,
  accentHex: string,
  nodeRhaiColor: string,
): string {
  switch (displayType) {
    case 'timer':
      return 'color-mix(in srgb, var(--accent) 55%, var(--warning) 45%)';
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
          ? `${rawData?.config?.method ?? 'POST'} ${rawData?.config?.url ?? ''}`.trim()
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
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  destructive?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      style={{
        ...FLOWGRAM_BUTTON_STYLE,
        cursor: disabled ? 'not-allowed' : 'pointer',
        color: disabled
          ? 'var(--toolbar-disabled)'
          : destructive
            ? 'var(--danger-ink)'
            : 'var(--toolbar-text)',
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
  minimapVisible,
  onToggleMinimap,
  onRun,
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
        downloadMenuRef.current?.contains(target)
      ) {
        return;
      }

      closeMenu(interactiveMenuRef);
      closeMenu(zoomMenuRef);
      closeMenu(downloadMenuRef);
    }

    document.addEventListener('pointerdown', handlePointerDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
    };
  }, []);

  function closeMenu(ref: { current: HTMLDetailsElement | null }) {
    ref.current?.removeAttribute('open');
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
              触控板优先
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
              鼠标优先
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
          label={minimapVisible ? '隐藏缩略图' : '显示缩略图'}
          onClick={onToggleMinimap}
        >
          <MinimapIcon width={16} height={16} />
        </FlowgramToolButton>
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
              PNG
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
              JPEG
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
              SVG
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
              JSON
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
              YAML
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

        <button
          type="button"
          className="flowgram-tools__action flowgram-tools__action--primary"
          onClick={onRun}
          disabled={!canRun}
        >
          运行
        </button>
      </div>
    </div>
  );
}

function FlowgramMinimap({ hidden = false }: { hidden?: boolean }) {
  if (hidden) {
    return null;
  }

  return (
    <div style={FLOWGRAM_MINIMAP_WRAPPER_STYLE} data-flow-editor-selectable="false">
      <MinimapRender
        containerStyles={FLOWGRAM_MINIMAP_CONTAINER_STYLE}
        panelStyles={FLOWGRAM_MINIMAP_PANEL_STYLE}
        inactiveStyle={FLOWGRAM_MINIMAP_INACTIVE_STYLE}
      />
    </div>
  );
}

export function FlowgramCanvas({
  graph,
  reloadVersion,
  runtimeState,
  workflowStatus,
  accentHex,
  nodeRhaiColor,
  onRunRequested,
  onGraphChange,
}: FlowgramCanvasProps) {
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
  const lastReloadVersionRef = useRef(reloadVersion);
  const connectionOptions = graph?.connections ?? [];
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
  const primaryConnectionId = graph?.connections?.[0]?.id ?? null;

  useEffect(() => {
    latestGraphRef.current = graph;
  }, [graph]);

  useEffect(() => {
    latestFlowgramDataRef.current = flowgramData;
  }, [flowgramData]);

  useEffect(() => {
    if (reloadVersion !== lastReloadVersionRef.current) {
      lastReloadVersionRef.current = reloadVersion;
      pendingFitViewRef.current = true;
    }
  }, [reloadVersion]);

  useEffect(() => {
    if (!editorCtx) {
      initialFlowgramDataRef.current = flowgramData;
    }
  }, [editorCtx, flowgramData]);

  if (!initialFlowgramDataRef.current && flowgramData) {
    initialFlowgramDataRef.current = flowgramData;
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
  const workflowStatusLabel = getCanvasWorkflowStatusLabel(workflowStatus);

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

      const fromId = line.from.id;
      const toId = line.to?.id;

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
      (failedNodeIds.has(fromPort.node.id) || Boolean(toPort && failedNodeIds.has(toPort.node.id))),
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
          },
        });
        return;
      }

      panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right');
    },
    [connectionOptions],
  );

  const buildCurrentAstText = useCallback(
    (ctx: FreeLayoutPluginContext) => {
      if (!latestGraphRef.current) {
        return null;
      }

      const nextFlowgramGraph = ctx.document.toJSON();
      const nextGraph = toNazhWorkflowGraph(nextFlowgramGraph, latestGraphRef.current);
      return formatWorkflowGraph(nextGraph);
    },
    [],
  );

  const handleSaveCurrentGraph = useCallback(() => {
    if (!editorCtx) {
      return;
    }

    const nextAstText = buildCurrentAstText(editorCtx);
    if (!nextAstText) {
      return;
    }

    onGraphChange(nextAstText);
  }, [buildCurrentAstText, editorCtx, onGraphChange]);

  const handleDownloadCurrentGraph = useCallback(
    async (format: FlowDownloadFormat) => {
      if (!editorCtx) {
        return;
      }

      const downloadService = (editorCtx as FreeLayoutPluginContext & {
        get?: <T>(token: unknown) => T;
      }).get?.<FlowDownloadService>(FlowDownloadService);
      if (!downloadService) {
        return;
      }

      await downloadService.download({ format });
    },
    [editorCtx],
  );

  const handleEditorRef = useCallback(
    (ctx: FreeLayoutPluginContext | null) => {
      setEditorCtx(ctx);
    },
    [],
  );

  const applyExternalFlowgramGraph = useCallback(
    (ctx: FreeLayoutPluginContext, nextGraph: FlowgramWorkflowJSON) => {
      applyingExternalGraphRef.current = true;

      try {
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
      } finally {
        applyingExternalGraphRef.current = false;
      }
    },
    [syncSelectionState],
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
      await handleFlowgramDragLineEnd(ctx, params);
      syncSelectionState(ctx);
    },
    [syncSelectionState],
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

    if (!editorCtx || !nextFlowgramData || !flowgramDataSignature) {
      return;
    }

    const currentGraphSignature = buildFlowgramGraphSignature(editorCtx.document.toJSON());
    if (currentGraphSignature === flowgramDataSignature) {
      return;
    }

    applyExternalFlowgramGraph(editorCtx, nextFlowgramData);
  }, [applyExternalFlowgramGraph, editorCtx, flowgramDataSignature, reloadVersion]);

  function handleContentChange(
    ctx: FreeLayoutPluginContext,
    event: WorkflowContentChangeEvent,
  ) {
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
    }

    syncTimerRef.current = window.setTimeout(() => {
      const nextAstText = buildCurrentAstText(ctx);
      if (nextAstText) {
        onGraphChange(nextAstText);
      }
    }, 120);
  }

  function nextNodeId(prefix: string): string {
    const currentIds = new Set(
      editorCtx?.document.getAllNodes().map((node) => node.id) ?? Object.keys(graph?.nodes ?? {}),
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

    const anchorNode = mode === 'downstream' ? selectedNodeRef.current : null;
    const nextId = nextNodeId(seed.idPrefix);
    const node = editorCtx.document.createWorkflowNodeByType(
      seed.kind,
      buildInsertionPosition(anchorNode),
      {
        id: nextId,
        type: seed.kind,
        data: resolveNodeData(seed, nextId, primaryConnectionId),
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
  }

  const editorProps = useFlowgramEditorProps({
    initialData: initialFlowgramDataRef.current ??
      flowgramData ?? {
        nodes: [],
        edges: [],
      },
    accentColor: accentHex,
    primaryConnectionId,
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
      {!graph || !flowgramData ? (
        <div className="canvas-empty">无有效流程</div>
      ) : (
        <FreeLayoutEditorProvider ref={handleEditorRef} {...editorProps}>
          <div className="flowgram-host">
            <div className="flowgram-workspace">
              <FlowgramNodeAddPanel
                primaryConnectionId={primaryConnectionId}
                hasSelection={hasSelection}
                disabled={isReadonlyMode}
                onInsertSeed={handleInsertNode}
              />

              <div className="flowgram-stage">
                <EditorRenderer className="flowgram-editor" />
                <FlowgramToolbar
                  canRun={Boolean(onRunRequested)}
                  canSave={Boolean(editorCtx && graph)}
                  minimapVisible={minimapVisible}
                  onToggleMinimap={() => setMinimapVisible((visible) => !visible)}
                  onRun={onRunRequested}
                  onSave={handleSaveCurrentGraph}
                  onDownload={handleDownloadCurrentGraph}
                />
                <FlowgramMinimap hidden={!minimapVisible || (hasSelection && !isReadonlyMode)} />
                <div className="flowgram-overlay">
                  <span>{`工作流状态: ${workflowStatusLabel}`}</span>
                  <span>{lastChange ? `最近变更: ${lastChange}` : '未变更'}</span>
                  <span>{`${flowgramData.nodes.length} nodes / ${flowgramData.edges.length} edges`}</span>
                  <span>
                    {isWorkflowRuntimeMapped && runtimeState.traceId
                      ? `运行态: ${runtimeState.lastEventType ?? 'idle'} @ ${runtimeState.lastNodeId ?? '--'}`
                      : '运行态: non-runtime'}
                  </span>
                </div>
              </div>
            </div>
          </div>
        </FreeLayoutEditorProvider>
      )}
    </section>
  );
}
