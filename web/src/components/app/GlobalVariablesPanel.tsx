import { useCallback, useEffect, useState } from 'react';

import {
  deleteGlobalVariable,
  listGlobalVariables,
  setGlobalVariable,
} from '../../lib/workflow-variables';
import type { GlobalVariableSnapshot } from '../../generated';

interface NamespaceGroup {
  namespace: string;
  variables: GlobalVariableSnapshot[];
}

/**
 * 全局变量面板（ADR-0012 Phase 3）。
 *
 * 按 namespace 分组展示所有全局变量，支持 CRUD 操作。
 * 全局变量不属于任何工作流，通过 namespace + key 唯一标识。
 */
export function GlobalVariablesPanel() {
  const [groups, setGroups] = useState<NamespaceGroup[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listGlobalVariables({});
      const grouped = groupByNamespace(response.variables);
      setGroups(grouped);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleSet = useCallback(
    async (namespace: string, key: string, value: string) => {
      try {
        setError(null);
        let parsed: unknown;
        try {
          parsed = JSON.parse(value);
        } catch {
          parsed = value;
        }
        await setGlobalVariable({
          namespace,
          key,
          value: parsed as GlobalVariableSnapshot['value'],
        });
        await refresh();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [refresh],
  );

  const handleDelete = useCallback(
    async (namespace: string, key: string) => {
      try {
        setError(null);
        await deleteGlobalVariable({ namespace, key });
        await refresh();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [refresh],
  );

  return (
    <div className="global-variables-panel">
      {error && <div className="global-variables-panel__error">{error}</div>}
      <div className="global-variables-panel__toolbar">
        <AddGlobalVariableForm onAdd={handleSet} />
      </div>
      {isLoading && groups.length === 0 ? (
        <div className="global-variables-panel global-variables-panel--loading">加载中…</div>
      ) : groups.length === 0 ? (
        <div className="global-variables-panel global-variables-panel--empty">暂无全局变量</div>
      ) : (
        groups.map((group) => (
          <div key={group.namespace} className="global-variables-panel__namespace">
            <h4 className="global-variables-panel__namespace-title">{group.namespace}</h4>
            <ul className="global-variables-panel__list">
              {group.variables.map((v) => (
                <GlobalVariableRow
                  key={`${v.namespace}:${v.key}`}
                  variable={v}
                  onSet={handleSet}
                  onDelete={handleDelete}
                />
              ))}
            </ul>
          </div>
        ))
      )}
    </div>
  );
}

interface GlobalVariableRowProps {
  variable: GlobalVariableSnapshot;
  onSet: (namespace: string, key: string, value: string) => Promise<void>;
  onDelete: (namespace: string, key: string) => Promise<void>;
}

function GlobalVariableRow({ variable, onSet, onDelete }: GlobalVariableRowProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(JSON.stringify(variable.value));
  const [confirmDelete, setConfirmDelete] = useState(false);

  const handleSubmit = async () => {
    await onSet(variable.namespace, variable.key, draft);
    setIsEditing(false);
  };

  return (
    <li className="global-variables-panel__row">
      <div className="global-variables-panel__key">{variable.key}</div>
      <div className="global-variables-panel__type">{variable.varType}</div>
      {!isEditing ? (
        <>
          <div className="global-variables-panel__value">{JSON.stringify(variable.value)}</div>
          <button type="button" onClick={() => { setDraft(JSON.stringify(variable.value)); setIsEditing(true); }}>
            编辑
          </button>
          {!confirmDelete ? (
            <button
              type="button"
              className="global-variables-panel__delete"
              onClick={() => setConfirmDelete(true)}
            >
              删除
            </button>
          ) : (
            <span className="global-variables-panel__confirm-delete">
              确认删除？
              <button type="button" onClick={() => { setConfirmDelete(false); void onDelete(variable.namespace, variable.key); }}>
                是
              </button>
              <button type="button" onClick={() => setConfirmDelete(false)}>
                否
              </button>
            </span>
          )}
        </>
      ) : (
        <>
          <input
            value={draft}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onBlur={() => setIsEditing(false)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleSubmit();
              if (e.key === 'Escape') setIsEditing(false);
            }}
            autoFocus
          />
        </>
      )}
      <div className="global-variables-panel__meta">
        {variable.updatedBy ?? '-'} · {variable.updatedAt}
      </div>
    </li>
  );
}

interface AddGlobalVariableFormProps {
  onAdd: (namespace: string, key: string, value: string) => Promise<void>;
}

function AddGlobalVariableForm({ onAdd }: AddGlobalVariableFormProps) {
  const [namespace, setNamespace] = useState('default');
  const [key, setKey] = useState('');
  const [value, setValue] = useState('""');
  const [isExpanded, setIsExpanded] = useState(false);

  const handleAdd = async () => {
    if (!key.trim()) return;
    await onAdd(namespace.trim(), key.trim(), value);
    setKey('');
    setValue('""');
    setIsExpanded(false);
  };

  if (!isExpanded) {
    return (
      <button type="button" className="global-variables-panel__add-btn" onClick={() => setIsExpanded(true)}>
        + 添加全局变量
      </button>
    );
  }

  return (
    <div className="global-variables-panel__add-form">
      <input
        placeholder="命名空间"
        value={namespace}
        onChange={(e) => setNamespace(e.currentTarget.value)}
      />
      <input
        placeholder="变量名"
        value={key}
        onChange={(e) => setKey(e.currentTarget.value)}
        autoFocus
      />
      <input
        placeholder="值（JSON）"
        value={value}
        onChange={(e) => setValue(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void handleAdd();
          if (e.key === 'Escape') setIsExpanded(false);
        }}
      />
      <button type="button" onClick={() => void handleAdd()} disabled={!key.trim()}>
        添加
      </button>
      <button type="button" onClick={() => setIsExpanded(false)}>
        取消
      </button>
    </div>
  );
}

function groupByNamespace(variables: GlobalVariableSnapshot[]): NamespaceGroup[] {
  const map = new Map<string, GlobalVariableSnapshot[]>();
  for (const v of variables) {
    const existing = map.get(v.namespace);
    if (existing) {
      existing.push(v);
    } else {
      map.set(v.namespace, [v]);
    }
  }
  return Array.from(map.entries())
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([namespace, vars]) => ({ namespace, variables: vars }));
}
