import { useCallback, useEffect, useState } from 'react';

import { hasTauriRuntime } from '../../lib/tauri';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail, ExtractionProposal } from '../../hooks/use-device-assets';
import { useCapabilities } from '../../hooks/use-capabilities';
import type { CapabilitySummary, CapabilityDetail, GeneratedCapability } from '../../hooks/use-capabilities';
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
    extractProposal,
  } = useDeviceAssets();

  const { saveCapability } = useCapabilities();

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DeviceAssetDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>('list');

  // AI 抽取状态
  const [importText, setImportText] = useState('');
  const [extractedYaml, setExtractedYaml] = useState('');
  const [extracting, setExtracting] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);

  // 结构化提案状态（RFC-0004 Phase 4A）
  const [proposal, setProposal] = useState<ExtractionProposal | null>(null);

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

  // AI 结构化抽取（Phase 4A）
  const handleExtract = async () => {
    if (!importText.trim()) return;
    setExtracting(true);
    setExtractError(null);
    setExtractedYaml('');
    setProposal(null);
    try {
      const result = await extractProposal(importText);
      setProposal(result);
      setExtractedYaml(result.deviceYaml);
      const msg = [
        'AI 抽取完成',
        result.uncertainties.length > 0 ? ` · ${result.uncertainties.length} 项待确认` : '',
        result.warnings.length > 0 ? ` · ${result.warnings.length} 条警告` : '',
      ].join('');
      onStatusMessage(msg);
    } catch (error) {
      // 降级到旧版纯 YAML 抽取
      try {
        const yaml = await extractFromText(importText);
        setExtractedYaml(yaml);
        onStatusMessage('AI 抽取完成（基础模式）');
      } catch (fallbackError) {
        setExtractError(`抽取失败: ${fallbackError}`);
      }
    } finally {
      setExtracting(false);
    }
  };

  // 保存抽取结果（设备 + 能力）
  const handleSaveExtracted = async () => {
    if (!extractedYaml) return;
    try {
      const idMatch = extractedYaml.match(/^id:\s*(.+)$/m);
      const typeMatch = extractedYaml.match(/^type:\s*(.+)$/m);
      const deviceId = idMatch?.[1]?.trim() ?? `device_${Date.now()}`;
      const deviceType = typeMatch?.[1]?.trim() ?? 'unknown';
      const name = deviceId.replace(/_/g, ' ');

      await saveAsset(deviceId, name, deviceType, extractedYaml);
      onStatusMessage(`设备 ${deviceId} 已保存`);

      // 批量保存提案中的能力
      if (proposal?.capabilityYamls.length) {
        for (const capYaml of proposal.capabilityYamls) {
          try {
            const capIdMatch = capYaml.match(/^id:\s*(.+)$/m);
            const capId = capIdMatch?.[1]?.trim() ?? `cap_${Date.now()}`;
            const descMatch = capYaml.match(/^description:\s*(.+)$/m);
            const desc = descMatch?.[1]?.trim() ?? capId;
            await saveCapability(capId, deviceId, desc, desc, capYaml);
          } catch {
            // 单个能力保存失败不阻塞其余
          }
        }
        onStatusMessage(`设备 ${deviceId} + ${proposal.capabilityYamls.length} 个能力已保存`);
      }

      setProposal(null);
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
                    保存{proposal?.capabilityYamls.length ? `（+${proposal.capabilityYamls.length} 能力）` : ''}
                  </button>
                </div>
                <pre className="device-modeling__import-yaml">{extractedYaml}</pre>
                {proposal?.capabilityYamls.length ? (
                  <details className="device-modeling__proposal-capabilities">
                    <summary>推断能力 ({proposal.capabilityYamls.length})</summary>
                    {proposal.capabilityYamls.map((cap, idx) => (
                      <pre key={idx} className="device-modeling__import-yaml device-modeling__import-yaml--small">{cap}</pre>
                    ))}
                  </details>
                ) : null}
                {proposal?.uncertainties.length ? (
                  <div className="device-modeling__proposal-uncertainties">
                    <h4>待确认项 ({proposal.uncertainties.length})</h4>
                    <ul>
                      {proposal.uncertainties.map((u, idx) => (
                        <li key={idx}>
                          <code>{u.fieldPath}</code>：{u.guessedValue}
                          <span className="device-modeling__proposal-reason">{u.reason}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : null}
                {proposal?.warnings.length ? (
                  <div className="device-modeling__proposal-warnings">
                    <h4>警告 ({proposal.warnings.length})</h4>
                    <ul>
                      {proposal.warnings.map((w, idx) => (
                        <li key={idx}>{w}</li>
                      ))}
                    </ul>
                  </div>
                ) : null}
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
  const [tab, setTab] = useState<'signals' | 'capabilities'>('signals');

  return (
    <>
      <div className="device-modeling__detail-header">
        <div>
          <h2>{String(detail.spec_json?.id ?? detail.id)}</h2>
          <span className="device-modeling__detail-type">
            {String(detail.spec_json?.type ?? detail.device_type)}
          </span>
          <span className="device-modeling__detail-version">
            v{detail.version}
          </span>
          {(detail.spec_json?.manufacturer as string | undefined) && (
            <span className="device-modeling__detail-extra">
              {detail.spec_json.manufacturer as string}
            </span>
          )}
          {(detail.spec_json?.model as string | undefined) && (
            <span className="device-modeling__detail-extra">
              {detail.spec_json.model as string}
            </span>
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

      {/* Tab 切换 */}
      <div className="device-modeling__tabs">
        <button
          type="button"
          className={`device-modeling__tab${tab === 'signals' ? ' is-active' : ''}`}
          onClick={() => setTab('signals')}
        >
          信号
        </button>
        <button
          type="button"
          className={`device-modeling__tab${tab === 'capabilities' ? ' is-active' : ''}`}
          onClick={() => setTab('capabilities')}
        >
          能力
        </button>
      </div>

      {tab === 'signals' ? (
        <SignalsTab detail={detail} />
      ) : (
        <CapabilitiesTab deviceId={detail.id} />
      )}
    </>
  );
}

/** 信号 Tab。 */
function SignalsTab({ detail }: { detail: DeviceAssetDetail }) {
  const spec = detail.spec_json;
  const signals = spec?.signals as Array<Record<string, unknown>> | undefined;
  const alarms = spec?.alarms as Array<Record<string, unknown>> | undefined;
  const connection = spec?.connection as Record<string, unknown> | undefined;

  return (
    <>
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
    <div className="capability-catalog">
      {/* 头部 */}
      <div className="capability-catalog__header">
        <h3>能力目录</h3>
        <div className="capability-catalog__actions">
          <button
            type="button"
            className="capability-catalog__btn capability-catalog__btn--primary"
            onClick={() => void handleGenerate()}
            disabled={generating}
          >
            {generating ? '生成中...' : '从信号生成'}
          </button>
        </div>
      </div>

      {/* 生成预览 */}
      {generated.length > 0 && (
        <div className="capability-catalog__generated">
          {generated.map((gen) => (
            <div key={gen.capability_id} className="capability-catalog__generated-item">
              <span className="capability-catalog__generated-id">
                {gen.capability_id}
              </span>
              <button
                type="button"
                className="capability-catalog__btn"
                onClick={() => void handleSaveGenerated(gen)}
              >
                保存
              </button>
            </div>
          ))}
        </div>
      )}

      {/* 能力列表 */}
      {loading ? (
        <div className="capability-catalog__loading">加载中...</div>
      ) : capabilities.length === 0 && generated.length === 0 ? (
        <div className="capability-catalog__empty">
          <p>暂无能力</p>
          <span>从设备信号自动生成能力</span>
        </div>
      ) : (
        <ul className="capability-catalog__list">
          {capabilities.map((cap) => (
            <li
              key={cap.id}
              className={`capability-catalog__item${selectedCapId === cap.id ? ' is-active' : ''}`}
              onClick={() => void handleSelectCap(cap.id)}
            >
              <span className="capability-catalog__item-name">{cap.name}</span>
              <button
                type="button"
                className="capability-catalog__btn capability-catalog__btn--danger"
                onClick={(e) => {
                  e.stopPropagation();
                  void handleDeleteCap(cap.id);
                }}
                title="删除"
              >
                <DeleteActionIcon width={12} height={12} />
              </button>
            </li>
          ))}
        </ul>
      )}

      {/* 能力详情 */}
      {capDetail && (
        <div className="capability-catalog__detail">
          <div className="capability-catalog__detail-header">
            <div>
              <h3>{capDetail.name}</h3>
              {capDetail.description && (
                <div className="capability-catalog__detail-desc">
                  {capDetail.description}
                </div>
              )}
            </div>
          </div>

          {/* 输入参数 */}
          {inputs && inputs.length > 0 && (
            <div className="capability-catalog__section">
              <h4>输入参数</h4>
              <table className="capability-catalog__table">
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
                      <td className="is-mono">{String(inp.id)}</td>
                      <td>{inp.unit ? String(inp.unit) : '-'}</td>
                      <td className="is-mono">
                        {inp.range
                          ? `[${(inp.range as { min: number; max: number }).min}, ${(inp.range as { min: number; max: number }).max}]`
                          : '-'}
                      </td>
                      <td>{inp.required ? '是' : '否'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {/* 前置条件 */}
          {preconditions && preconditions.length > 0 && (
            <div className="capability-catalog__section">
              <h4>前置条件</h4>
              <ul className="capability-catalog__conditions">
                {preconditions.map((cond, i) => (
                  <li key={i} className="capability-catalog__condition-item">
                    {String(cond)}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* 实现 */}
          {implementation && (
            <div className="capability-catalog__section">
              <h4>实现</h4>
              <div className="capability-catalog__impl">
                <span className="capability-catalog__impl-type">
                  {String(implementation.type ?? '-')}
                </span>
                {implementation.register != null && (
                  <div className="capability-catalog__impl-field">
                    <span className="capability-catalog__impl-label">寄存器</span>
                    <span className="capability-catalog__impl-value">
                      {String(implementation.register)}
                    </span>
                  </div>
                )}
                {implementation.topic != null && (
                  <div className="capability-catalog__impl-field">
                    <span className="capability-catalog__impl-label">主题</span>
                    <span className="capability-catalog__impl-value">
                      {String(implementation.topic)}
                    </span>
                  </div>
                )}
                {(implementation.value ?? implementation.payload ?? implementation.command ?? implementation.content) != null && (
                  <div className="capability-catalog__impl-field">
                    <span className="capability-catalog__impl-label">值</span>
                    <span className="capability-catalog__impl-value">
                      {String(implementation.value ?? implementation.payload ?? implementation.command ?? implementation.content)}
                    </span>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* 副作用 */}
          {effects && effects.length > 0 && (
            <div className="capability-catalog__section">
              <h4>副作用</h4>
              <ul className="capability-catalog__conditions">
                {effects.map((eff, i) => (
                  <li key={i} className="capability-catalog__condition-item">
                    {String(eff)}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* 后备 */}
          {fallback && fallback.length > 0 && (
            <div className="capability-catalog__section">
              <h4>后备能力</h4>
              <ul className="capability-catalog__conditions">
                {fallback.map((fb, i) => (
                  <li key={i} className="capability-catalog__condition-item">
                    {String(fb)}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* 安全约束 */}
          {safety && (
            <div className="capability-catalog__section">
              <h4>安全约束</h4>
              <div className="capability-catalog__safety">
                <span className={`capability-catalog__item-badge capability-catalog__item-badge--${String(safety.level ?? 'low')}`}>
                  {String(safety.level ?? 'low')}
                </span>
                {Boolean(safety.requires_approval) && (
                  <span className="capability-catalog__safety-detail">需要审批</span>
                )}
                {safety.max_execution_time != null && (
                  <span className="capability-catalog__safety-detail">
                    最大执行时间: {String(safety.max_execution_time)}
                  </span>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
