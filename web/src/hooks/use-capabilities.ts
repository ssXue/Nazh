import { useCallback, useState } from 'react';

import { hasTauriRuntime } from '../lib/tauri';

/** 能力资产摘要。 */
export interface CapabilitySummary {
  id: string;
  device_id: string;
  name: string;
  description: string | null;
  version: number;
  updated_at: string;
}

/** 能力资产详情。 */
export interface CapabilityDetail {
  id: string;
  device_id: string;
  name: string;
  description: string | null;
  version: number;
  spec_json: Record<string, unknown>;
  spec_yaml?: string;
  yaml_file_path?: string | null;
  created_at: string;
  updated_at: string;
}

/** 自动生成的能力条目。 */
export interface GeneratedCapability {
  capability_yaml: string;
  capability_id: string;
}

/** AI 来源追溯记录。 */
export interface CapabilitySource {
  field_path: string;
  source_text: string;
  confidence: number;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
  return tauriInvoke<T>(command, args);
}

export function useCapabilities(workspacePath = '') {
  const [capabilities, setCapabilities] = useState<CapabilitySummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadCapabilities = useCallback(async (deviceId?: string) => {
    if (!hasTauriRuntime()) return;
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<CapabilitySummary[]>('list_capabilities', {
        deviceId: deviceId ?? null,
        workspacePath: workspacePath.trim() || null,
      });
      setCapabilities(list);
    } catch (err) {
      setError(`加载能力列表失败: ${err}`);
    } finally {
      setLoading(false);
    }
  }, [workspacePath]);

  const loadDetail = useCallback(
    async (id: string): Promise<CapabilityDetail | null> => {
      if (!hasTauriRuntime()) return null;
      return invoke<CapabilityDetail | null>('load_capability', {
        id,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const saveCapability = useCallback(
    async (
      id: string,
      deviceId: string,
      name: string,
      description: string | null,
      specYaml: string,
    ) => {
      if (!hasTauriRuntime()) return;
      await invoke('save_capability', {
        id,
        deviceId,
        name,
        description,
        specYaml,
        workspacePath: workspacePath.trim() || null,
      });
      await loadCapabilities(deviceId);
    },
    [loadCapabilities, workspacePath],
  );

  const deleteCapability = useCallback(
    async (id: string, deviceId?: string) => {
      if (!hasTauriRuntime()) return;
      await invoke('delete_capability', {
        id,
        workspacePath: workspacePath.trim() || null,
      });
      await loadCapabilities(deviceId);
    },
    [loadCapabilities, workspacePath],
  );

  const generateFromDevice = useCallback(
    async (deviceId: string): Promise<GeneratedCapability[]> => {
      if (!hasTauriRuntime()) return [];
      return invoke<GeneratedCapability[]>('generate_capabilities_from_device_cmd', {
        deviceId,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const loadSources = useCallback(
    async (capabilityId: string): Promise<CapabilitySource[]> => {
      if (!hasTauriRuntime()) return [];
      return invoke<CapabilitySource[]>('load_capability_sources', {
        capabilityId,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const saveSources = useCallback(
    async (capabilityId: string, sources: CapabilitySource[]) => {
      if (!hasTauriRuntime()) return;
      await invoke('save_capability_sources', {
        capabilityId,
        sources,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  const listVersions = useCallback(
    async (capabilityId: string) => {
      if (!hasTauriRuntime()) return [];
      return invoke<
        Array<{ version: number; created_at: string; source_summary: string | null }>
      >('list_capability_versions', {
        capabilityId,
        workspacePath: workspacePath.trim() || null,
      });
    },
    [workspacePath],
  );

  return {
    capabilities,
    loading,
    error,
    loadCapabilities,
    loadDetail,
    saveCapability,
    deleteCapability,
    generateFromDevice,
    loadSources,
    saveSources,
    listVersions,
  };
}
