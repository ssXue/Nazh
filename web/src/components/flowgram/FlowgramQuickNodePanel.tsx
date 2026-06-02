import { useMemo, useState, useCallback, type MouseEvent } from 'react';

import type { NodePanelRenderProps } from '@flowgram.ai/free-node-panel-plugin';
import type { WorkflowNodeJSON } from '@flowgram.ai/free-layout-editor';

import {
  buildPaletteNodeJson,
  getFlowgramPaletteGroups,
  type FlowgramConnectionDefaults,
  type FlowgramPaletteGroup,
  type FlowgramPaletteItem,
} from './flowgram-node-library';
import {
  FlowgramNodeGlyph,
  getFlowgramDisplayLabel,
  normalizeFlowgramDisplayType,
} from './FlowgramNodeGlyph';

export function createFlowgramQuickNodePanel(connectionDefaults: FlowgramConnectionDefaults) {
  return function FlowgramQuickNodePanel(props: NodePanelRenderProps) {
    const paletteGroups = useMemo(() => getFlowgramPaletteGroups(), []);
    const [expandedKeys, setExpandedKeys] = useState<Set<string>>(() => {
      const initial = new Set<string>();
      for (const group of paletteGroups) {
        if (!group.collapsed) {
          initial.add(group.key);
        }
      }
      return initial;
    });
    const [search, setSearch] = useState('');

    const handleSelect = useCallback(
      (event: MouseEvent<HTMLButtonElement>, item: FlowgramPaletteItem) => {
        props.onSelect({
          nodeType: item.seed.kind,
          nodeJSON: buildPaletteNodeJson(item.seed, connectionDefaults) as WorkflowNodeJSON,
          selectEvent: event,
        });
      },
      [props, connectionDefaults],
    );

    const toggleGroup = useCallback((key: string) => {
      setExpandedKeys((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        return next;
      });
    }, []);

    const lowerSearch = search.trim().toLowerCase();
    const hasSearch = lowerSearch.length > 0;

    const filteredGroups = useMemo(() => {
      if (!hasSearch) return paletteGroups;

      return paletteGroups
        .map((group): FlowgramPaletteGroup | null => {
          const matchedSections = group.sections
            .map((section) => {
              const matched = section.items.filter((item) => {
                const hay = `${item.title} ${item.badge} ${item.description} ${item.seed.kind}`.toLowerCase();
                return hay.includes(lowerSearch);
              });
              return matched.length > 0 ? { ...section, items: matched } : null;
            })
            .filter((s): s is NonNullable<typeof s> => s !== null);

          return matchedSections.length > 0
            ? { ...group, sections: matchedSections }
            : null;
        })
        .filter((g): g is NonNullable<typeof g> => g !== null);
    }, [paletteGroups, hasSearch, lowerSearch]);

    return (
      <div
        className="flowgram-node-panel"
        style={{
          left: props.position.x,
          top: props.position.y,
        }}
        data-flow-editor-selectable="false"
      >
        <div className="flowgram-node-panel__search">
          <input
            type="text"
            className="flowgram-node-panel__search-input"
            placeholder="搜索节点…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </div>

        {filteredGroups.map((group) => {
          const expanded = hasSearch || expandedKeys.has(group.key);

          return (
            <div
              key={group.key}
              className={`flowgram-node-panel__group${expanded ? ' flowgram-node-panel__group--expanded' : ''}`}
            >
              <button
                type="button"
                className="flowgram-node-panel__group-header"
                onClick={() => toggleGroup(group.key)}
              >
                <span className="flowgram-node-panel__group-arrow" aria-hidden="true">
                  ▸
                </span>
                {group.title}
                <span className="flowgram-node-panel__group-count">
                  {group.sections.reduce((n, s) => n + s.items.length, 0)}
                </span>
              </button>

              {expanded && group.sections.map((section) => (
                <div key={section.key} className="flowgram-node-panel__section">
                  <div className="flowgram-node-panel__title">{section.title}</div>

                  <div className="flowgram-node-panel__list">
                    {section.items.map((item) => {
                      const displayType = normalizeFlowgramDisplayType(item.seed.displayType ?? item.seed.kind);

                      return (
                        <button
                          key={item.key}
                          type="button"
                          className={`flowgram-node-panel__item flowgram-node-panel__item--${displayType}`}
                          onClick={(event) => handleSelect(event, item)}
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
                </div>
              ))}
            </div>
          );
        })}

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
