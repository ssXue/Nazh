import { useCallback, useState } from 'react';

import {
  extractDeviceFromText as aiExtractFromText,
  extractDeviceProposal as aiExtractProposal,
  extractDeviceProposalStream as aiExtractProposalStream,
  type ExtractionProposal as AiExtractionProposal,
} from '../ai/device-extraction';
import { hasTauriRuntime, loadAiConfig } from '../lib/tauri';
import { resolveGlobalAiProvider } from '../lib/workflow-ai';

/** 设备资产摘要中携带的连接引用（与 Rust DeviceConnectionRef 对齐）。 */
export interface DeviceConnectionRef {
  type: string;
  id: string;
  unit?: number;
}

/** 设备资产摘要。 */
export interface DeviceAssetSummary {
  id: string;
  name: string;
  device_type: string;
  version: number;
  updated_at: string;
  /** 设备绑定的连接资源（从 DSL connection 块派生）；未配置连接时为空。 */
  connection?: DeviceConnectionRef;
}

/** 设备资产详情。 */
export interface DeviceAssetDetail {
  id: string;
  name: string;
  device_type: string;
  version: number;
  spec_json: Record<string, unknown>;
  spec_yaml?: string;
  yaml_file_path?: string | null;
  created_at: string;
  updated_at: string;
}

/** Pin schema 条目。 */
export interface PinSchemaEntry {
  id: string;
  label: string;
  pin_type: string;
  direction: string;
  description: string | null;
}

/** AI 来源追溯记录。 */
export interface FieldSource {
  field_path: string;
  source_text: string;
  confidence: number;
}

/** AI 抽取的不确定项（RFC-0004 Phase 4A）。 */
export interface UncertaintyItem {
  fieldPath: string;
  guessedValue: string;
  reason: string;
}

/** 设备 + 能力的结构化抽取提案（RFC-0004 Phase 4A）。 */
export interface ExtractionProposal {
  deviceYamls: string[];
  capabilityYamls: string[];
  uncertainties: UncertaintyItem[];
  warnings: string[];
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
  return tauriInvoke<T>(command, args);
}

