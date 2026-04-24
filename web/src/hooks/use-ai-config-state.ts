import { useEffect, useState } from 'react';

import { hasTauriRuntime, loadAiConfig, saveAiConfig, testAiProvider } from '../lib/tauri';
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
    setAiTesting(true);
    setAiTestResult(null);
    try {
      const result = await testAiProvider(draft);
      setAiTestResult(result);
    } catch (error) {
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
