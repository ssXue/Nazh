import { useCallback, useState } from 'react';

import {
  compileWorkflowDsl,
  loadCompilerAssetSnapshot,
  hasTauriRuntime,
} from '../lib/tauri';
import type {
  CompileWorkflowResponse,
  CompilerAssetSnapshot,
  DiagnosticItem,
} from '../lib/tauri';

export { type DiagnosticItem, type CompileWorkflowResponse, type CompilerAssetSnapshot };

export function useDslEditor() {
  const [yamlText, setYamlText] = useState('');
  const [compiling, setCompiling] = useState(false);
  const [compileResult, setCompileResult] = useState<CompileWorkflowResponse | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticItem[]>([]);
  const [assetSnapshot, setAssetSnapshot] = useState<CompilerAssetSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);

  const compile = useCallback(async (yaml?: string) => {
    const text = yaml ?? yamlText;
    if (!text.trim() || !hasTauriRuntime()) return;

    setCompiling(true);
    setError(null);
    try {
      const result = await compileWorkflowDsl({ workflowYaml: text });
      setCompileResult(result);
      setDiagnostics(result.diagnostics);
      if (result.error) {
        setError(result.error);
      }
    } catch (err) {
      setError(`编译失败: ${err}`);
    } finally {
      setCompiling(false);
    }
  }, [yamlText]);

  const loadSnapshot = useCallback(async () => {
    if (!hasTauriRuntime()) return;
    try {
      const snapshot = await loadCompilerAssetSnapshot();
      setAssetSnapshot(snapshot);
    } catch {
      // 资产快照加载失败不阻塞编辑器
    }
  }, []);

  return {
    yamlText,
    setYamlText,
    compiling,
    compileResult,
    diagnostics,
    assetSnapshot,
    error,
    compile,
    loadSnapshot,
  };
}
