import { useCallback, useEffect, useState } from 'react';
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';

import { formatPinType } from '../../lib/pin-schema-cache';
import {
  deleteWorkflowVariable,
  onWorkflowVariableChanged,
  onWorkflowVariableDeleted,
  queryVariableHistory,
  resetWorkflowVariable,
  setWorkflowVariable,
  snapshotWorkflowVariables,
} from '../../lib/workflow-variables';
import type {
  HistoryEntryPayload,
  JsonValue,
  PinType,
  TypedVariableSnapshot,
  VariableChangedPayload,
  VariableDeletedPayload,
} from '../../generated';

interface RuntimeVariablesPanelProps {
  workflowId: string | null;
  onVariableCountChange?: (count: number) => void;
}

interface VariableEntry extends TypedVariableSnapshot {
  name: string;
}

/**
 * 运行时变量面板（ADR-0012 Phase 2 + Phase 3）。
 *
 * - 初始通过 `snapshotWorkflowVariables` 拉一次快照
 * - 订阅 `workflow://variable-changed` / `workflow://variable-deleted` 事件实时更新本地 state
 * - 编辑：按 `PinType.kind` 分派输入解析（bool / integer / float / string / json）
 * - 删除：调 `deleteWorkflowVariable`，引擎侧发 `VariableDeleted` 事件
 */
