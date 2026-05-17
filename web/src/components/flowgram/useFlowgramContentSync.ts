import {
  WorkflowContentChangeType,
  type FreeLayoutPluginContext,
  type WorkflowContentChangeEvent,
} from '@flowgram.ai/free-layout-editor';
import {
  useCallback,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';

import {
  configToRecord,
  invalidateNodePinSchema,
  refreshNodePinSchema,
} from '../../lib/pin-schema-cache';
import type { WorkflowGraph } from '../../types';

interface UseFlowgramContentSyncOptions {
  applyingExternalGraphRef: MutableRefObject<boolean>;
  latestGraphRef: MutableRefObject<WorkflowGraph | null>;
  syncTimerRef: MutableRefObject<number | null>;
  setLastChange: Dispatch<SetStateAction<string | null>>;
  syncSelectionState: (ctx: FreeLayoutPluginContext | null) => void;
  emitCurrentGraphChange: (ctx: FreeLayoutPluginContext) => string | null;
  reportFlowgramError: (title: string, error: unknown) => void;
}

export function useFlowgramContentSync({
  applyingExternalGraphRef,
  latestGraphRef,
  syncTimerRef,
  setLastChange,
  syncSelectionState,
  emitCurrentGraphChange,
  reportFlowgramError,
}: UseFlowgramContentSyncOptions) {
  return useCallback(
    (ctx: FreeLayoutPluginContext, event: WorkflowContentChangeEvent) => {
      try {
        if (applyingExternalGraphRef.current) {
          return;
        }

        if (event.type === WorkflowContentChangeType.META_CHANGE) {
          return;
        }

        // 节点生命周期事件触发 pin schema 缓存维护。部署期校验作为 backstop 兜底。
        if (
          event.type === WorkflowContentChangeType.ADD_NODE ||
          event.type === WorkflowContentChangeType.NODE_DATA_CHANGE
        ) {
          const entity = event.entity as { id?: string; getExtInfo?: () => unknown } | undefined;
          if (entity?.id && entity.getExtInfo) {
            const ext = (entity.getExtInfo() ?? {}) as {
              nodeType?: string;
              config?: unknown;
            };
            if (ext.nodeType) {
              void refreshNodePinSchema(
                entity.id,
                ext.nodeType,
                configToRecord(ext.config as never),
              );
            }
          }
        } else if (event.type === WorkflowContentChangeType.DELETE_NODE) {
          const entity = event.entity as { id?: string } | undefined;
          if (entity?.id) {
            invalidateNodePinSchema(entity.id);
          }
        }

        if (
          event.type === WorkflowContentChangeType.DELETE_NODE ||
          event.type === WorkflowContentChangeType.DELETE_LINE
        ) {
          ctx.playground.flush();
        }

        window.setTimeout(() => {
          setLastChange(event.type);
          syncSelectionState(ctx);

          if (!latestGraphRef.current) {
            return;
          }

          if (syncTimerRef.current !== null) {
            window.clearTimeout(syncTimerRef.current);
            syncTimerRef.current = null;
          }

          syncTimerRef.current = window.setTimeout(() => {
            emitCurrentGraphChange(ctx);
          }, 120);
        }, 0);
      } catch (error) {
        reportFlowgramError('FlowGram 内容同步失败', error);
      }
    },
    [
      applyingExternalGraphRef,
      emitCurrentGraphChange,
      latestGraphRef,
      reportFlowgramError,
      setLastChange,
      syncSelectionState,
      syncTimerRef,
    ],
  );
}
