import { useCallback, useEffect, useRef, useState } from 'react';

import {
  type ScopedWorkflowEvent,
  hasTauriRuntime,
  listPendingApprovals,
  onWorkflowEvent,
  respondHumanLoop,
} from '../../lib/tauri';
import { ApprovalForm } from './ApprovalForm';

interface PendingItem {
  approvalId: string;
  nodeId: string;
  nodeLabel: string;
  formSchema: unknown[];
  pendingSince: string;
  timeoutMs: number | null;
}

function normalizeFormSchema(raw: unknown): unknown[] {
  if (!Array.isArray(raw)) return [];
  return raw;
}

function parsePendingList(raw: unknown[]): PendingItem[] {
  return raw
    .map((item): PendingItem | null => {
      if (typeof item !== 'object' || item === null) return null;
      const obj = item as Record<string, unknown>;
      return {
        approvalId: String(obj.approvalId ?? ''),
        nodeId: String(obj.nodeId ?? ''),
        nodeLabel: String(obj.nodeLabel || (obj.nodeId ?? '')),
        formSchema: normalizeFormSchema(obj.formSchema),
        pendingSince: String(obj.pendingSince ?? ''),
        timeoutMs: typeof obj.timeoutMs === 'number' ? obj.timeoutMs : null,
      };
    })
    .filter((x): x is PendingItem => x !== null && x.approvalId !== '');
}

/** workflow://node-status 事件中的 ExecutionEvent 枚举。 */
type ExecutionEventShape =
  | { Started: { stage: string; trace_id: string } }
  | { Completed: { stage: string; trace_id: string } }
  | { Failed: { stage: string; trace_id: string; error: string } }
  | Record<string, unknown>;

function isStartedForHitlNode(event: ExecutionEventShape): boolean {
  if ('Started' in event && typeof event.Started === 'object' && event.Started !== null) {
    // stage = node_id；无法直接判断是否 humanLoop 节点，
    // 所以每次 Started 都触发 reload。
    return true;
  }
  // Completed/Failed 也触发 reload（审批被处理后会移除 pending item）
  return 'Completed' in event || 'Failed' in event;
}

export function ApprovalOverlay() {
  const [items, setItems] = useState<PendingItem[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState<string | null>(null);
  const reloadingRef = useRef(false);

  const reload = useCallback(async () => {
    if (!hasTauriRuntime() || reloadingRef.current) return;
    reloadingRef.current = true;
    try {
      const raw = await listPendingApprovals();
      const pending = parsePendingList(raw);
      setItems(pending);
      if (pending.length > 0 && !pending.some((i) => i.approvalId === expandedId)) {
        setExpandedId(pending[0].approvalId);
      }
      if (pending.length === 0) {
        setExpandedId(null);
      }
    } catch {
      // ignore
    } finally {
      reloadingRef.current = false;
    }
  }, [expandedId]);

  // 初始加载 + 定时轮询（兜底）
  useEffect(() => {
    void reload();
    const interval = setInterval(() => void reload(), 3000);
    return () => clearInterval(interval);
  }, [reload]);

  // 监听 workflow 事件即时 reload
  useEffect(() => {
    if (!hasTauriRuntime()) return;
    let disposed = false;
    const cleanup = onWorkflowEvent((payload: ScopedWorkflowEvent) => {
      const event = payload.event as ExecutionEventShape;
      if (!disposed && isStartedForHitlNode(event)) {
        // 稍微延迟让 registry create_slot 完成
        setTimeout(() => void reload(), 200);
      }
    });
    return () => {
      disposed = true;
      cleanup.then((fn) => fn());
    };
  }, [reload]);

  const handleSubmit = useCallback(
    async (approvalId: string, formData: Record<string, unknown>, comment: string) => {
      setSubmitting(approvalId);
      try {
        await respondHumanLoop({
          approvalId,
          action: 'approved',
          formData,
          comment: comment || null,
        });
        setItems((prev) => prev.filter((i) => i.approvalId !== approvalId));
      } catch (error) {
        console.error('审批响应失败:', error);
      } finally {
        setSubmitting(null);
      }
    },
    [],
  );

  const handleReject = useCallback(
    async (approvalId: string, comment: string) => {
      setSubmitting(approvalId);
      try {
        await respondHumanLoop({
          approvalId,
          action: 'rejected',
          formData: {},
          comment: comment || null,
        });
        setItems((prev) => prev.filter((i) => i.approvalId !== approvalId));
      } catch (error) {
        console.error('审批拒绝失败:', error);
      } finally {
        setSubmitting(null);
      }
    },
    [],
  );

  if (items.length === 0) return null;

  const current = items.find((i) => i.approvalId === expandedId) ?? items[0];

  return (
    <div className="approval-overlay">
      <div className="approval-overlay__backdrop" />
      <div className="approval-overlay__card">
        <div className="approval-overlay__header">
          <span className="approval-overlay__badge">{items.length}</span>
          <strong>{current.nodeLabel || current.nodeId}</strong>
          <span className="approval-overlay__time">
            {new Date(current.pendingSince).toLocaleTimeString()}
          </span>
          {items.length > 1 && (
            <span className="approval-overlay__queue">+{items.length - 1}</span>
          )}
        </div>
        <ApprovalForm
          formSchema={normalizeFormSchema(current.formSchema) as Array<{
            type: string;
            name: string;
            label: string;
            default?: unknown;
          }>}
          onSubmit={(fd, c) => handleSubmit(current.approvalId, fd, c)}
          onReject={(c) => handleReject(current.approvalId, c)}
          disabled={submitting === current.approvalId}
        />
      </div>
    </div>
  );
}
