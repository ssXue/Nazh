/**
 * Flowgram 画布底部工具栏组件。
 *
 * 包含缩放、交互模式切换、只读锁定、撤销重做、下载导出、
 * 缩略图、测试运行、部署/停止等全部画布操作按钮。
 * 从 FlowgramCanvas.tsx 拆出以降低单文件复杂度。
 */

import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react';
import {
  useClientContext,
  usePlaygroundTools,
  useService,
  type InteractiveType as EditorInteractiveType,
} from '@flowgram.ai/free-layout-editor';
import { FlowDownloadFormat, FlowDownloadService } from '@flowgram.ai/export-plugin';
import { MinimapRender } from '@flowgram.ai/minimap-plugin';

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
  TrackpadModeIcon,
  UndoActionIcon,
} from '../app/AppIcons';
import {
  type FlowgramInteractiveType,
  getPreferredInteractiveType,
  setPreferredInteractiveType,
} from './flowgram-canvas-utils';

// ---------------------------------------------------------------------------
// 样式常量
// ---------------------------------------------------------------------------

const FLOWGRAM_BUTTON_STYLE: CSSProperties = {
  border: '0',
  borderRadius: 'var(--radius-sm)',
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
  borderRadius: 'var(--radius-md)',
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
  borderRadius: 'var(--radius-sm)',
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

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface FlowgramToolbarProps {
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

// ---------------------------------------------------------------------------
// 子组件：工具按钮
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 主组件：工具栏
// ---------------------------------------------------------------------------

export function FlowgramToolbar({
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
