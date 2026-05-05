import { useCallback, useState } from 'react';

import { hasTauriRuntime } from '../lib/tauri';

/** 设备资产摘要。 */
export interface DeviceAssetSummary {
  id: string;
  name: string;
  device_type: string;
  version: number;
  updated_at: string;
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
  deviceYaml: string;
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
    async (id: string, name: string, deviceType: string, specYaml: string) => {
      if (!hasTauriRuntime()) return;
      await invoke('save_device_asset', {
        id,
        name,
        deviceType,
        specYaml,
        workspacePath: workspacePath.trim() || null,
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
    async (text: string, providerId?: string): Promise<string> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      return invoke<string>('extract_device_from_text', {
        text,
        providerId: providerId ?? null,
      });
    },
    [],
  );

  const extractProposal = useCallback(
    async (text: string, providerId?: string): Promise<ExtractionProposal> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      return invoke<ExtractionProposal>('extract_device_proposal', {
        text,
        providerId: providerId ?? null,
      });
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
    async (pdfBase64: string, providerId?: string): Promise<ExtractionProposal> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      return invoke<ExtractionProposal>('extract_device_from_pdf', {
        pdfBase64,
        providerId: providerId ?? null,
      });
    },
    [],
  );

  const importEthercatEsi = useCallback(
    async (esiXml: string, connectionId?: string, deviceId?: string): Promise<ExtractionProposal> => {
      if (!hasTauriRuntime()) throw new Error('需要 Tauri 运行时');
      return invoke<ExtractionProposal>('import_ethercat_esi', {
        esiXml,
        connectionId: connectionId?.trim() || null,
        deviceId: deviceId?.trim() || null,
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
    generatePinSchema,
    loadSources,
    saveSources,
    listVersions,
    extractTextFromPdf,
    extractFromPdf,
    importEthercatEsi,
  };
}
