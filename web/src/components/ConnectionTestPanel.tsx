/**
 * 连接健康面板内容组件。
 *
 * 渲染设置面板的头部（连接名/图标/测试按钮/关闭按钮）、
 * 运行态健康状态展示、熔断器重置、绑定设备列表。
 *
 * 注意：本组件不渲染外层 <section className="connection-settings-panel">，
 * 由 ConnectionStudio 主组件提供统一包裹，以确保 DOM 结构与拆分前一致。
 */

import type { DeviceAssetSummary } from '../hooks/use-device-assets';
import type { ConnectionDefinition } from '../types';

import {
  connectionIconFor,
  connectionParameterBrief,
  connectionRuntimeState,
  formatHealthTimestamp,
  isSerialConnectionType,
} from './connection-studio-utils';
import type { ConnectionHealthCallbacks } from './connection-utils';

// ---------------------------------------------------------------------------
// 属性
// ---------------------------------------------------------------------------

export interface ConnectionTestPanelProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  runtimeConnection: import('../types').ConnectionRecord | undefined;
  devicesByConnectionId?: Map<string, DeviceAssetSummary[]>;
  onClose: () => void;
  healthCallbacks: ConnectionHealthCallbacks;
}

// ---------------------------------------------------------------------------
// 连接设置面板头部 + 健康面板 + 绑定设备
// ---------------------------------------------------------------------------

export function ConnectionTestPanel({
  connection,
  connectionIndex,
  runtimeConnection,
  devicesByConnectionId,
  onClose,
  healthCallbacks,
}: ConnectionTestPanelProps) {
  const runtimeState = connectionRuntimeState(runtimeConnection);
  const ConnectionIcon = connectionIconFor(connection.type);

  const {
    handleTestConnection,
    handleResetCircuitBreaker,
    isTesting,
    isResettingCircuit,
    testResult,
  } = healthCallbacks;

  const boundDevices = connection.id
    ? devicesByConnectionId?.get(connection.id) ?? []
    : [];

  return (
    <>
      {/* 面板头部 */}
      <div className="connection-settings-panel__header">
        <div className="connection-settings-panel__icon">
          <ConnectionIcon />
        </div>
        <div>
          <strong>
            {connection.id || `connection_${connectionIndex + 1}`}
          </strong>
          <span>{connectionParameterBrief(connection)}</span>
        </div>
        <div className="connection-settings-panel__actions">
          {isSerialConnectionType(connection.type) ? (
            <button
              type="button"
              className={`ghost ${testResult !== null && testResult.ok ? 'is-success' : ''} ${testResult !== null && !testResult.ok ? 'is-error' : ''}`}
              onClick={handleTestConnection}
              disabled={isTesting}
            >
              {isTesting ? '测试中...' : '测试连接'}
            </button>
          ) : null}
          <button
            type="button"
            className="connection-settings-panel__close"
            onClick={onClose}
          >
            完成
          </button>
        </div>
      </div>

      {/* 健康面板 */}
      <section className="connection-health-panel">
        <div className="connection-health-panel__headline">
          <span className={`connection-status is-${runtimeState.state}`}>
            <span className="connection-status__dot" />
            {runtimeState.label}
          </span>
          {runtimeState.health?.lastLatencyMs ? (
            <span className="asset-card__chip">
              最近链路 {runtimeState.health.lastLatencyMs} ms
            </span>
          ) : null}
          {runtimeState.health?.consecutiveFailures ? (
            <span className="asset-card__chip">
              连续失败 {runtimeState.health.consecutiveFailures}
            </span>
          ) : null}
        </div>

        <p className="connection-health-panel__summary">
          {runtimeState.detail ?? '当前没有可用的连接健康诊断。'}
        </p>

        <div className="connection-health-panel__metrics">
          <span className="asset-card__chip">
            最近心跳 {formatHealthTimestamp(runtimeState.health?.lastHeartbeatAt) ?? '--'}
          </span>
          <span className="asset-card__chip">
            最近失败 {formatHealthTimestamp(runtimeState.health?.lastFailureAt) ?? '--'}
          </span>
          <span className="asset-card__chip">
            超时 {runtimeState.health?.timeoutCount ?? 0}
          </span>
          <span className="asset-card__chip">
            限流 {runtimeState.health?.rateLimitHits ?? 0}
          </span>
          <span className="asset-card__chip">
            重连 {runtimeState.health?.reconnectAttempts ?? 0}
          </span>
          {runtimeState.health?.nextRetryAt ? (
            <span className="asset-card__chip">
              下次重试 {formatHealthTimestamp(runtimeState.health.nextRetryAt)}
            </span>
          ) : null}
          {runtimeState.health?.circuitOpenUntil ? (
            <span className="asset-card__chip">
              熔断至 {formatHealthTimestamp(runtimeState.health.circuitOpenUntil)}
            </span>
          ) : null}
        </div>

        {runtimeState.health?.recommendedAction ? (
          <p className="connection-health-panel__action">
            {runtimeState.health.recommendedAction}
          </p>
        ) : null}

        {runtimeState.state === 'danger' &&
        runtimeState.health?.phase === 'circuitOpen' ? (
          <button
            type="button"
            className="ghost"
            onClick={handleResetCircuitBreaker}
            disabled={isResettingCircuit}
          >
            {isResettingCircuit ? '重置中...' : '手动重置熔断器'}
          </button>
        ) : null}

        {runtimeState.failureReason ? (
          <p className="connection-card__error">{runtimeState.failureReason}</p>
        ) : null}
      </section>

      {/* 绑定设备列表 */}
      {boundDevices.length > 0 ? (
        <section className="connection-bound-devices" aria-label="绑定设备列表">
          <div className="connection-bound-devices__head">
            <strong>绑定设备</strong>
            <span className="asset-card__chip">{boundDevices.length}</span>
          </div>
          <ul className="connection-bound-devices__list">
            {boundDevices.map((device) => (
              <li key={device.id} className="connection-bound-devices__item">
                <strong>{device.name}</strong>
                <span className="connection-bound-devices__type">{device.device_type}</span>
                {device.connection?.unit != null && (
                  <span className="connection-bound-devices__unit">
                    站号 {device.connection.unit}
                  </span>
                )}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </>
  );
}
