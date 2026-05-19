/**
 * 连接卡片网格渲染组件。
 *
 * 负责将连接列表渲染为可点击的卡片网格，每张卡片展示连接 ID、
 * 协议类型、设备绑定数、节点引用数、运行态状态等信息。
 */

import type { ConnectionRecord } from '../types';
import { SpotlightCard } from './animations/SpotlightCard';

import type { DeviceAssetSummary } from '../hooks/use-device-assets';
import type { ConnectionUsageSummary } from './connection-studio-utils';
import {
  connectionIconFor,
  connectionParameterBrief,
  connectionRuntimeState,
} from './connection-studio-utils';
import type { ConnectionCardListContext } from './connection-utils';

// ---------------------------------------------------------------------------
// 单张连接卡片
// ---------------------------------------------------------------------------

interface ConnectionCardItemProps {
  connection: import('../types').ConnectionDefinition;
  index: number;
  isActive: boolean;
  runtimeConnection: ConnectionRecord | undefined;
  usage: ConnectionUsageSummary;
  boundDeviceCount: number;
  onSelect: () => void;
}

function ConnectionCardItem({
  connection,
  index,
  isActive,
  runtimeConnection,
  usage,
  boundDeviceCount,
  onSelect,
}: ConnectionCardItemProps) {
  const runtimeState = connectionRuntimeState(runtimeConnection);
  const ConnectionIcon = connectionIconFor(connection.type);

  return (
    <SpotlightCard
      as="article"
      className={`asset-card connection-card ${isActive ? 'is-active' : ''}`}
      data-testid="connection-card"
      role="button"
      tabIndex={0}
      aria-label={`编辑 ${connection.id || `connection_${index + 1}`}`}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      spotlightColor="rgba(74, 114, 201, 0.06)"
    >
      <div className="connection-card__main">
        <div className="asset-card__icon">
          <ConnectionIcon />
        </div>
        <div className="connection-card__identity">
          <strong>{connection.id || `connection_${index + 1}`}</strong>
          <span className="connection-card__brief">
            {connectionParameterBrief(connection)}
          </span>
          <div className="asset-card__chips">
            <span className="asset-card__chip">
              {connection.type || 'custom'}
            </span>
            <span
              className={`asset-card__chip${boundDeviceCount > 0 ? ' asset-card__chip--accent' : ''}`}
            >
              {`${boundDeviceCount} 设备`}
            </span>
            <span className="asset-card__chip">
              {`${usage.nodeIds.length} 节点`}
            </span>
            <span className="asset-card__chip">
              {`${usage.projectNames.length} 工程`}
            </span>
          </div>
        </div>
      </div>

      <div className="asset-card__footer connection-card__footer">
        <span className={`connection-status is-${runtimeState.state}`}>
          <span className="connection-status__dot" />
          {runtimeState.label}
        </span>
      </div>

      {runtimeState.detail ? (
        <p className="connection-card__hint">{runtimeState.detail}</p>
      ) : null}

      {runtimeState.failureReason ? (
        <p className="connection-card__error">{runtimeState.failureReason}</p>
      ) : null}
    </SpotlightCard>
  );
}

// ---------------------------------------------------------------------------
// 连接卡片网格
// ---------------------------------------------------------------------------

export interface ConnectionCardGridProps extends ConnectionCardListContext {}

export function ConnectionCardGrid({
  connections,
  activeConnectionIndex,
  setActiveConnectionIndex,
  runtimeById,
  usageByConnection,
  devicesByConnectionId,
}: ConnectionCardGridProps) {
  return (
    <div className="connection-grid">
      {connections.map((connection, index) => {
        const runtimeConnection = connection.id
          ? runtimeById.get(connection.id)
          : undefined;
        const usage = connection.id
          ? usageByConnection.get(connection.id) ?? { nodeIds: [], projectNames: [] }
          : { nodeIds: [], projectNames: [] };
        const boundDeviceCount = connection.id
          ? devicesByConnectionId?.get(connection.id)?.length ?? 0
          : 0;

        return (
          <ConnectionCardItem
            key={`${connection.id || 'connection'}-${index}`}
            connection={connection}
            index={index}
            isActive={activeConnectionIndex === index}
            runtimeConnection={runtimeConnection}
            usage={usage}
            boundDeviceCount={boundDeviceCount}
            onSelect={() => setActiveConnectionIndex(index)}
          />
        );
      })}
    </div>
  );
}
