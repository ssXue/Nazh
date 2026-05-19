import type { ConnectionDefinition, JsonValue } from '../../types';

import { metadataNumber, metadataString } from '../connection-studio-utils';

export interface EthercatFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handleRefreshInterfaces: () => void;
  scannedInterfaces: import('../../lib/tauri').NetworkInterfaceInfo[];
  isScanningInterfaces: boolean;
}

export function EthercatFields({
  connection,
  connectionIndex,
  handleMetadataFieldChange,
  handleRefreshInterfaces,
  scannedInterfaces,
  isScanningInterfaces,
}: EthercatFieldsProps) {
  return (
    <>
      <label>
        <span>后端</span>
        <select
          value={metadataString(connection.metadata, 'backend', 'ethercrab')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'backend', event.target.value)
          }
        >
          <option value="ethercrab">ethercrab（真实）</option>
          <option value="mock">Mock（模拟）</option>
        </select>
      </label>
      <label className="serial-port-field">
        <span>网络接口</span>
        <div className="serial-port-select">
          <select
            value={metadataString(connection.metadata, 'interface', 'eth0')}
            onChange={(event) =>
              handleMetadataFieldChange(connectionIndex, 'interface', event.target.value)
            }
          >
            <option value="">-- 选择网卡 --</option>
            {scannedInterfaces
              .filter((iface) => !iface.isLoopback)
              .map((iface) => (
                <option key={iface.name} value={iface.name}>
                  {iface.name}
                  {iface.mac ? ` (${iface.mac})` : ''}
                  {!iface.isUp ? ' [DOWN]' : ''}
                </option>
              ))}
          </select>
          <button
            type="button"
            className="ghost"
            onClick={handleRefreshInterfaces}
            disabled={isScanningInterfaces}
            title="刷新网卡列表"
          >
            {isScanningInterfaces ? '扫描中...' : '刷新'}
          </button>
        </div>
      </label>
      <label>
        <span>周期 (ms)</span>
        <input
          type="number"
          value={metadataNumber(connection.metadata, 'cycle_time_ms', 10)}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'cycle_time_ms',
              parseInt(event.target.value, 10) || 10,
            )
          }
          min={1}
        />
      </label>
      <label>
        <span>OP 超时 (ms)</span>
        <input
          type="number"
          value={metadataNumber(connection.metadata, 'op_timeout_ms', 15000)}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'op_timeout_ms',
              parseInt(event.target.value, 10) || 15000,
            )
          }
          min={1}
        />
      </label>
    </>
  );
}
