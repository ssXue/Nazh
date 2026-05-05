import { useCallback, useEffect, useMemo, useState } from 'react';

import { hasTauriRuntime } from '../../lib/tauri';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail } from '../../hooks/use-device-assets';
import { useCapabilities } from '../../hooks/use-capabilities';
import type { CapabilitySummary, CapabilityDetail, GeneratedCapability } from '../../hooks/use-capabilities';
import {
  SparklesIcon,
  PlusIcon,
  DeleteActionIcon,
  SearchIcon,
  DeviceIcon,
  BackIcon,
} from './AppIcons';
import { DeviceImportDrawer } from './DeviceImportDrawer';
import { ExpandTransition } from './ExpandTransition';

interface DeviceModelingPanelProps {
  isTauriRuntime: boolean;
  onStatusMessage: (message: string) => void;
}

export function DeviceModelingPanel({
  isTauriRuntime,
  onStatusMessage,
}: DeviceModelingPanelProps) {
  const {
    assets,
    loading,
    loadAssets,
    loadDetail,
    deleteAsset,
  } = useDeviceAssets();

  const [detail, setDetail] = useState<DeviceAssetDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [importDrawerOpen, setImportDrawerOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const filteredAssets = useMemo(() => {
    if (!searchQuery.trim()) return assets;
    const q = searchQuery.toLowerCase();
    return assets.filter(
      (a) =>
        a.name.toLowerCase().includes(q) ||
        a.device_type.toLowerCase().includes(q) ||
        a.id.toLowerCase().includes(q),
    );
  }, [assets, searchQuery]);

  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  const handleOpenDetail = useCallback(
    async (id: string) => {
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

  const handleCloseDetail = useCallback(() => {
    setDetail(null);
  }, []);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await deleteAsset(id);
        onStatusMessage(`设备 ${id} 已删除`);
        setDetail(null);
      } catch (error) {
        onStatusMessage(`删除设备失败: ${error}`);
      }
    },
    [deleteAsset, onStatusMessage],
  );

  if (!isTauriRuntime) {
    return (
      <div className="device-modeling">
        <div
          className="panel__header panel__header--desktop window-safe-header"
          data-window-drag-region
        >
          <div className="panel__header__heading">
            <h2>设备建模</h2>
            <span>设备建模功能需要 Tauri 桌面运行时。</span>
          </div>
        </div>
      </div>
    );
  }

  const showDetail = detail && !detailLoading;

  const gridBase = (
    <div className="dm-grid-view">
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div className="panel__header__heading">
          <h2>设备资产</h2>
          <span>{assets.length} 个设备资产</span>
        </div>
      </div>

      {assets.length > 0 && (
        <div className="dm-grid-search" data-no-window-drag>
          <SearchIcon width={14} height={14} />
          <input
            type="text"
            className="dm-grid-search-input"
            placeholder="搜索设备..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
      )}

      <div className="dm-grid">
        {/* 虚线添加卡片 */}
        <button
          type="button"
          className="board-card board-card--create"
          onClick={() => setImportDrawerOpen(true)}
        >
          <div className="board-card__icon board-card__icon--create">
            <PlusIcon />
          </div>
          <div className="board-card__body">
            <strong className="board-card__name">导入设备</strong>
            <span className="board-card__desc">
              从 PDF 说明书或文本自动提取设备信息，生成设备模型。
            </span>
          </div>
          <div className="board-card__chips">
            <span className="board-card__chip board-card__chip--create">PDF 导入</span>
            <span className="board-card__chip board-card__chip--create">AI 提取</span>
          </div>
          <div className="board-card__footer">
            <span className="board-card__meta">
              {assets.length === 0 ? '当前还没有设备' : '继续添加设备'}
            </span>
          </div>
        </button>

        {loading ? (
          <div className="dm-loading-card">加载中...</div>
        ) : filteredAssets.length === 0 && assets.length > 0 ? (
          <div className="dm-empty-card">
            <strong>无匹配设备</strong>
            <span>尝试其他搜索词</span>
          </div>
        ) : (
          filteredAssets.map((asset) => {
            return (
              <article
                key={asset.id}
                className="board-card board-card--entry"
                role="button"
                tabIndex={0}
                onClick={() => void handleOpenDetail(asset.id)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    void handleOpenDetail(asset.id);
                  }
                }}
              >
                <div className="board-card__icon">
                  <DeviceIcon />
                </div>

                <div className="board-card__body">
                  <strong className="board-card__name">{asset.name}</strong>
                  <span className="board-card__desc">{asset.device_type}</span>
                </div>

                <div className="board-card__chips">
                  <span className="board-card__chip">{asset.device_type}</span>
                  <span className="board-card__chip">{`v${asset.version}`}</span>
                </div>

                <div className="board-card__footer">
                  <span className="board-card__meta">{asset.updated_at}</span>
                  <button
                    type="button"
                    className="board-card__delete"
                    aria-label={`删除设备 ${asset.name}`}
                    title={`删除设备 ${asset.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleDelete(asset.id);
                    }}
                  >
                    <DeleteActionIcon />
                  </button>
                </div>
              </article>
            );
          })
        )}
      </div>
    </div>
  );

  const detailOverlay = detail ? (
    <DetailPanel
      detail={detail}
      onBack={handleCloseDetail}
      onDelete={() => void handleDelete(detail.id)}
    />
  ) : null;

  return (
    <div className="device-modeling">
      <ExpandTransition
        active={!!showDetail}
        loading={detailLoading}
        mode="centered"
        base={gridBase}
        overlay={detailOverlay}
      />

      <DeviceImportDrawer
        open={importDrawerOpen}
        onClose={() => setImportDrawerOpen(false)}
        onSaved={() => void loadAssets()}
        onStatusMessage={onStatusMessage}
      />
    </div>
  );
}

/** 设备类型标记。 */
function DeviceTypeBadge({ type }: { type: string }) {
  const initial = type.charAt(0).toUpperCase();
  return <span className="dm-type-badge">{initial}</span>;
}

/** 设备详情子面板。 */
function DetailPanel({
  detail,
  onBack,
  onDelete,
}: {
  detail: DeviceAssetDetail;
  onBack: () => void;
  onDelete: () => void;
}) {
  const [tab, setTab] = useState<'signals' | 'capabilities'>('signals');

  const spec = detail.spec_json;

  return (
    <div className="dm-detail-dialog">
      {/* 头部 */}
      <div className="dm-detail-header">
        <div className="dm-detail-header__left">
          <button
            type="button"
            className="dm-back-btn"
            onClick={onBack}
            title="返回设备列表"
          >
            <BackIcon />
          </button>
          <DeviceTypeBadge type={String(spec?.type ?? detail.device_type)} />
          <div className="dm-detail-header__info">
            <h2>{String(spec?.id ?? detail.id)}</h2>
            <div className="dm-detail-header__badges">
              <span className="dm-badge dm-badge--type">
                {String(spec?.type ?? detail.device_type)}
              </span>
              <span className="dm-badge dm-badge--version">
                v{detail.version}
              </span>
              {(spec?.manufacturer as string | undefined) && (
                <span className="dm-badge dm-badge--meta">
                  {spec.manufacturer as string}
                </span>
              )}
              {(spec?.model as string | undefined) && (
                <span className="dm-badge dm-badge--meta">
                  {spec.model as string}
                </span>
              )}
            </div>
          </div>
        </div>
        <button
          type="button"
          className="dm-btn dm-btn--danger"
          title="删除设备"
          onClick={onDelete}
        >
          <DeleteActionIcon width={16} height={16} />
        </button>
      </div>

      {/* Tab 切换 */}
      <div className="dm-tabs">
        <button
          type="button"
          className={`dm-tabs__item${tab === 'signals' ? ' is-active' : ''}`}
          onClick={() => setTab('signals')}
        >
          信号
        </button>
        <button
          type="button"
          className={`dm-tabs__item${tab === 'capabilities' ? ' is-active' : ''}`}
          onClick={() => setTab('capabilities')}
        >
          能力
        </button>
      </div>

      <div className="dm-detail-dialog__body">
        {tab === 'signals' ? (
          <SignalsTab detail={detail} />
        ) : (
          <CapabilitiesTab deviceId={detail.id} />
        )}
      </div>
    </div>
  );
}

/** 信号 Tab。 */
function SignalsTab({ detail }: { detail: DeviceAssetDetail }) {
  const spec = detail.spec_json;
  const signals = spec?.signals as Array<Record<string, unknown>> | undefined;
  const alarms = spec?.alarms as Array<Record<string, unknown>> | undefined;
  const connection = spec?.connection as Record<string, unknown> | undefined;

  return (
    <div className="dm-detail-body">
      {/* 连接信息 */}
      {connection && (
        <div className="dm-section-card">
          <div className="dm-section-card__header">
            <h3>连接</h3>
          </div>
          <div className="dm-section-card__body">
            <div className="dm-connection">
              <span className="dm-tag">
                {String(connection.type ?? '-')}
              </span>
              <span className="dm-mono">
                {String(connection.id ?? '-')}
              </span>
              {connection.unit != null && (
                <span className="dm-meta">
                  站号 {String(connection.unit)}
                </span>
              )}
            </div>
          </div>
        </div>
      )}

      {/* 信号表 */}
      {signals && signals.length > 0 && (
        <div className="dm-section-card">
          <div className="dm-section-card__header">
            <h3>信号 ({signals.length})</h3>
          </div>
          <div className="dm-section-card__body dm-section-card__body--no-pad">
            <table className="dm-table">
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
                    <td className="dm-mono">{String(sig.id)}</td>
                    <td>
                      <span className={`dm-tag dm-tag--signal-${String(sig.signal_type).split('_')[0]}`}>
                        {formatSignalType(String(sig.signal_type))}
                      </span>
                    </td>
                    <td>{sig.unit ? String(sig.unit) : '-'}</td>
                    <td className="dm-mono">
                      {sig.range
                        ? `[${(sig.range as { min: number; max: number }).min}, ${(sig.range as { min: number; max: number }).max}]`
                        : '-'}
                    </td>
                    <td className="dm-mono">
                      {sig.source
                        ? String((sig.source as Record<string, unknown>).type ?? '-')
                        : '-'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* 告警表 */}
      {alarms && alarms.length > 0 && (
        <div className="dm-section-card">
          <div className="dm-section-card__header">
            <h3>告警 ({alarms.length})</h3>
          </div>
          <div className="dm-section-card__body dm-section-card__body--no-pad">
            <table className="dm-table">
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
                    <td className="dm-mono">{String(alarm.id)}</td>
                    <td className="dm-mono">{String(alarm.condition)}</td>
                    <td>
                      <span className={`dm-severity dm-severity--${String(alarm.severity)}`}>
                        {String(alarm.severity)}
                      </span>
                    </td>
                    <td>{alarm.action ? String(alarm.action) : '-'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* 元数据 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>元数据</h3>
        </div>
        <div className="dm-section-card__body">
          <div className="dm-meta-grid">
            <span>创建时间</span>
            <span>{detail.created_at}</span>
            <span>更新时间</span>
            <span>{detail.updated_at}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

/** 能力 Tab。 */
function CapabilitiesTab({ deviceId }: { deviceId: string }) {
  const {
    capabilities,
    loading,
    loadCapabilities,
    loadDetail,
    deleteCapability,
    generateFromDevice,
    saveCapability,
  } = useCapabilities();

  const [selectedCapId, setSelectedCapId] = useState<string | null>(null);
  const [capDetail, setCapDetail] = useState<CapabilityDetail | null>(null);
  const [generated, setGenerated] = useState<GeneratedCapability[]>([]);
  const [generating, setGenerating] = useState(false);

  useEffect(() => {
    void loadCapabilities(deviceId);
  }, [deviceId, loadCapabilities]);

  const handleSelectCap = useCallback(
    async (id: string) => {
      setSelectedCapId(id);
      setCapDetail(null);
      try {
        const result = await loadDetail(id);
        setCapDetail(result);
      } catch {
        /* 忽略加载错误 */
      }
    },
    [loadDetail],
  );

  const handleGenerate = async () => {
    setGenerating(true);
    try {
      const result = await generateFromDevice(deviceId);
      setGenerated(result);
    } catch {
      /* 忽略生成错误 */
    } finally {
      setGenerating(false);
    }
  };

  const handleSaveGenerated = async (gen: GeneratedCapability) => {
    try {
      const name = gen.capability_id.split('.').pop() ?? gen.capability_id;
      await saveCapability(gen.capability_id, deviceId, name, '自动生成', gen.capability_yaml);
      setGenerated((prev) => prev.filter((g) => g.capability_id !== gen.capability_id));
    } catch {
      /* 忽略保存错误 */
    }
  };

  const handleDeleteCap = async (id: string) => {
    try {
      await deleteCapability(id, deviceId);
      if (selectedCapId === id) {
        setSelectedCapId(null);
        setCapDetail(null);
      }
    } catch {
      /* 忽略删除错误 */
    }
  };

  const spec = capDetail?.spec_json;
  const inputs = spec?.inputs as Array<Record<string, unknown>> | undefined;
  const preconditions = spec?.preconditions as string[] | undefined;
  const effects = spec?.effects as string[] | undefined;
  const fallback = spec?.fallback as string[] | undefined;
  const safety = spec?.safety as Record<string, unknown> | undefined;
  const implementation = spec?.implementation as Record<string, unknown> | undefined;

  return (
    <div className="dm-detail-body">
      {/* 标签行 */}
      <div className="dm-cap-tags">
        {loading && <span className="dm-meta">加载中...</span>}
        {!loading && capabilities.length === 0 && generated.length === 0 && (
          <span className="dm-meta">暂无能力</span>
        )}
        {capabilities.map((cap) => (
          <button
            key={cap.id}
            type="button"
            className={`dm-cap-tag${selectedCapId === cap.id ? ' is-active' : ''}`}
            onClick={() => void handleSelectCap(cap.id)}
            title={cap.description || cap.name}
          >
            {cap.name}
          </button>
        ))}
        {generated.map((gen) => (
          <span key={gen.capability_id} className="dm-cap-tag dm-cap-tag--generated">
            {gen.capability_id.split('.').pop()}
            <button
              type="button"
              className="dm-cap-tag__save"
              onClick={() => void handleSaveGenerated(gen)}
              title="保存"
            >
              保存
            </button>
          </span>
        ))}
        <button
          type="button"
          className="dm-cap-tag dm-cap-tag--action"
          onClick={() => void handleGenerate()}
          disabled={generating}
          title="从设备信号自动生成能力"
        >
          {generating ? '生成中...' : '生成'}
        </button>
      </div>

      {/* 能力详情 */}
      {!capDetail ? (
        selectedCapId === null && !loading && capabilities.length > 0 && (
          <div className="dm-section-card">
            <div className="dm-section-card__body" style={{ textAlign: 'center', padding: '24px', color: 'var(--muted)' }}>
              点击上方能力标签查看详情
            </div>
          </div>
        )
      ) : (
        <div className="dm-detail-body" style={{ padding: 0, gap: 8 }}>
          {/* 头部 */}
          <div className="dm-detail-header" style={{ padding: '0 0 12px', borderBottom: '1px solid var(--panel-border)' }}>
            <div className="dm-detail-header__left">
              <DeviceTypeBadge type="C" />
              <div className="dm-detail-header__info">
                <h2>{capDetail.name}</h2>
                <div className="dm-detail-header__badges">
                  {implementation && (
                    <span className={`dm-badge dm-badge--type ${implTypeBadgeClass(String(implementation.type ?? ''))}`}>
                      {String(implementation.type ?? '-')}
                    </span>
                  )}
                  {safety && (
                    <span className={`dm-cap-safety-badge dm-cap-safety-badge--${String(safety.level ?? 'low')}`}>
                      {String(safety.level ?? 'low')}
                    </span>
                  )}
                  <span className="dm-badge dm-badge--version">v{capDetail.version}</span>
                  {capDetail.description && (
                    <span className="dm-badge dm-badge--meta">{capDetail.description}</span>
                  )}
                </div>
              </div>
            </div>
            <button
              type="button"
              className="dm-btn dm-btn--danger"
              title="删除能力"
              onClick={() => void handleDeleteCap(capDetail.id)}
            >
              <DeleteActionIcon width={14} height={14} />
            </button>
          </div>

          {/* 输入参数 */}
          {inputs && inputs.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>输入参数 ({inputs.length})</h3>
              </div>
              <div className="dm-section-card__body dm-section-card__body--no-pad">
                <table className="dm-table">
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>单位</th>
                      <th>量程</th>
                      <th>必填</th>
                    </tr>
                  </thead>
                  <tbody>
                    {inputs.map((inp, i) => (
                      <tr key={String(inp.id ?? i)}>
                        <td className="dm-mono">{String(inp.id)}</td>
                        <td>{inp.unit ? String(inp.unit) : '-'}</td>
                        <td className="dm-mono">
                          {inp.range
                            ? `[${(inp.range as { min: number; max: number }).min}, ${(inp.range as { min: number; max: number }).max}]`
                            : '-'}
                        </td>
                        <td>
                          {inp.required ? (
                            <>
                              <span className="dm-required-dot" />
                              必填
                            </>
                          ) : (
                            '可选'
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* 实现 */}
          {implementation && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>实现</h3>
              </div>
              <div className="dm-section-card__body">
                <div className="dm-impl-grid">
                  {implementation.register != null && (
                    <>
                      <span className="dm-meta">寄存器</span>
                      <span className="dm-mono">{String(implementation.register)}</span>
                    </>
                  )}
                  {implementation.topic != null && (
                    <>
                      <span className="dm-meta">主题</span>
                      <span className="dm-mono">{String(implementation.topic)}</span>
                    </>
                  )}
                  {(implementation.value ?? implementation.payload ?? implementation.command ?? implementation.content) != null && (
                    <>
                      <span className="dm-meta">值</span>
                      <span className="dm-mono">
                        {String(implementation.value ?? implementation.payload ?? implementation.command ?? implementation.content)}
                      </span>
                    </>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* 安全约束 */}
          {safety && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>安全约束</h3>
              </div>
              <div className="dm-section-card__body">
                <div className="dm-safety-row">
                  <span className={`dm-cap-safety-badge dm-cap-safety-badge--${String(safety.level ?? 'low')}`}>
                    {String(safety.level ?? 'low')}
                  </span>
                  {Boolean(safety.requires_approval) && (
                    <span className="dm-meta">需要审批</span>
                  )}
                  {safety.max_execution_time != null && (
                    <span className="dm-meta">最大执行时间: {String(safety.max_execution_time)}</span>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* 前置条件 */}
          {preconditions && preconditions.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>前置条件 ({preconditions.length})</h3>
              </div>
              <div className="dm-section-card__body">
                <ul className="dm-condition-list">
                  {preconditions.map((cond, i) => (
                    <li key={i} className="dm-condition-item">
                      {String(cond)}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}

          {/* 副作用 */}
          {effects && effects.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>副作用 ({effects.length})</h3>
              </div>
              <div className="dm-section-card__body">
                <ul className="dm-condition-list">
                  {effects.map((eff, i) => (
                    <li key={i} className="dm-condition-item">
                      {String(eff)}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}

          {/* 后备能力 */}
          {fallback && fallback.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>后备能力 ({fallback.length})</h3>
              </div>
              <div className="dm-section-card__body">
                <ul className="dm-condition-list">
                  {fallback.map((fb, i) => (
                    <li key={i} className="dm-condition-item">
                      {String(fb)}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---- 辅助函数 ----

function formatSignalType(t: string): string {
  return t.replace(/_/g, ' ');
}

/** 实现 type → CSS class 后缀。 */
function implTypeBadgeClass(type: string): string {
  const lower = type.toLowerCase();
  if (lower.includes('read')) return 'dm-badge--impl-read';
  if (lower.includes('write')) return 'dm-badge--impl-write';
  if (lower.includes('control')) return 'dm-badge--impl-control';
  return '';
}
