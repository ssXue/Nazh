import { useCallback, useEffect, useState } from 'react';

import { useCapabilities } from '../../hooks/use-capabilities';
import type { CapabilityDetail, GeneratedCapability } from '../../hooks/use-capabilities';
import { DeleteActionIcon } from './AppIcons';
import { DeviceTypeBadge, EditableField, implTypeBadgeClass } from './device-modeling-helpers';

/** 能力 Tab。 */
export function CapabilitiesTab({ deviceId, workspacePath, onAddToCanvas }: {
  deviceId: string;
  workspacePath: string;
  onAddToCanvas?: (nodeOp: import('../FlowgramCanvas').CanvasNodeOp) => void;
}) {
  const {
    capabilities,
    loading,
    loadCapabilities,
    loadDetail,
    deleteCapability,
    generateFromDevice,
    saveCapability,
  } = useCapabilities(workspacePath);

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
                  <span className="dm-badge dm-badge--version">{capDetail.version} 个快照</span>
                  {capDetail.description && (
                    <span className="dm-badge dm-badge--meta">{capDetail.description}</span>
                  )}
                </div>
              </div>
            </div>
            {onAddToCanvas ? (
              <button
                type="button"
                className="dm-btn dm-btn--ghost"
                title="添加到画布"
                onClick={() => {
                  const impl = capDetail.spec_json?.implementation as Record<string, unknown> | undefined;
                  onAddToCanvas({
                    id: `capability_call_${capDetail.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`,
                    type: 'capabilityCall',
                    label: capDetail.name,
                    config: {
                      capability_id: capDetail.id,
                      device_id: deviceId,
                      implementation: impl ?? { type: 'script', content: 'payload' },
                      args: {},
                    },
                  });
                }}
              >
                添加到画布
              </button>
            ) : null}
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
