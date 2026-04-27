import { useCallback, useEffect, useRef, useState } from 'react';

import { formatPinType } from '../../lib/pin-schema-cache';
import {
  onWorkflowVariableChanged,
  setWorkflowVariable,
  snapshotWorkflowVariables,
} from '../../lib/workflow-variables';
import type {
  JsonValue,
  PinType,
  TypedVariableSnapshot,
  VariableChangedPayload,
} from '../../generated';

interface RuntimeVariablesPanelProps {
  workflowId: string | null;
}

interface VariableEntry extends TypedVariableSnapshot {
  name: string;
}

/**
 * 运行时变量面板（ADR-0012 Phase 2）。
 *
 * - 初始通过 `snapshotWorkflowVariables` 拉一次快照
 * - 订阅 `workflow://variable-changed` 事件实时更新本地 state
 * - 编辑：按 `PinType.kind` 分派输入解析（bool / integer / float / string / json）
 */
export function RuntimeVariablesPanel({ workflowId }: RuntimeVariablesPanelProps) {
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
            <VariableRow key={entry.name} entry={entry} onSubmit={handleSet} />
          ))}
        </ul>
      )}
    </div>
  );
}

interface VariableRowProps {
  entry: VariableEntry;
  onSubmit: (name: string, value: JsonValue) => Promise<void>;
}

function VariableRow({ entry, onSubmit }: VariableRowProps) {
  const [draft, setDraft] = useState<string>(JSON.stringify(entry.value));
  const [isEditing, setIsEditing] = useState(false);
  const [parseError, setParseError] = useState<string | null>(null);
  const isSubmittingRef = useRef(false);

  // 外部事件更新 entry.value 时，非编辑态 draft 跟随，避免下次进入编辑看到过期值。
  // 退出编辑态时重置双 submit 守卫，避免 onBlur+Enter 同时触发。
  useEffect(() => {
    if (!isEditing) {
      setDraft(JSON.stringify(entry.value));
      isSubmittingRef.current = false;
    }
  }, [entry.value, isEditing]);

  const handleSubmit = async () => {
    // Enter 触发 handleSubmit 后 setIsEditing(false) 会让 input blur，本守卫防止 onBlur 二次提交。
    if (isSubmittingRef.current) return;
    isSubmittingRef.current = true;
    let parsed: JsonValue;
    try {
      parsed = parseValueByPinType(draft, entry.variableType);
      setParseError(null);
    } catch (err) {
      setParseError(err instanceof Error ? err.message : String(err));
      isSubmittingRef.current = false; // 解析失败回退守卫
      return;
    }
    await onSubmit(entry.name, parsed);
    setIsEditing(false);
  };

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
        </>
      ) : (
        <>
          <input
            value={draft}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onBlur={() => void handleSubmit()}
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
