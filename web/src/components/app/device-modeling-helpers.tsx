import { useState } from 'react';

import type { ConnectionDefinition, ConnectionRecord } from '../../types';
import { connectionRuntimeState } from '../connection-studio-utils';
import { ConnectionsIcon, PencilIcon } from './AppIcons';
import type { DeviceAssetDetail, DeviceAssetSummary } from '../../hooks/use-device-assets';

/** 设备绑定的连接摘要——给卡片/详情共用的派生结构。 */
export interface DeviceConnectionBinding {
  connectionId: string;
  connectionType: string;
  unit: number | null;
  /** 全局连接库中是否存在该 ID 的定义（true = 已绑定有效连接）。 */
  isResolved: boolean;
  /** 全局连接库中匹配到的定义（解析名称、参数）。 */
  definition: ConnectionDefinition | undefined;
  /** 运行时连接（含 health 快照）。 */
  runtime: ConnectionRecord | undefined;
}

export function buildDeviceConnectionBinding(
  asset: DeviceAssetSummary,
  connectionsById: Map<string, ConnectionDefinition>,
  runtimeById: Map<string, ConnectionRecord>,
): DeviceConnectionBinding | null {
  const ref = asset.connection;
  if (!ref?.id?.trim()) {
    return null;
  }
  const cid = ref.id.trim();
  const definition = connectionsById.get(cid);
  return {
    connectionId: cid,
    connectionType: ref.type?.trim() || definition?.type || '',
    unit: typeof ref.unit === 'number' ? ref.unit : null,
    isResolved: Boolean(definition),
    definition,
    runtime: runtimeById.get(cid),
  };
}

/** 设备类型标记。 */
export function DeviceTypeBadge({ type }: { type: string }) {
  const initial = type.charAt(0).toUpperCase();
  return <span className="dm-type-badge">{initial}</span>;
}

/** 可内联编辑的文本字段。点击后进入编辑模式，Enter/失焦保存，Esc 取消。 */
export function EditableField({
  value,
  label,
  onSave,
  disabled,
  className,
}: {
  value: string;
  label: string;
  onSave: (newValue: string) => void;
  disabled?: boolean;
  className?: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  if (!editing) {
    return (
      <span
        className={`dm-editable${className ? ` ${className}` : ''}`}
        role="button"
        tabIndex={0}
        title={value}
        onClick={() => { setDraft(value); setEditing(true); }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') { setDraft(value); setEditing(true); }
        }}
      >
        {value}
        <PencilIcon width={12} height={12} />
      </span>
    );
  }

  return (
    <input
      className="dm-editable__input"
      type="text"
      value={draft}
      disabled={disabled}
      autoFocus
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft.trim() && draft !== value) {
          onSave(draft.trim());
        }
        setEditing(false);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          if (draft.trim() && draft !== value) {
            onSave(draft.trim());
          }
          setEditing(false);
        } else if (e.key === 'Escape') {
          setEditing(false);
        }
      }}
    />
  );
}

export function formatSignalType(t: string): string {
  return t.replace(/_/g, ' ');
}

/** 实现 type → CSS class 后缀。 */
export function implTypeBadgeClass(type: string): string {
  const lower = type.toLowerCase();
  if (lower.includes('read')) return 'dm-badge--impl-read';
  if (lower.includes('write')) return 'dm-badge--impl-write';
  if (lower.includes('control')) return 'dm-badge--impl-control';
  return '';
}

/** 设备详情顶部的绑定连接栏（置顶展示物理链路 + 切换连接入口）。 */
export function DeviceConnectionBar({
  detail,
  connectionsById,
  runtimeById,
  onJumpToConnection,
}: {
  detail: DeviceAssetDetail;
  connectionsById: Map<string, ConnectionDefinition>;
  runtimeById: Map<string, ConnectionRecord>;
  onJumpToConnection?: (connectionId: string) => void;
}) {
  const conn = detail.spec_json?.connection as
    | { type?: string; id?: string; unit?: number }
    | undefined;
  if (!conn?.id?.trim()) {
    return (
      <div className="dm-detail-connection-bar dm-detail-connection-bar--missing">
        <ConnectionsIcon width={14} height={14} />
        <span>该设备尚未绑定连接，下方信号表的源会因此无法读取。</span>
      </div>
    );
  }
  const cid = conn.id.trim();
  const definition = connectionsById.get(cid);
  const runtime = runtimeById.get(cid);
  const runtimeState = connectionRuntimeState(runtime);
  const isResolved = Boolean(definition);

  return (
    <div className={`dm-detail-connection-bar${isResolved ? '' : ' dm-detail-connection-bar--unresolved'}`}>
      <div className="dm-detail-connection-bar__head">
        <ConnectionsIcon width={14} height={14} />
        <span className="dm-detail-connection-bar__type">{conn.type || definition?.type || '未知协议'}</span>
        <span className="dm-detail-connection-bar__id">{cid}</span>
        {conn.unit != null && (
          <span className="dm-detail-connection-bar__unit">站号 {conn.unit}</span>
        )}
      </div>
      <div className="dm-detail-connection-bar__tail">
        <span className={`connection-status is-${runtimeState.state}`}>
          <span className="connection-status__dot" />
          {runtimeState.label}
        </span>
        {!isResolved && (
          <span className="dm-detail-connection-bar__hint">
            未在全局连接库中找到该 ID，先在连接 Tab 中创建。
          </span>
        )}
        {onJumpToConnection && (
          <button
            type="button"
            className="dm-btn dm-btn--ghost"
            onClick={() => onJumpToConnection(cid)}
            title="切换到连接 Tab 并定位此连接"
          >
            前往连接
          </button>
        )}
      </div>
    </div>
  );
}
