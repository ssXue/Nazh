import { useMemo, type MouseEvent } from 'react';

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
      className={disabled ? 'flowgram-add-panel is-disabled' : 'flowgram-add-panel'}
      data-no-window-drag
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
