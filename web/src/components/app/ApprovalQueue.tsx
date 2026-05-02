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

/** 检查 workflow://node-status 事件是否为 HITL 审批 pending/resolved。 */
function isHitlStartedEvent(event: unknown): { nodeId: string; approvalId: string; formSchema: unknown } | null {
  if (typeof event !== 'object' || event === null) return null;
  const obj = event as Record<string, unknown>;
  // ExecutionEvent::Started { stage, trace_id }
  const stage = obj.stage ?? obj.Started;
  // 不够——Started 事件不含 approval_id。
  // 用 Completed 事件的 metadata.human_loop 判断更可靠。
  // 但审批 pending 时节点还在 await，不会发 Completed。
  //
  // 最简方案：ApprovalQueue 依赖 listPendingApprovals 轮询/初始化加载，
  // onWorkflowEvent 仅用于感知"有变更发生"时触发重新加载。
  return null;
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
    const cleanup = onWorkflowEvent((_payload: ScopedWorkflowEvent) => {
      // 任何节点状态变化都触发重新加载——简单但可靠
      // TODO: 可优化为仅 humanLoop 节点事件触发
      if (!disposed) {
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
      <div style={{ padding: 12, color: '#666', fontSize: 12, textAlign: 'center' }}>
        暂无待处理审批
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      {pendingItems.map((item) => (
        <div
          key={item.approvalId}
          style={{
            background: '#1a1a2e',
            border: '1px solid #333',
            borderRadius: 6,
            overflow: 'hidden',
          }}
        >
          <button
            onClick={() => toggleExpand(item.approvalId)}
            style={{
              width: '100%',
              padding: '8px 12px',
              background: 'transparent',
              border: 'none',
              color: '#ddd',
              cursor: 'pointer',
              textAlign: 'left',
              fontSize: 12,
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
            }}
          >
            <span>
              <span style={{ color: '#f0ad4e', fontWeight: 600 }}>{item.nodeLabel}</span>
              <span style={{ color: '#666', marginLeft: 8 }}>
                {new Date(item.pendingSince).toLocaleTimeString()}
              </span>
            </span>
            <span style={{ color: '#666' }}>{item.expanded ? '▼' : '▶'}</span>
          </button>
          {item.expanded && (
            <div style={{ padding: '0 12px 12px' }}>
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
