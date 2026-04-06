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
  type NodeSeed,
} from './flowgram-node-library';

interface FlowgramNodeAddPanelProps {
  primaryConnectionId: string | null;
  hasSelection: boolean;
  onInsertSeed: (seed: NodeSeed, mode: 'standalone' | 'downstream') => void | Promise<void>;
}

export function FlowgramNodeAddPanel({
  primaryConnectionId,
  hasSelection,
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
      buildPaletteNodeJson(seed, primaryConnectionId, baseJson),
    );
  }

  return (
    <aside className="flowgram-add-panel">
      {paletteSections.map((section) => (
        <section key={section.key} className="flowgram-add-panel__section">
          <div className="flowgram-add-panel__title">{section.title}</div>

          <div className="flowgram-add-panel__list">
            {section.items.map((item) => (
              <button
                key={item.key}
                type="button"
                className={`flowgram-add-card flowgram-add-card--${item.seed.kind}`}
                data-flow-editor-selectable="false"
                onMouseDown={(event) => void handleCardMouseDown(event, item.seed)}
                onDoubleClick={() =>
                  void onInsertSeed(item.seed, hasSelection ? 'downstream' : 'standalone')
                }
              >
                <strong>{item.title}</strong>
                <span>{item.badge}</span>
              </button>
            ))}
          </div>
        </section>
      ))}
    </aside>
  );
}
