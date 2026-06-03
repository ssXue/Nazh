import { useCallback, useState } from 'react';

import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail } from '../../hooks/use-device-assets';
import { SaveIcon } from './AppIcons';

/**
 * YAML 源码编辑 Tab。
 *
 * 直接编辑设备 DSL 的原始 YAML 文件，保存时走 `save_device_asset` IPC
 * 做解析校验。适合 AI 不可用时的手动降级编辑场景。
 */
export function YamlTab({
  detail,
  workspacePath,
  onReload,
  onStatusMessage,
}: {
  detail: DeviceAssetDetail;
  workspacePath: string;
  onReload: () => void;
  onStatusMessage: (msg: string) => void;
}) {
  const { saveAsset } = useDeviceAssets(workspacePath);
  const [yaml, setYaml] = useState(detail.spec_yaml ?? '');
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  const handleChange = useCallback(
    (value: string) => {
      setYaml(value);
      setDirty(value !== (detail.spec_yaml ?? ''));
    },
    [detail.spec_yaml],
  );

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await saveAsset(
        detail.id,
        detail.name,
        detail.device_type,
        yaml,
        '手动 YAML 编辑',
        '通过 YAML 源码编辑器修改',
      );
      setDirty(false);
      onStatusMessage('YAML 已保存');
      onReload();
    } catch (error) {
      onStatusMessage(`保存失败: ${error}`);
    } finally {
      setSaving(false);
    }
  }, [detail.id, detail.name, detail.device_type, yaml, saveAsset, onReload, onStatusMessage]);

  return (
    <div className="yaml-tab">
      <div className="yaml-tab__toolbar">
        <span className="yaml-tab__hint">
          直接编辑设备 DSL 源码。保存时会校验格式。
        </span>
        <button
          type="button"
          className="settings-inline-button"
          disabled={!dirty || saving}
          onClick={() => void handleSave()}
        >
          <SaveIcon className="ai-btn-icon" />
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
      <textarea
        className="yaml-tab__editor"
        spellCheck={false}
        value={yaml}
        onChange={(e) => handleChange(e.target.value)}
      />
    </div>
  );
}
