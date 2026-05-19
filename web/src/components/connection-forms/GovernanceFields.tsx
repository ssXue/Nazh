import type { ConnectionDefinition } from '../../types';

import {
  DEFAULT_CONNECTION_GOVERNANCE,
  governanceNumber,
  parseMetadataNumber,
} from '../connection-studio-utils';

export interface GovernanceFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleGovernanceFieldChange: (
    index: number,
    key: string,
    value: number,
  ) => void;
}

export function GovernanceFields({
  connection,
  connectionIndex,
  handleGovernanceFieldChange,
}: GovernanceFieldsProps) {
  return (
    <details className="flowgram-advanced-section">
      <summary className="flowgram-advanced-section__toggle">连接健康治理</summary>
      <div className="flowgram-advanced-section__body">
        <label>
          <span>建连超时 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'connect_timeout_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'connect_timeout_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.connect_timeout_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>操作超时 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'operation_timeout_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'operation_timeout_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.operation_timeout_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>心跳间隔 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'heartbeat_interval_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'heartbeat_interval_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.heartbeat_interval_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>心跳超时 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'heartbeat_timeout_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'heartbeat_timeout_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.heartbeat_timeout_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>限流次数</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'rate_limit_max_attempts')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'rate_limit_max_attempts',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.rate_limit_max_attempts,
                ),
              )
            }
          />
        </label>
        <label>
          <span>限流窗口 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'rate_limit_window_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'rate_limit_window_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.rate_limit_window_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>限流冷却 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'rate_limit_cooldown_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'rate_limit_cooldown_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.rate_limit_cooldown_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>熔断阈值</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'circuit_failure_threshold')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'circuit_failure_threshold',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.circuit_failure_threshold,
                ),
              )
            }
          />
        </label>
        <label>
          <span>熔断冷却 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'circuit_open_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'circuit_open_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.circuit_open_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>重连基准 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'reconnect_base_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'reconnect_base_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.reconnect_base_ms,
                ),
              )
            }
          />
        </label>
        <label>
          <span>重连上限 ms</span>
          <input
            type="number"
            value={governanceNumber(connection.metadata, 'reconnect_max_ms')}
            onChange={(event) =>
              handleGovernanceFieldChange(
                connectionIndex,
                'reconnect_max_ms',
                parseMetadataNumber(
                  event.target.value,
                  DEFAULT_CONNECTION_GOVERNANCE.reconnect_max_ms,
                ),
              )
            }
          />
        </label>
      </div>
    </details>
  );
}
