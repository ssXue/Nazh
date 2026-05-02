import { useEffect, useMemo, useState } from 'react';

import type { NodeTypeEntry } from '../../types';
import { hasTauriRuntime, listNodeTypes } from '../../lib/tauri';
import { NODE_CATEGORIES, getNodeCatalogInfo } from '../flowgram/flowgram-node-library';
import {
  NODE_CAPABILITY_LABELS,
  capabilityNames,
  type NodeCapabilityName,
} from '../../lib/node-capabilities';

interface PluginPanelProps {
  isTauriRuntime: boolean;
}

interface PluginDisplayEntry {
  name: string;
  category: string;
  description: string;
  capabilities: NodeCapabilityName[];
}

export function PluginPanel({ isTauriRuntime }: PluginPanelProps) {
  const [entries, setEntries] = useState<PluginDisplayEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setIsLoading(false);
      return;
    }

    let cancelled = false;

    const load = async () => {
      try {
        const response = await listNodeTypes();
        if (cancelled) return;

        const displayEntries: PluginDisplayEntry[] = response.types.map(
          (nodeType: NodeTypeEntry) => {
            const meta = getNodeCatalogInfo(nodeType.name);
            return {
              name: nodeType.name,
              category: meta.category,
              description: meta.description,
              capabilities: capabilityNames(nodeType.capabilities),
            };
          },
        );

        setEntries(displayEntries);
        setError(null);
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : '加载节点类型失败');
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const grouped = useMemo(() => {
    const groups = new Map<string, PluginDisplayEntry[]>();

    const allCategories: string[] = [
      ...(NODE_CATEGORIES as readonly string[]),
      '其他',
    ];
    for (const cat of allCategories) {
      groups.set(cat, []);
    }

    for (const entry of entries) {
      const list = groups.get(entry.category);
      if (list) {
        list.push(entry);
      } else {
        let other = groups.get('其他');
        if (!other) {
          other = [];
          groups.set('其他', other);
        }
        other.push(entry);
      }
    }

    return allCategories
      .map((cat) => ({ category: cat, items: groups.get(cat) ?? [] }))
      .filter((group) => group.items.length > 0);
  }, [entries]);

  const totalTypes = entries.length;
  const categoryCount = grouped.length;

  if (!isTauriRuntime) {
    return (
      <>
        <div
          className="panel__header panel__header--desktop window-safe-header"
          data-window-drag-region
        >
          <div>
            <h2>插件管理</h2>
          </div>
        </div>
        <div className="plugin-panel__empty">
          <p>浏览器预览模式下无法读取引擎节点注册表。</p>
          <p>请在 Tauri 桌面应用中查看已注册的节点类型插件。</p>
        </div>
      </>
    );
  }

  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>插件管理</h2>
          <span className="panel__header-badge">
            共 {totalTypes} 个节点类型 · {categoryCount} 个分类
          </span>
        </div>
      </div>

      {isLoading && (
        <div className="plugin-panel__loading">
          <p>正在加载节点类型列表…</p>
        </div>
      )}

      {error && (
        <div className="plugin-panel__error">
          <p>加载失败: {error}</p>
        </div>
      )}

      {!isLoading && !error && (
        <div className="plugin-panel__groups">
          {grouped.map((group) => (
            <div key={group.category} className="plugin-panel__group">
              <h3 className="plugin-panel__group-title">{group.category}</h3>
              <div className="plugin-panel__grid">
                {group.items.map((item) => (
                  <div key={item.name} className="plugin-panel__card">
                    <div className="plugin-panel__card-name">{item.name}</div>
                    {item.description && (
                      <div className="plugin-panel__card-desc">
                        {item.description}
                      </div>
                    )}
                    {item.capabilities.length > 0 && (
                      <div className="plugin-panel__card-caps">
                        {item.capabilities.map((cap) => (
                          <span
                            key={cap}
                            className={`plugin-panel__cap plugin-panel__cap--${cap.toLowerCase()}`}
                            title={cap}
                          >
                            {NODE_CAPABILITY_LABELS[cap]}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
