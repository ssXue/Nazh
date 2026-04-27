import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';

import {
  WorkflowDocument,
  WorkflowDragService,
  type WorkflowNodeJSON,
  useClientContext,
  useService,
} from '@flowgram.ai/free-layout-editor';

import {
  buildPaletteNodeJson,
  getFlowgramPaletteSections,
  type FlowgramConnectionDefaults,
  type NodeSeed,
} from './flowgram-node-library';
import {
  FlowgramNodeGlyph,
  getFlowgramDisplayLabel,
  normalizeFlowgramDisplayType,
} from './FlowgramNodeGlyph';

/** 左侧触发区宽度（比面板宽一点，方便鼠标进入） */
const REVEAL_HIT_ZONE = 200;
/** 鼠标离开后隐藏延迟（ms） */
const HIDE_DELAY = 600;

interface FlowgramNodeAddPanelProps {
  connectionDefaults: FlowgramConnectionDefaults;
  hasSelection: boolean;
  disabled?: boolean;
  onInsertSeed: (seed: NodeSeed, mode: 'standalone' | 'downstream') => void | Promise<void>;
}

export function FlowgramNodeAddPanel({
  connectionDefaults,
  hasSelection,
  disabled = false,
  onInsertSeed,
}: FlowgramNodeAddPanelProps) {
  const workflowDocument = useService(WorkflowDocument);
  const dragService = useService(WorkflowDragService);
  const context = useClientContext();
  const paletteSections = useMemo(() => getFlowgramPaletteSections(), []);

  const [revealed, setRevealed] = useState(false);
  const hideTimerRef = useRef(0);

  useEffect(() => {
    const container = context.container;
    if (!container) return;
    const el = container as unknown as HTMLElement;
    if (!el.getBoundingClientRect) return;

    function handleMouseMove(e: globalThis.MouseEvent) {
      const rect = el.getBoundingClientRect();
      const relX = e.clientX - rect.left;
      const relY = e.clientY - rect.top;
      const inLeftZone = relX > 32 && relX < REVEAL_HIT_ZONE && relY > 74;

      if (inLeftZone) {
        clearTimeout(hideTimerRef.current);
        setRevealed(true);
      }
    }

    function handleMouseLeave() {
      hideTimerRef.current = window.setTimeout(() => setRevealed(false), HIDE_DELAY);
    }

    el.addEventListener('mousemove', handleMouseMove);
    el.addEventListener('mouseleave', handleMouseLeave);

    return () => {
      el.removeEventListener('mousemove', handleMouseMove);
      el.removeEventListener('mouseleave', handleMouseLeave);
      clearTimeout(hideTimerRef.current);
    };
  }, [context.container]);

  const scheduleHide = useCallback(() => {
    hideTimerRef.current = window.setTimeout(() => setRevealed(false), HIDE_DELAY);
  }, []);

  const cancelHide = useCallback(() => {
    clearTimeout(hideTimerRef.current);
  }, []);

  async function handleCardMouseDown(
    event: MouseEvent<HTMLButtonElement>,
    seed: NodeSeed,
  ) {
    if (disabled) {
      return;
    }

    if (event.button !== 0) {
      return;
    }

    const registry = workflowDocument.getNodeRegistry(seed.kind) as {
      onAdd?: (ctx: unknown) => Partial<WorkflowNodeJSON>;
    };
    const baseJson = registry.onAdd?.(context) ?? {};

    await dragService.startDragCard(
      seed.kind,
      event,
      buildPaletteNodeJson(seed, connectionDefaults, baseJson),
    );
  }

  return (
    <aside
      className={[
        'flowgram-add-panel',
        disabled && 'is-disabled',
        revealed && 'is-hover-reveal',
      ]
        .filter(Boolean)
        .join(' ')}
      data-no-window-drag
      onMouseEnter={cancelHide}
      onMouseLeave={scheduleHide}
    >
      {paletteSections.map((section) => (
        <section key={section.key} className="flowgram-add-panel__section">
          <div className="flowgram-add-panel__title">{section.title}</div>

          <div className="flowgram-add-panel__list">
            {section.items.map((item) => {
              const displayType = normalizeFlowgramDisplayType(item.seed.displayType ?? item.seed.kind);

              return (
                <button
                  key={item.key}
                  type="button"
                  className={`flowgram-add-card flowgram-add-card--${displayType}`}
                  data-flow-editor-selectable="false"
                  disabled={disabled}
                  onMouseDown={(event) => void handleCardMouseDown(event, item.seed)}
                  onDoubleClick={() => {
                    if (disabled) {
                      return;
                    }

                    void onInsertSeed(item.seed, hasSelection ? 'downstream' : 'standalone');
                  }}
                >
                  <span className={`flowgram-add-card__icon flowgram-add-card__icon--${displayType}`}>
                    <FlowgramNodeGlyph displayType={displayType} width={16} height={16} />
                  </span>
                  <span className="flowgram-add-card__copy">
                    <strong>{item.title}</strong>
                    <span>{item.badge || getFlowgramDisplayLabel(displayType)}</span>
                  </span>
                </button>
              );
            })}
          </div>
        </section>
      ))}
    </aside>
  );
}