export function useDeviceAssets(workspacePath = '') {
  const [assets, setAssets] = useState<DeviceAssetSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadAssets = useCallback(async () => {
    if (!hasTauriRuntime()) return;
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<DeviceAssetSummary[]>('list_device_assets', {
        workspacePath: workspacePath.trim() || null,
      });
      setAssets(list);
    } catch (err) {
      setError(`加载设备列表失败: ${err}`);
    } finally {
      setLoading(false);
    }
  }, [workspacePath]);

  const loadDetail = useCallback(async (id: string): Promise<DeviceAssetDetail | null> => {
    if (!hasTauriRuntime()) return null;
    return invoke<DeviceAssetDetail | null>('load_device_asset', {
      id,
      workspacePath: workspacePath.trim() || null,
    });
  }, [workspacePath]);

  const saveAsset = useCallback(
    async (
      id: string,
      name: string,
      deviceType: string,
      specYaml: string,
      snapshotLabel?: string,
      snapshotReason?: string,
    ) => {
      if (!hasTauriRuntime()) return;
      await invoke('save_device_asset', {
        id,
        name,
        deviceType,
        specYaml,
        workspacePath: workspacePath.trim() || null,
        snapshotLabel: snapshotLabel ?? null,
        snapshotReason: snapshotReason ?? null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const deleteAsset = useCallback(
    async (id: string) => {
      if (!hasTauriRuntime()) return;
      await invoke('delete_device_asset', {
        id,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const extractFromText = useCallback(
    async (text: string): Promise<string> => {
      const aiConfig = await loadAiConfig();
      const provider = resolveGlobalAiProvider(aiConfig);
      if (!provider) throw new Error('未配置 AI 提供商，请先在设置中配置');
      return aiExtractFromText(text, provider);
    },
    [],
  );

  const extractProposal = useCallback(
    async (text: string): Promise<ExtractionProposal> => {
      const aiConfig = await loadAiConfig();
      const provider = resolveGlobalAiProvider(aiConfig);
      if (!provider) throw new Error('未配置 AI 提供商，请先在设置中配置');
      const result = await aiExtractProposal(text, provider);
      return result as ExtractionProposal;
    },
    [],
  );

  /** 流式 AI 结构化抽取（前端直连 AI provider）。 */
  const extractProposalStream = useCallback(
    async (
      text: string,
      onDelta: (accumulated: string) => void,
      onThinking?: (accumulated: string) => void,
      _providerId?: string,
      correction?: { yaml: string; error: string },
    ): Promise<string> => {
      const aiConfig = await loadAiConfig();
      const provider = resolveGlobalAiProvider(aiConfig);
      if (!provider) throw new Error('未配置 AI 提供商，请先在设置中配置');
      return aiExtractProposalStream(text, provider, { onDelta, onThinking }, correction);
    },
    [],
  );

  const generatePinSchema = useCallback(
    async (deviceId: string): Promise<PinSchemaEntry[]> => {
      if (!hasTauriRuntime()) return [];
      return invoke<PinSchemaEntry[]>('generate_pin_schema', {
        deviceId,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const loadSources = useCallback(
    async (assetId: string): Promise<FieldSource[]> => {
      if (!hasTauriRuntime()) return [];
      return invoke<FieldSource[]>('load_device_asset_sources', {
        assetId,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const saveSources = useCallback(
    async (assetId: string, sources: FieldSource[]) => {
      if (!hasTauriRuntime()) return;
      await invoke('save_device_asset_sources', {
        assetId,
        sources,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const extractTextFromPdf = useCallback(
    async (pdfBase64: string): Promise<string> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      return invoke<string>('extract_text_from_pdf', { pdfBase64 });
    },
    [],
  );

  const extractFromPdf = useCallback(
    async (pdfBase64: string): Promise<ExtractionProposal> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      // PDF 文本提取仍在 Rust 端（JS 无等价库）
      const text = await invoke<string>('extract_text_from_pdf', { pdfBase64 });
      const aiConfig = await loadAiConfig();
      const provider = resolveGlobalAiProvider(aiConfig);
      if (!provider) throw new Error('未配置 AI 提供商，请先在设置中配置');
      const result = await aiExtractProposal(text, provider);
      return result as ExtractionProposal;
    },
    [],
  );

  const importEthercatEsi = useCallback(
    async (esiXml: string): Promise<ExtractionProposal> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      return invoke<ExtractionProposal>('import_ethercat_esi', {
        esiXml,
      });
    },
    [],
  );

  const listVersions = useCallback(
    async (assetId: string) => {
      if (!hasTauriRuntime()) return [];
      return invoke<Array<{ version: number; created_at: string; source_summary: string | null }>>(
        'list_asset_versions',
        {
          assetId,
          workspacePath: workspacePath.trim() || null,
        },
      );
    },
    [workspacePath],
  );

  const listSnapshots = useCallback(
    async (assetId: string) => {
      if (!hasTauriRuntime()) return [];
      return invoke<Array<{
        version: number;
        label: string;
        description: string;
        reason: string;
        created_at: string;
      }>>('list_device_snapshots', {
        assetId,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const createSnapshot = useCallback(
    async (assetId: string, label?: string, description?: string) => {
      if (!hasTauriRuntime()) return;
      await invoke('create_device_snapshot', {
        assetId,
        label: label ?? null,
        description: description ?? null,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const rollbackSnapshot = useCallback(
    async (assetId: string, targetVersion: number) => {
      if (!hasTauriRuntime()) return;
      await invoke('rollback_device_snapshot', {
        assetId,
        targetVersion,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const deleteSnapshot = useCallback(
    async (assetId: string, version: number) => {
      if (!hasTauriRuntime()) return;
      await invoke('delete_device_snapshot', {
        assetId,
        version,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const patchField = useCallback(
    async (
      assetId: string,
      jsonPath: string,
      value: string,
      snapshotLabel?: string,
    ) => {
      if (!hasTauriRuntime()) return;
      await invoke('patch_device_field', {
        assetId,
        jsonPath,
        value,
        snapshotLabel: snapshotLabel ?? null,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const bindConnection = useCallback(
    async (
      assetId: string,
      connectionType: string | null,
      connectionId: string | null,
      unit?: number | null,
    ) => {
      if (!hasTauriRuntime()) return;
      await invoke('bind_device_connection', {
        assetId,
        connectionType: connectionType ?? null,
        connectionId: connectionId ?? null,
        unit: unit ?? null,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const addSignal = useCallback(
    async (assetId: string, signalYaml: string) => {
      if (!hasTauriRuntime()) return;
      await invoke('add_device_signal', {
        assetId,
        signalYaml,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const removeSignal = useCallback(
    async (assetId: string, index: number) => {
      if (!hasTauriRuntime()) return;
      await invoke('remove_device_signal', {
        assetId,
        index,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const addAlarm = useCallback(
    async (assetId: string, alarmYaml: string) => {
      if (!hasTauriRuntime()) return;
      await invoke('add_device_alarm', {
        assetId,
        alarmYaml,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  const removeAlarm = useCallback(
    async (assetId: string, index: number) => {
      if (!hasTauriRuntime()) return;
      await invoke('remove_device_alarm', {
        assetId,
        index,
        workspacePath: workspacePath.trim() || null,
      });
      await loadAssets();
    },
    [loadAssets, workspacePath],
  );

  return {
    assets,
    loading,
    error,
    loadAssets,
    loadDetail,
    saveAsset,
    deleteAsset,
    extractFromText,
    extractProposal,
    extractProposalStream,
    generatePinSchema,
    loadSources,
    saveSources,
    listVersions,
    listSnapshots,
    createSnapshot,
    rollbackSnapshot,
    deleteSnapshot,
    patchField,
    bindConnection,
    addSignal,
    removeSignal,
    addAlarm,
    removeAlarm,
    extractTextFromPdf,
    extractFromPdf,
    importEthercatEsi,
  };
}

