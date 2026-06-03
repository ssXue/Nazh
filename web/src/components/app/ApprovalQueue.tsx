import { useCallback, useEffect, useState } from 'react';
import {
  type ScopedWorkflowEvent,
  listPendingApprovals,
  onWorkflowEvent,
  respondHumanLoop,
  hasTauriRuntime,
} from '../../lib/tauri';
import { ApprovalForm } from './ApprovalForm';

interface FormField {
  type: string;
  name: string;
  label: string;
  required?: boolean;
  default?: unknown;
  min?: number;
  max?: number;
  unit?: string;
  multiline?: boolean;
  maxLength?: number;
  options?: Array<{ value: string; label: string }>;
}

interface PendingItem {
  approvalId: string;
  nodeId: string;
  nodeLabel: string;
  formSchema: FormField[];
  pendingSince: string;
  timeoutMs: number | null;
  expanded: boolean;
}

function normalizeFormSchema(raw: unknown): FormField[] {
  if (!Array.isArray(raw)) return [];
  return raw as FormField[];
}

/** 从 listPendingApprovals 返回的 unknown[] 解析为 PendingItem[]。 */
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
        expanded: false,
      };
    })
    .filter((x): x is PendingItem => x !== null && x.approvalId !== '');
}

/** 检查 workflow://node-status 事件是否可能影响 HITL 审批列表。 */
function isHitlRelevantEvent(event: unknown): boolean {
  if (typeof event !== 'object' || event === null) return false;
  const obj = event as Record<string, unknown>;
  // EdgeTransmitSummary 和 BackpressureDetected 高频发射，不影响审批列表。
  // 只在 Started / Completed / Failed / Output / Finished 时重新加载。
  return (
    'Started' in obj ||
    'Completed' in obj ||
    'Failed' in obj ||
    'Output' in obj ||
    'Finished' in obj
  );
}

export function ApprovalQueue() {
  const [pendingItems, setPendingItems] = useState<PendingItem[]>([]);
  const [submitting, setSubmitting] = useState<string | null>(null);

  const reloadPending = useCallback(async () => {
    if (!hasTauriRuntime()) return;
    try {
      const raw = await listPendingApprovals();
      setPendingItems(parsePendingList(raw));
    } catch (error) {
      console.error('加载审批列表失败:', error);
    }
  }, []);

  // 初始加载
  useEffect(() => {
    void reloadPending();
  }, [reloadPending]);

  // 监听 workflow://node-status 事件，当有 HITL 节点活动时重新加载列表
  useEffect(() => {
    if (!hasTauriRuntime()) return;
    let disposed = false;

    const cleanup = onWorkflowEvent((payload: ScopedWorkflowEvent) => {
      // 过滤高频可观测性事件，仅在节点状态变更时重新加载审批列表
      if (!disposed && isHitlRelevantEvent(payload.event)) {
        void reloadPending();
      }
    });

    return () => {
      disposed = true;
      cleanup.then((fn) => fn());
    };
  }, [reloadPending]);

  const toggleExpand = useCallback((approvalId: string) => {
    setPendingItems((prev) =>
      prev.map((item) =>
        item.approvalId === approvalId ? { ...item, expanded: !item.expanded } : item,
      ),
    );
  }, []);

  const handleSubmit = useCallback(async (approvalId: string, formData: Record<string, unknown>, comment: string) => {
    setSubmitting(approvalId);
    try {
      await respondHumanLoop({
        approvalId,
        action: 'approved',
        formData,
        comment: comment || null,
      });
      void reloadPending();
    } catch (error) {
      console.error('审批响应失败:', error);
    } finally {
      setSubmitting(null);
    }
  }, [reloadPending]);

  const handleReject = useCallback(async (approvalId: string, comment: string) => {
    setSubmitting(approvalId);
    try {
      await respondHumanLoop({
        approvalId,
        action: 'rejected',
        formData: {},
        comment: comment || null,
      });
      void reloadPending();
    } catch (error) {
      console.error('审批拒绝失败:', error);
    } finally {
      setSubmitting(null);
    }
  }, [reloadPending]);

  if (pendingItems.length === 0) {
    return (
      <div className="approval-queue__empty">
        暂无待处理审批
      </div>
    );
  }

  return (
    <div className="approval-queue__list">
      {pendingItems.map((item) => (
        <div
          key={item.approvalId}
          className="approval-queue__item"
        >
          <button
            onClick={() => toggleExpand(item.approvalId)}
            className="approval-queue__header"
          >
            <span>
              <span className="approval-queue__node-label">{item.nodeLabel}</span>
              <span className="approval-queue__time">
                {new Date(item.pendingSince).toLocaleTimeString()}
              </span>
            </span>
            <span className="approval-queue__toggle">{item.expanded ? '▼' : '▶'}</span>
          </button>
          {item.expanded && (
            <div className="approval-queue__body">
              <ApprovalForm
                formSchema={item.formSchema}
                onSubmit={(fd, c) => handleSubmit(item.approvalId, fd, c)}
                onReject={(c) => handleReject(item.approvalId, c)}
                disabled={submitting === item.approvalId}
              />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
