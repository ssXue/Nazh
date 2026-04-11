import { useMemo, type MouseEvent } from 'react';

import type { NodePanelRenderProps } from '@flowgram.ai/free-node-panel-plugin';
import type { WorkflowNodeJSON } from '@flowgram.ai/free-layout-editor';

import {
  buildPaletteNodeJson,
  getFlowgramPaletteSections,
  type FlowgramConnectionDefaults,
} from './flowgram-node-library';
import {
  FlowgramNodeGlyph,
  getFlowgramDisplayLabel,
  normalizeFlowgramDisplayType,
} from './FlowgramNodeGlyph';

export function createFlowgramQuickNodePanel(connectionDefaults: FlowgramConnectionDefaults) {
  return function FlowgramQuickNodePanel(props: NodePanelRenderProps) {
    const paletteSections = useMemo(() => getFlowgramPaletteSections(), []);

    function handleSelect(
      event: MouseEvent<HTMLButtonElement>,
      nodeType: string,
      nodeJSON: WorkflowNodeJSON,
    ) {
      props.onSelect({
        nodeType,
        nodeJSON,
        selectEvent: event,
      });
    }

    return (
      <div
        className="flowgram-node-panel"
        style={{
          left: props.position.x,
          top: props.position.y,
        }}
        data-flow-editor-selectable="false"
      >
        {paletteSections.map((section) => (
          <section key={section.key} className="flowgram-node-panel__section">
            <div className="flowgram-node-panel__title">{section.title}</div>

            <div className="flowgram-node-panel__list">
              {section.items.map((item) => {
                const displayType = normalizeFlowgramDisplayType(item.seed.displayType ?? item.seed.kind);

                return (
                  <button
                    key={item.key}
                    type="button"
                    className={`flowgram-node-panel__item flowgram-node-panel__item--${displayType}`}
                    onClick={(event) =>
                      handleSelect(
                        event,
                        item.seed.kind,
                        buildPaletteNodeJson(item.seed, connectionDefaults) as WorkflowNodeJSON,
                      )
                    }
                  >
                    <span className={`flowgram-node-panel__glyph flowgram-node-panel__glyph--${displayType}`}>
                      <FlowgramNodeGlyph displayType={displayType} width={14} height={14} />
                    </span>
                    <span className="flowgram-node-panel__copy">
                      <strong>{item.title}</strong>
                      <span>{item.badge || getFlowgramDisplayLabel(displayType)}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          </section>
        ))}

        <button
          type="button"
          className="flowgram-node-panel__dismiss"
          onClick={() => props.onClose()}
        >
          关闭
        </button>
      </div>
    );
  };
}
