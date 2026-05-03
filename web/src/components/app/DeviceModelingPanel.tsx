import { useCallback, useEffect, useState } from 'react';

import { hasTauriRuntime } from '../../lib/tauri';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail } from '../../hooks/use-device-assets';
import {
  SparklesIcon,
  PlusIcon,
  DeleteActionIcon,
  FileYamlIcon,
} from './AppIcons';

interface DeviceModelingPanelProps {
  isTauriRuntime: boolean;
  onStatusMessage: (message: string) => void;
}

type ViewMode = 'list' | 'import';

export function DeviceModelingPanel({
  isTauriRuntime,
  onStatusMessage,
}: DeviceModelingPanelProps) {
  const {
    assets,
    loading,
    loadAssets,
    loadDetail,
    saveAsset,
    deleteAsset,
    extractFromText,
  } = useDeviceAssets();

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DeviceAssetDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>('list');

  // AI 抽取状态
  const [importText, setImportText] = useState('');
  const [extractedYaml, setExtractedYaml] = useState('');
  const [extracting, setExtracting] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);

  // 加载列表
  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  // 加载详情
  const handleSelectAsset = useCallback(
    async (id: string) => {
      setSelectedId(id);
      setDetail(null);
      setDetailLoading(true);
      try {
        const result = await loadDetail(id);
        setDetail(result);
      } catch (error) {
        onStatusMessage(`加载设备详情失败: ${error}`);
      } finally {
        setDetailLoading(false);
      }
    },
    [loadDetail, onStatusMessage],
  );

  // 删除设备
  const handleDelete = async (id: string) => {
    try {
      await deleteAsset(id);
      onStatusMessage(`设备 ${id} 已删除`);
      if (selectedId === id) {
        setSelectedId(null);
        setDetail(null);
      }
    } catch (error) {
      onStatusMessage(`删除设备失败: ${error}`);
    }
  };

  // AI 抽取
  const handleExtract = async () => {
    if (!importText.trim()) return;
    setExtracting(true);
    setExtractError(null);
    setExtractedYaml('');
    try {
      const yaml = await extractFromText(importText);
      setExtractedYaml(yaml);
      onStatusMessage('AI 抽取完成');
    } catch (error) {
      setExtractError(`抽取失败: ${error}`);
    } finally {
      setExtracting(false);
    }
  };

  // 保存抽取结果
  const handleSaveExtracted = async () => {
    if (!extractedYaml) return;
    try {
      // 从 YAML 中提取 id 和 type
      const idMatch = extractedYaml.match(/^id:\s*(.+)$/m);
      const typeMatch = extractedYaml.match(/^type:\s*(.+)$/m);
      const deviceId = idMatch?.[1]?.trim() ?? `device_${Date.now()}`;
      const deviceType = typeMatch?.[1]?.trim() ?? 'unknown';
      const name = deviceId.replace(/_/g, ' ');

      await saveAsset(deviceId, name, deviceType, extractedYaml);
      onStatusMessage(`设备 ${deviceId} 已保存`);
      setExtractedYaml('');
      setImportText('');
      setViewMode('list');
    } catch (error) {
      onStatusMessage(`保存设备失败: ${error}`);
    }
  };

  if (!isTauriRuntime) {
    return (
      <div className="device-modeling">
        <div className="device-modeling__empty">
          <h2>设备建模</h2>
          <p>设备建模功能需要 Tauri 桌面运行时。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="device-modeling">
      {/* 左侧：设备列表 */}
      <div className="device-modeling__list">
        <div className="device-modeling__list-header">
          <h2>设备资产</h2>
          <div className="device-modeling__list-actions">
            <button
              type="button"
              className="device-modeling__btn device-modeling__btn--primary"
              title="从说明书导入"
              onClick={() => setViewMode(viewMode === 'import' ? 'list' : 'import')}
            >
              <SparklesIcon width={16} height={16} />
            </button>
            <button
              type="button"
              className="device-modeling__btn"
              title="新建设备"
              disabled
            >
              <PlusIcon width={16} height={16} />
            </button>
          </div>
        </div>

        {/* 导入面板（内嵌在列表头部） */}
        {viewMode === 'import' && (
          <div className="device-modeling__import">
            <textarea
              className="device-modeling__import-textarea"
              placeholder="粘贴设备说明书文本..."
              value={importText}
              onChange={(e) => setImportText(e.target.value)}
              rows={6}
            />
            <div className="device-modeling__import-actions">
              <button
                type="button"
                className="device-modeling__import-btn"
                disabled={extracting || !importText.trim()}
                onClick={() => void handleExtract()}
              >
                {extracting ? '抽取中...' : 'AI 抽取'}
              </button>
            </div>
            {extractError && (
              <div className="device-modeling__import-error">{extractError}</div>
            )}
            {extractedYaml && (
              <div className="device-modeling__import-result">
                <div className="device-modeling__import-result-header">
                  <FileYamlIcon width={14} height={14} />
                  <span>抽取结果</span>
                  <button
                    type="button"
                    className="device-modeling__import-save-btn"
                    onClick={() => void handleSaveExtracted()}
                  >
                    保存
                  </button>
                </div>
                <pre className="device-modeling__import-yaml">{extractedYaml}</pre>
              </div>
            )}
          </div>
        )}

        {loading ? (
          <div className="device-modeling__loading">加载中...</div>
        ) : assets.length === 0 ? (
          <div className="device-modeling__empty-list">
            <SparklesIcon width={24} height={24} />
            <p>暂无设备</p>
            <span>从说明书导入或手动创建</span>
          </div>
        ) : (
          <ul className="device-modeling__assets">
            {assets.map((asset) => (
              <li
                key={asset.id}
                className={
                  selectedId === asset.id
                    ? 'device-modeling__asset-item is-active'
                    : 'device-modeling__asset-item'
                }
                onClick={() => void handleSelectAsset(asset.id)}
              >
                <div className="device-modeling__asset-name">{asset.name}</div>
                <div className="device-modeling__asset-meta">
                  {asset.device_type} · v{asset.version}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* 右侧：设备详情 */}
      <div className="device-modeling__detail">
        {!detail ? (
          <div className="device-modeling__empty">
            <h2>设备详情</h2>
            <p>选择左侧设备查看详情，或从说明书导入新设备。</p>
          </div>
        ) : detailLoading ? (
          <div className="device-modeling__loading">加载详情...</div>
        ) : (
          <DetailPanel
            detail={detail}
            onDelete={() => void handleDelete(detail.id)}
          />
        )}
      </div>
    </div>
  );
}

/** 设备详情子面板。 */
function DetailPanel({
  detail,
  onDelete,
}: {
  detail: DeviceAssetDetail;
  onDelete: () => void;
}) {
  const spec = detail.spec_json;
  const signals = spec?.signals as Array<Record<string, unknown>> | undefined;
  const alarms = spec?.alarms as Array<Record<string, unknown>> | undefined;
  const connection = spec?.connection as Record<string, unknown> | undefined;
  const manufacturer = spec?.manufacturer as string | undefined;
  const model = spec?.model as string | undefined;

  return (
    <>
      <div className="device-modeling__detail-header">
        <div>
          <h2>{String(spec?.id ?? detail.id)}</h2>
          <span className="device-modeling__detail-type">
            {String(spec?.type ?? detail.device_type)}
          </span>
          <span className="device-modeling__detail-version">
            v{detail.version}
          </span>
          {manufacturer && (
            <span className="device-modeling__detail-extra">
              {manufacturer}
            </span>
          )}
          {model && (
            <span className="device-modeling__detail-extra">{model}</span>
          )}
        </div>
        <button
          type="button"
          className="device-modeling__btn device-modeling__btn--danger"
          title="删除设备"
          onClick={onDelete}
        >
          <DeleteActionIcon width={16} height={16} />
        </button>
      </div>

      {/* 连接信息 */}
      {connection && (
        <div className="device-modeling__section">
          <h3>连接</h3>
          <div className="device-modeling__connection">
            <span className="device-modeling__tag">
              {String(connection.type ?? '-')}
            </span>
            <span className="device-modeling__mono">
              {String(connection.id ?? '-')}
            </span>
            {connection.unit != null && (
              <span className="device-modeling__meta">
                站号 {String(connection.unit)}
              </span>
            )}
          </div>
        </div>
      )}

      {/* 信号表 */}
      {signals && signals.length > 0 && (
        <div className="device-modeling__section">
          <h3>信号 ({signals.length})</h3>
          <table className="device-modeling__table">
            <thead>
              <tr>
                <th>ID</th>
                <th>类型</th>
                <th>单位</th>
                <th>量程</th>
                <th>来源</th>
              </tr>
            </thead>
            <tbody>
              {signals.map((sig, i) => (
                <tr key={String(sig.id ?? i)}>
                  <td className="device-modeling__mono">
                    {String(sig.id)}
                  </td>
                  <td>
                    <span className="device-modeling__tag">
                      {String(sig.signal_type)}
                    </span>
                  </td>
                  <td>{sig.unit ? String(sig.unit) : '-'}</td>
                  <td className="device-modeling__mono">
                    {sig.range
                      ? `[${(sig.range as { min: number; max: number }).min}, ${(sig.range as { min: number; max: number }).max}]`
                      : '-'}
                  </td>
                  <td className="device-modeling__mono">
                    {sig.source
                      ? String((sig.source as Record<string, unknown>).type ?? '-')
                      : '-'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* 告警表 */}
      {alarms && alarms.length > 0 && (
        <div className="device-modeling__section">
          <h3>告警 ({alarms.length})</h3>
          <table className="device-modeling__table">
            <thead>
              <tr>
                <th>ID</th>
                <th>条件</th>
                <th>级别</th>
                <th>动作</th>
              </tr>
            </thead>
            <tbody>
              {alarms.map((alarm, i) => (
                <tr key={String(alarm.id ?? i)}>
                  <td className="device-modeling__mono">
                    {String(alarm.id)}
                  </td>
                  <td className="device-modeling__mono">
                    {String(alarm.condition)}
                  </td>
                  <td>
                    <span
                      className={`device-modeling__severity device-modeling__severity--${String(alarm.severity)}`}
                    >
                      {String(alarm.severity)}
                    </span>
                  </td>
                  <td>{alarm.action ? String(alarm.action) : '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* 元数据 */}
      <div className="device-modeling__section">
        <h3>元数据</h3>
        <div className="device-modeling__meta-grid">
          <span>创建时间</span>
          <span>{detail.created_at}</span>
          <span>更新时间</span>
          <span>{detail.updated_at}</span>
        </div>
      </div>
    </>
  );
}
