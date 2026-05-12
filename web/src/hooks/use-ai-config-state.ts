import { useEffect, useState } from 'react';

import { loadApiKey } from '../ai/api-key';
import { testProviderConnection } from '../ai/test-connection';
import { hasTauriRuntime, loadAiConfig, saveAiConfig } from '../lib/tauri';
import { describeUnknownError } from '../lib/workflow-events';
import type { AiConfigUpdate, AiConfigView, AiProviderDraft, AiTestResult } from '../types';

export function useAiConfigState() {
  const [aiConfig, setAiConfig] = useState<AiConfigView | null>(null);
  const [aiConfigLoading, setAiConfigLoading] = useState(true);
  const [aiConfigError, setAiConfigError] = useState<string | null>(null);
  const [aiTestResult, setAiTestResult] = useState<AiTestResult | null>(null);
  const [aiTesting, setAiTesting] = useState(false);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setAiConfigLoading(false);
      return;
    }

    let cancelled = false;

    const load = async () => {
      try {
        const config = await loadAiConfig();
        if (!cancelled) {
          setAiConfig(config);
          setAiConfigError(null);
        }
      } catch (error) {
        if (!cancelled) {
          setAiConfigError(describeUnknownError(error).message);
        }
      } finally {
        if (!cancelled) {
          setAiConfigLoading(false);
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleAiConfigSave = async (update: AiConfigUpdate) => {
    try {
      const saved = await saveAiConfig(update);
      setAiConfig(saved);
      setAiConfigError(null);
    } catch (error) {
      setAiConfigError(describeUnknownError(error).message);
    }
  };

  const handleAiProviderTest = async (draft: AiProviderDraft) => {
    console.log('[ai-test] 收到 draft:', { id: draft.id, name: draft.name, hasApiKey: !!draft.apiKey, apiKeyLen: draft.apiKey?.length });
    setAiTesting(true);
    setAiTestResult(null);
    try {
      // keep 模式下 draft.apiKey 为空，需从后端补回已存 key
      let resolvedDraft = draft;
      if (!resolvedDraft.apiKey && resolvedDraft.id) {
        console.log('[ai-test] apiKey 为空，尝试从后端加载, providerId:', resolvedDraft.id);
        const savedKey = await loadApiKey(resolvedDraft.id);
        console.log('[ai-test] 后端返回 key 长度:', savedKey.length);
        if (savedKey) {
          resolvedDraft = { ...resolvedDraft, apiKey: savedKey };
        }
      }
      console.log('[ai-test] 最终 draft:', { id: resolvedDraft.id, hasApiKey: !!resolvedDraft.apiKey, apiKeyLen: resolvedDraft.apiKey?.length, baseUrl: resolvedDraft.baseUrl, model: resolvedDraft.defaultModel });
      const result = await testProviderConnection(resolvedDraft);
      console.log('[ai-test] 测试结果:', result);
      setAiTestResult(result);
    } catch (error) {
      console.error('[ai-test] 异常:', error);
      setAiTestResult({
        success: false,
        message: describeUnknownError(error).message,
      });
    } finally {
      setAiTesting(false);
    }
  };

  return {
    aiConfig,
    aiConfigError,
    aiConfigLoading,
    aiTesting,
    aiTestResult,
    handleAiConfigSave,
    handleAiProviderTest,
  };
}
