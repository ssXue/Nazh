import { useCallback, useState } from 'react';

import {
  aiGenerateWorkflowDsl,
  aiGenerateWorkflowDslStream,
  compileWorkflowDsl,
  hasTauriRuntime,
  loadCompilerAssetSnapshot,
} from '../lib/tauri';
import { listen } from '@tauri-apps/api/event';
import type {
  AiWorkflowDslProposal,
  CompileWorkflowResponse,
  CompilerAssetSnapshot,
  UncertaintyItem,
} from '../lib/tauri';

export type DslOrchestrationPhase =
  | 'idle'
  | 'generating'
  | 'streaming'
  | 'compiling'
  | 'ready'
  | 'error';

export interface DslOrchestrationState {
  phase: DslOrchestrationPhase;
  goal: string;
  proposedYaml: string;
  compiledGraph: Record<string, unknown> | null;
  compileError: string | null;
  uncertainties: UncertaintyItem[];
  warnings: string[];
  assetSnapshot: CompilerAssetSnapshot | null;
  streamingText: string;
  error: string | null;
}

const INITIAL_STATE: DslOrchestrationState = {
  phase: 'idle',
  goal: '',
  proposedYaml: '',
  compiledGraph: null,
  compileError: null,
  uncertainties: [],
  warnings: [],
  assetSnapshot: null,
  streamingText: '',
  error: null,
};

function createStreamId(): string {
  const randomId = globalThis.crypto?.randomUUID?.();
  if (typeof randomId === 'string' && randomId.trim()) {
    return randomId;
  }
  return `orch-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function useDslOrchestrator() {
  const [state, setState] = useState<DslOrchestrationState>(INITIAL_STATE);

  const loadAssets = useCallback(async () => {
    if (!hasTauriRuntime()) return;
    try {
      const snapshot = await loadCompilerAssetSnapshot();
      setState((prev) => ({ ...prev, assetSnapshot: snapshot }));
    } catch {
      // 资产加载失败不阻塞编排
    }
  }, []);

  const generate = useCallback(async (goal: string, providerId?: string | null) => {
    if (!hasTauriRuntime() || !goal.trim()) return;

    setState((prev) => ({
      ...prev,
      phase: 'generating',
      goal,
      proposedYaml: '',
      compiledGraph: null,
      compileError: null,
      uncertainties: [],
      warnings: [],
      error: null,
    }));

    try {
      const proposal: AiWorkflowDslProposal = await aiGenerateWorkflowDsl(goal, providerId);
      setState((prev) => ({
        ...prev,
        phase: proposal.compileResult?.error ? 'error' : 'ready',
        proposedYaml: proposal.workflowYaml,
        compiledGraph: proposal.compileResult?.graphJson ?? null,
        compileError: proposal.compileResult?.error ?? null,
        uncertainties: proposal.uncertainties,
        warnings: proposal.warnings,
      }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        phase: 'error',
        error: `AI 生成失败: ${err}`,
      }));
    }
  }, []);

  const generateStream = useCallback(async (goal: string, providerId?: string | null) => {
    if (!hasTauriRuntime() || !goal.trim()) return;

    async function handleStreamDone(text: string): Promise<void> {
      try {
        const trimmed = text.trim();
        let jsonText = trimmed;
        if (trimmed.startsWith('```json')) {
          jsonText = trimmed.slice(7);
          if (jsonText.endsWith('```')) {
            jsonText = jsonText.slice(0, -3);
          }
          jsonText = jsonText.trim();
        } else if (trimmed.startsWith('```')) {
          jsonText = trimmed.slice(3);
          if (jsonText.endsWith('```')) {
            jsonText = jsonText.slice(0, -3);
          }
          jsonText = jsonText.trim();
        }

        const parsed = JSON.parse(jsonText) as {
          workflowYaml?: string;
          workflow_yaml?: string;
          uncertainties?: UncertaintyItem[];
          warnings?: string[];
        };
        const yaml = parsed.workflowYaml ?? parsed.workflow_yaml ?? text;
        setState((prev) => ({
          ...prev,
          phase: 'compiling',
          proposedYaml: yaml,
          streamingText: '',
        }));

        const compileResult: CompileWorkflowResponse = await compileWorkflowDsl({
          workflowYaml: yaml,
        });
        setState((prev) => ({
          ...prev,
          phase: compileResult.error ? 'error' : 'ready',
          compiledGraph: compileResult.graphJson,
          compileError: compileResult.error,
          uncertainties: parsed.uncertainties ?? [],
          warnings: parsed.warnings ?? [],
        }));
      } catch {
        // JSON 解析失败，直接当作 YAML 处理
        setState((prev) => ({
          ...prev,
          phase: 'compiling',
          proposedYaml: text,
          streamingText: '',
        }));

        try {
          const compileResult = await compileWorkflowDsl({ workflowYaml: text });
          setState((prev) => ({
            ...prev,
            phase: compileResult.error ? 'error' : 'ready',
            compiledGraph: compileResult.graphJson,
            compileError: compileResult.error,
          }));
        } catch (err) {
          setState((prev) => ({
            ...prev,
            phase: 'error',
            compileError: `编译失败: ${err}`,
          }));
        }
      }
    }

    const streamId = createStreamId();
    const eventName = `copilot://stream/${streamId}`;

    setState((prev) => ({
      ...prev,
      phase: 'streaming',
      goal,
      proposedYaml: '',
      compiledGraph: null,
      compileError: null,
      uncertainties: [],
      warnings: [],
      streamingText: '',
      error: null,
    }));

    let accumulated = '';
    let stopListening: (() => void) | null = null;

    try {
      stopListening = await listen<{
        delta?: string;
        text?: string;
        done?: boolean;
        error?: string;
      }>(eventName, (event) => {
        const payload = event.payload;
        if (payload.error) {
          setState((prev) => ({ ...prev, phase: 'error', error: payload.error ?? '流式输出错误' }));
          return;
        }
        if (payload.delta) {
          accumulated += payload.delta;
        } else if (payload.text) {
          accumulated = payload.text;
        }
        setState((prev) => ({ ...prev, streamingText: accumulated }));

        if (payload.done) {
          // 流结束，解析并编译
          void handleStreamDone(accumulated);
        }
      });

      await aiGenerateWorkflowDslStream(goal, providerId ?? null, streamId);
    } catch (err) {
      setState((prev) => ({
        ...prev,
        phase: 'error',
        error: `流式生成失败: ${err}`,
      }));
    } finally {
      stopListening?.();
    }
  }, []);

  const compileProposed = useCallback(async (yaml?: string) => {
    const text = yaml ?? state.proposedYaml;
    if (!text.trim()) return;

    setState((prev) => ({ ...prev, phase: 'compiling', compileError: null }));
    try {
      const result = await compileWorkflowDsl({ workflowYaml: text });
      setState((prev) => ({
        ...prev,
        phase: result.error ? 'error' : 'ready',
        proposedYaml: text,
        compiledGraph: result.graphJson,
        compileError: result.error,
      }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        phase: 'error',
        compileError: `编译失败: ${err}`,
      }));
    }
  }, [state.proposedYaml]);

  const reset = useCallback(() => {
    setState(INITIAL_STATE);
  }, []);

  return {
    state,
    loadAssets,
    generate,
    generateStream,
    compileProposed,
    reset,
  };
}
