import { useCallback, useEffect, useState } from 'react';

import { formatRelativeTimestamp } from '../../lib/projects';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import { DeleteActionIcon, SnapshotIcon } from './AppIcons';

/** 快照 Tab。 */
export function SnapshotsTab({
  deviceId,
  workspacePath,
  onReload,
  onStatusMessage,
}: {
  deviceId: string;
  workspacePath: string;
  onReload: () => void;
  onStatusMessage: (msg: string) => void;
}) {
  const { listSnapshots, createSnapshot, rollbackSnapshot, deleteSnapshot } = useDeviceAssets(workspacePath);
  const [snapshots, setSnapshots] = useState<Array<{
    version: number;
    label: string;
    description: string;
    reason: string;
    created_at: string;
  }>>([]);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [newLabel, setNewLabel] = useState('');

  const loadSnapshots = useCallback(async () => {
    setLoading(true);
    try {
      const result = await listSnapshots(deviceId);
      setSnapshots(result);
    } catch {
      /* 忽略 */
    } finally {
      setLoading(false);
    }
  }, [deviceId, listSnapshots]);

  useEffect(() => {
    void loadSnapshots();
  }, [loadSnapshots]);

  const handleCreate = useCallback(async () => {
    setCreating(true);
    try {
      await createSnapshot(deviceId, newLabel.trim() || undefined);
      onStatusMessage('快照已创建');
      setNewLabel('');
      await loadSnapshots();
      onReload();
    } catch (error) {
      onStatusMessage(`创建快照失败: ${error}`);
    } finally {
      setCreating(false);
    }
  }, [deviceId, newLabel, createSnapshot, loadSnapshots, onReload, onStatusMessage]);

  const handleRollback = useCallback(async (version: number) => {
    try {
      await rollbackSnapshot(deviceId, version);
      onStatusMessage(`已回滚到快照 v${version}`);
      await loadSnapshots();
      onReload();
    } catch (error) {
      onStatusMessage(`回滚失败: ${error}`);
    }
  }, [deviceId, rollbackSnapshot, loadSnapshots, onReload, onStatusMessage]);

  const handleDelete = useCallback(async (version: number) => {
    try {
      await deleteSnapshot(deviceId, version);
      await loadSnapshots();
    } catch (error) {
      onStatusMessage(`删除快照失败: ${error}`);
    }
  }, [deviceId, deleteSnapshot, loadSnapshots, onStatusMessage]);

  const reasonLabel = (reason: string): string => {
    switch (reason) {
      case 'seed': return '初始';
      case 'manual': return '手动';
      case 'import': return '导入';
      case 'edit': return '编辑';
      case 'rollback': return '保护';
      default: return reason;
    }
  };

  return (
    <div className="dm-detail-body">
      {/* 创建快照 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>创建快照</h3>
        </div>
        <div className="dm-section-card__body">
          <div className="dm-snapshot-create">
            <input
              type="text"
              className="dm-snapshot-create__input"
              placeholder="快照标签（可选）"
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              disabled={creating}
            />
            <button
              type="button"
              className="dm-btn dm-btn--primary"
              disabled={creating}
              onClick={() => void handleCreate()}
            >
              <SnapshotIcon width={14} height={14} />
              {creating ? '创建中...' : '创建'}
            </button>
          </div>
        </div>
      </div>

      {/* 快照列表 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>历史快照 ({snapshots.length})</h3>
        </div>
        <div className="dm-section-card__body dm-section-card__body--no-pad">
          {loading ? (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--muted)' }}>加载中...</div>
          ) : snapshots.length === 0 ? (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--muted)' }}>暂无快照</div>
          ) : (
            <div className="dm-snapshot-list">
              {snapshots.map((snap) => (
                <div key={snap.version} className="dm-snapshot-card">
                  <div className="dm-snapshot-card__info">
                    <strong>{snap.label}</strong>
                    {snap.description && <span>{snap.description}</span>}
                  </div>
                  <div className="dm-snapshot-card__meta">
                    <em className={`dm-snapshot-reason dm-snapshot-reason--${snap.reason}`}>
                      {reasonLabel(snap.reason)}
                    </em>
                    <span>v{snap.version}</span>
                    <span>{formatRelativeTimestamp(snap.created_at)}</span>
                  </div>
                  <div className="dm-snapshot-card__actions">
                    <button
                      type="button"
                      className="dm-btn"
                      title="回滚到此快照"
                      onClick={() => void handleRollback(snap.version)}
                    >
                      回滚
                    </button>
                    <button
                      type="button"
                      className="dm-btn dm-btn--danger"
                      title="删除此快照元数据"
                      onClick={() => void handleDelete(snap.version)}
                    >
                      <DeleteActionIcon width={14} height={14} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