export function RuntimeVariablesPanel({ workflowId, onVariableCountChange }: RuntimeVariablesPanelProps) {
  const [variables, setVariables] = useState<Record<string, TypedVariableSnapshot>>({});
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!workflowId) {
      setVariables({});
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const response = await snapshotWorkflowVariables(workflowId);
      // 过滤掉值为 undefined 的条目（SnapshotWorkflowVariablesResponse.variables 字段为 optional record）
      const normalized: Record<string, TypedVariableSnapshot> = {};
      for (const [k, v] of Object.entries(response.variables ?? {})) {
        if (v !== undefined) normalized[k] = v;
      }
      setVariables(normalized);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [workflowId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!workflowId) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onWorkflowVariableChanged((payload: VariableChangedPayload) => {
      if (cancelled || payload.workflowId !== workflowId) return;
      setVariables((prev) => ({
        ...prev,
        [payload.name]: {
          value: payload.value,
          variableType: prev[payload.name]?.variableType ?? { kind: 'any' },
          initial: prev[payload.name]?.initial ?? payload.value,
          updatedAt: payload.updatedAt,
          updatedBy: payload.updatedBy,
        },
      }));
    }).then((u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [workflowId]);

  // ADR-0012 Phase 3：监听变量删除事件
  useEffect(() => {
    if (!workflowId) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onWorkflowVariableDeleted((payload: VariableDeletedPayload) => {
      if (cancelled || payload.workflowId !== workflowId) return;
      setVariables((prev) => {
        const next = { ...prev };
        delete next[payload.name];
        return next;
      });
    }).then((u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [workflowId]);

  // 向上报告变量数量（供 RuntimeDock badge 使用）
  useEffect(() => {
    onVariableCountChange?.(Object.keys(variables).length);
  }, [variables, onVariableCountChange]);

  const handleSet = useCallback(
    async (name: string, value: JsonValue) => {
      if (!workflowId) return;
      try {
        setError(null);
        await setWorkflowVariable({ workflowId, name, value });
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [workflowId],
  );

  const handleDelete = useCallback(
    async (name: string) => {
      if (!workflowId) return;
      try {
        setError(null);
        await deleteWorkflowVariable({ workflowId, name });
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [workflowId],
  );

  const handleReset = useCallback(
    async (name: string) => {
      if (!workflowId) return;
      try {
        setError(null);
        await resetWorkflowVariable({ workflowId, name });
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [workflowId],
  );

  const entries: VariableEntry[] = Object.entries(variables).map(([name, snapshot]) => ({
    name,
    ...snapshot,
  }));

  if (!workflowId) {
    return (
      <div className="runtime-variables-panel runtime-variables-panel--empty">
        未选中已部署的工作流
      </div>
    );
  }

  return (
    <div className="runtime-variables-panel">
      {error && <div className="runtime-variables-panel__error">{error}</div>}
      {isLoading && entries.length === 0 ? (
        <div className="runtime-variables-panel runtime-variables-panel--loading">加载中…</div>
      ) : entries.length === 0 ? (
        <div className="runtime-variables-panel runtime-variables-panel--empty">该工作流未声明变量</div>
      ) : (
        <ul className="runtime-variables-panel__list">
          {entries.map((entry) => (
            <VariableRow
              key={entry.name}
              workflowId={workflowId}
              entry={entry}
              onSubmit={handleSet}
              onDelete={handleDelete}
              onReset={handleReset}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

interface VariableRowProps {
  workflowId: string;
  entry: VariableEntry;
  onSubmit: (name: string, value: JsonValue) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
  onReset: (name: string) => Promise<void>;
}

function VariableRow({ workflowId, entry, onSubmit, onDelete, onReset }: VariableRowProps) {
  const [draft, setDraft] = useState<string>(JSON.stringify(entry.value));
  const [isEditing, setIsEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [parseError, setParseError] = useState<string | null>(null);
  const [showHistory, setShowHistory] = useState(false);
  const [historyEntries, setHistoryEntries] = useState<HistoryEntryPayload[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);

  // 外部事件更新 entry.value 时，非编辑态 draft 跟随，避免下次进入编辑看到过期值。
  useEffect(() => {
    if (!isEditing) {
      setDraft(JSON.stringify(entry.value));
    }
  }, [entry.value, isEditing]);

  const handleSubmit = async () => {
    let parsed: JsonValue;
    try {
      parsed = parseValueByPinType(draft, entry.variableType);
      setParseError(null);
    } catch (err) {
      setParseError(err instanceof Error ? err.message : String(err));
      return;
    }
    await onSubmit(entry.name, parsed);
    setIsEditing(false);
  };

  const handleToggleHistory = async () => {
    if (showHistory) {
      setShowHistory(false);
      return;
    }
    setShowHistory(true);
    setHistoryLoading(true);
    try {
      const response = await queryVariableHistory({
        workflowId,
        name: entry.name,
        limit: 50,
      });
      setHistoryEntries(response.entries);
    } catch {
      setHistoryEntries([]);
    } finally {
      setHistoryLoading(false);
    }
  };

  const isNumeric = entry.variableType.kind === 'integer' || entry.variableType.kind === 'float';
  const chartData = isNumeric
    ? historyEntries
        .filter((e) => typeof e.value === 'number')
        .reverse()
        .map((e) => ({
          time: new Date(e.updatedAt).toLocaleTimeString(),
          value: e.value as number,
        }))
    : [];

  return (
    <li className="runtime-variables-panel__row">
      <div className="runtime-variables-panel__name">{entry.name}</div>
      <div className="runtime-variables-panel__type">{formatPinType(entry.variableType)}</div>
      {!isEditing ? (
        <>
          <div className="runtime-variables-panel__value">{JSON.stringify(entry.value)}</div>
          <button
            type="button"
            onClick={() => {
              setDraft(JSON.stringify(entry.value));
              setIsEditing(true);
            }}
          >
            编辑
          </button>
          <button
            type="button"
            className="runtime-variables-panel__reset"
            onClick={() => void onReset(entry.name)}
          >
            重置
          </button>
          {!confirmDelete ? (
            <button
              type="button"
              className="runtime-variables-panel__delete"
              onClick={() => setConfirmDelete(true)}
            >
              删除
            </button>
          ) : (
            <span className="runtime-variables-panel__confirm-delete">
              确认删除？
              <button
                type="button"
                className="runtime-variables-panel__confirm-yes"
                onClick={() => {
                  setConfirmDelete(false);
                  void onDelete(entry.name);
                }}
              >
                是
              </button>
              <button
                type="button"
                className="runtime-variables-panel__confirm-no"
                onClick={() => setConfirmDelete(false)}
              >
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
          {parseError && (
            <span className="runtime-variables-panel__parse-error">{parseError}</span>
          )}
        </>
      )}
      <div className="runtime-variables-panel__meta">
        {entry.updatedBy ?? '-'} · {entry.updatedAt}
      </div>
      <button
        type="button"
        className="runtime-variables-panel__history-toggle"
        onClick={() => void handleToggleHistory()}
      >
        {showHistory ? '收起' : '历史'}
      </button>
      {showHistory && (
        <div className="runtime-variables-panel__history">
          {historyLoading ? (
            <span>加载中…</span>
          ) : historyEntries.length === 0 ? (
            <span>暂无历史记录</span>
          ) : isNumeric && chartData.length > 1 ? (
            <ResponsiveContainer width="100%" height={120}>
              <LineChart data={chartData}>
                <XAxis dataKey="time" tick={{ fontSize: 10 }} />
                <YAxis tick={{ fontSize: 10 }} width={40} />
                <Tooltip />
                <Line type="monotone" dataKey="value" stroke="#6366f1" dot={false} strokeWidth={1.5} />
              </LineChart>
            </ResponsiveContainer>
          ) : (
            <ul className="runtime-variables-panel__history-list">
              {historyEntries.map((h, i) => (
                <li key={i}>
                  <span className="runtime-variables-panel__history-value">{JSON.stringify(h.value)}</span>
                  <span className="runtime-variables-panel__history-time">
                    {new Date(h.updatedAt).toLocaleTimeString()}
                    {h.updatedBy ? ` · ${h.updatedBy}` : ''}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </li>
  );
}

function parseValueByPinType(raw: string, pinType: PinType): JsonValue {
  const trimmed = raw.trim();
  switch (pinType.kind) {
    case 'bool':
      if (trimmed === 'true') return true;
      if (trimmed === 'false') return false;
      throw new Error('期望 true / false');
    case 'integer': {
      if (trimmed === '') throw new Error('期望整数（不能为空）');
      const n = Number(trimmed);
      if (!Number.isInteger(n)) throw new Error('期望整数');
      return n;
    }
    case 'float': {
      if (trimmed === '') throw new Error('期望数字（不能为空）');
      const n = Number(trimmed);
      if (Number.isNaN(n)) throw new Error('期望数字');
      return n;
    }
    case 'string':
      return trimmed.startsWith('"') ? (JSON.parse(trimmed) as string) : trimmed;
    case 'json':
    case 'array':
    case 'binary':
    case 'any':
    case 'custom':
      if (trimmed === '') {
        throw new Error('期望有效 JSON 值（不能为空）');
      }
      return JSON.parse(trimmed) as JsonValue;
    default: {
      // 编译期保证：PinType 新增 kind 时此处报错，提示更新 parseValueByPinType
      const _exhaustive: never = pinType;
      throw new Error(`未知 PinType kind: ${(_exhaustive as { kind?: string }).kind ?? '<unknown>'}`);
    }
  }
}
