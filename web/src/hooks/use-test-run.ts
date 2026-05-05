//! 单次运行测试（stateless test run）编排 hook。
//!
//! 封装"一键部署→分发→等完成→自动反部署"全流程。
//! 与持久部署互斥：已部署时不可测试，测试运行按钮 disabled。
//!
//! 完成检测采用两阶段策略：
//! - Phase 1: isTestRunning=true 时注册 Tauri 事件监听，缓冲所有事件
//!   （必须在 dispatch 之前完成注册，Phase 1 通过 React effect 在
//!    setIsTestRunning(true) 触发的 re-render 中运行，先于 await dispatch）
//! - Phase 2: testRunTraceId 到达后处理缓冲 + 100ms 轮询新事件

import { useCallback, useEffect, useRef, useState } from 'react';

import { dispatchPayload, hasTauriRuntime, onWorkflowEvent } from '../lib/tauri';
import { parseWorkflowEventPayload } from '../lib/workflow-events';
import type { ParsedWorkflowEvent } from '../lib/workflow-events';
import type { AppErrorRecord, WorkflowResult } from '../types';

const TEST_RUN_TIMEOUT_MS = 30_000;
const POLL_INTERVAL_MS = 100;

export interface UseTestRunParams {
  getPayloadText: () => string;
  buildAndDeploy: () => Promise<boolean>;
  undeploy: () => Promise<void>;
  beginRestoreCheckPause: () => void;
  endRestoreCheckPause: () => void;
  appendRuntimeLog: (source: string, level: 'info' | 'success' | 'warn' | 'error', message: string, detail?: string | null) => void;
  appendAppError: (scope: AppErrorRecord['scope'], title: string, detail?: string | null) => void;
  setStatusMessage: (message: string) => void;
  clearResults: () => void;
  addPreviewResult: (payload: unknown) => void;
  getActiveBoardId: () => string | undefined;
  isCurrentlyDeployed: () => boolean;
  hasActiveBoard: () => boolean;
  hasActiveProject: () => boolean;
}

export interface UseTestRunResult {
  isTestRunning: boolean;
  canTestRun: boolean;
  handleTestRun: () => Promise<void>;
  /** 外部清理：部署/停止按钮点击时调用 */
  reset: () => void;
}

export function useTestRun(params: UseTestRunParams): UseTestRunResult {
  const [isTestRunning, setIsTestRunning] = useState(false);
  const [testRunTraceId, setTestRunTraceId] = useState<string | null>(null);
  const completedRef = useRef(false);
  const bufferRef = useRef<ParsedWorkflowEvent[]>([]);
  const undeployRef = useRef(params.undeploy);

  undeployRef.current = params.undeploy;

  const canTestRun = !isTestRunning && params.hasActiveBoard() && !params.isCurrentlyDeployed();

  const reset = useCallback(() => {
    completedRef.current = false;
    bufferRef.current = [];
    setTestRunTraceId(null);
    setIsTestRunning(false);
  }, []);

  // Phase 1: isTestRunning 变 true 时注册事件监听，缓冲所有事件
  useEffect(() => {
    if (!isTestRunning || !hasTauriRuntime()) return;

    bufferRef.current = [];
    let cleanup: (() => void) | null = null;

    void onWorkflowEvent((payload) => {
      const parsed = parseWorkflowEventPayload(payload.event);
      if (parsed) {
        bufferRef.current.push(parsed);
      }
    }).then((fn) => { cleanup = fn; });

    return () => { cleanup?.(); bufferRef.current = []; };
  }, [isTestRunning]);

  // Phase 2: testRunTraceId 到达后，处理缓冲 + 轮询完成
  useEffect(() => {
    if (!testRunTraceId || completedRef.current || !hasTauriRuntime()) return;

    let alive = true;
    const activeNodes = new Set<string>();
    let sawFailure = false;
    let sawAny = false;
    let cursor = 0;

    function finish(failed: boolean) {
      if (!alive) return;
      alive = false;
      completedRef.current = true;
      setTestRunTraceId(null);
      setIsTestRunning(false);
      params.appendRuntimeLog(
        'test-run',
        failed ? 'warn' : 'success',
        failed ? '测试运行节点失败，正在自动反部署' : '测试运行完成，正在自动反部署',
      );
      params.beginRestoreCheckPause();
      void undeployRef.current().finally(params.endRestoreCheckPause);
    }

    function drain() {
      const buf = bufferRef.current;
      while (cursor < buf.length) {
        const ev = buf[cursor++];
        if (!ev || ev.traceId !== testRunTraceId) continue;
        sawAny = true;
        switch (ev.kind) {
          case 'started': activeNodes.add(ev.nodeId); break;
          case 'completed':
          case 'output': activeNodes.delete(ev.nodeId); break;
          case 'failed': activeNodes.delete(ev.nodeId); sawFailure = true; break;
          case 'finished': activeNodes.clear(); break;
        }
      }
    }

    drain();
    if (sawFailure) { finish(true); return; }
    if (activeNodes.size === 0 && sawAny) { finish(false); return; }

    const poll = setInterval(() => {
      if (!alive) return;
      drain();
      if (sawFailure || (activeNodes.size === 0 && sawAny)) {
        clearInterval(poll);
        clearTimeout(timeout);
        finish(sawFailure);
      }
    }, POLL_INTERVAL_MS);

    const timeout = setTimeout(() => {
      if (!alive) return;
      clearInterval(poll);
      finish(true);
    }, TEST_RUN_TIMEOUT_MS);

    return () => { alive = false; clearInterval(poll); clearTimeout(timeout); };
  }, [testRunTraceId, params]);

  const handleTestRun = useCallback(async () => {
    if (!params.hasActiveBoard() || !params.hasActiveProject()) {
      params.setStatusMessage('请先从所有看板进入工程。');
      return;
    }
    if (params.isCurrentlyDeployed()) {
      params.setStatusMessage('请先停止当前部署，再进行测试运行。');
      return;
    }

    let payload: unknown;
    try {
      payload = JSON.parse(params.getPayloadText());
    } catch (error) {
      params.appendAppError(
        'command',
        '测试载荷 JSON 无法解析',
        error instanceof Error ? error.message : null,
      );
      params.setStatusMessage(
        error instanceof Error ? `Payload JSON 无法解析: ${error.message}` : 'Payload JSON 无法解析',
      );
      return;
    }

    if (!hasTauriRuntime()) {
      params.addPreviewResult(payload);
      params.setStatusMessage('已在纯 Web 预览模式下模拟测试运行。');
      params.appendRuntimeLog('system', 'info', '已在预览态模拟测试运行');
      return;
    }

    completedRef.current = false;
    params.clearResults();
    setIsTestRunning(true);

    try {
      const deployed = await params.buildAndDeploy();
      if (!deployed) {
        setIsTestRunning(false);
        return;
      }

      const boardId = params.getActiveBoardId();
      const response = await dispatchPayload(payload, boardId);
      const traceId = response.traceId;

      params.appendRuntimeLog('test-run', 'info', '测试运行已提交', `trace_id=${traceId}`);
      params.setStatusMessage(`测试运行中，trace_id=${traceId}`);
      setTestRunTraceId(traceId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      params.appendAppError('command', '测试运行失败', message);
      params.setStatusMessage(message);
      setIsTestRunning(false);
      if (params.isCurrentlyDeployed()) {
        await params.undeploy();
      }
    }
  }, [params]);

  return { isTestRunning, canTestRun, handleTestRun, reset };
}
