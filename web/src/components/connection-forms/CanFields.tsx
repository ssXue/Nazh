import type { ConnectionDefinition, JsonValue } from '../../types';

import {
  BAUD_RATE_OPTIONS,
  CAN_BITRATE_OPTIONS,
  metadataNumber,
  metadataString,
  platformDefaultPortPath,
} from '../connection-studio-utils';

export interface CanFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handleRefreshPorts: () => void;
  scannedPorts: import('../../lib/tauri').SerialPortInfo[];
  isScanningPorts: boolean;
}

export function CanFields({
  connection,
  connectionIndex,
  handleMetadataFieldChange,
  handleRefreshPorts,
  scannedPorts,
  isScanningPorts,
}: CanFieldsProps) {
  return (
    <>
      <label>
        <span>CAN 接口</span>
        <select
          value={metadataString(connection.metadata, 'interface', 'slcan')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'interface', event.target.value)
          }
        >
          <option value="slcan">SLCAN</option>
          <option value="mock">Mock</option>
        </select>
      </label>
      <label className="serial-port-field">
        <span>串口通道</span>
        <div className="serial-port-select">
          <select
            value={metadataString(
              connection.metadata,
              'channel',
              platformDefaultPortPath(),
            )}
            onChange={(event) =>
              handleMetadataFieldChange(connectionIndex, 'channel', event.target.value)
            }
          >
            <option value="">-- 选择通道 --</option>
            {scannedPorts.map((port) => (
              <option key={port.path} value={port.path}>
                {port.path}
                {port.description ? ` (${port.description})` : ''}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="ghost"
            onClick={handleRefreshPorts}
            disabled={isScanningPorts}
            title="刷新端口列表"
          >
            {isScanningPorts ? '扫描中...' : '刷新'}
          </button>
        </div>
      </label>
      <label>
        <span>串口波特率</span>
        <select
          value={metadataNumber(connection.metadata, 'baud_rate', 115200)}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'baud_rate',
              parseInt(event.target.value, 10),
            )
          }
        >
          {BAUD_RATE_OPTIONS.map((rate) => (
            <option key={rate} value={rate}>
              {rate}
            </option>
          ))}
        </select>
      </label>
      <label>
        <span>CAN bitrate</span>
        <select
          value={metadataNumber(connection.metadata, 'bitrate', 500000)}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'bitrate',
              parseInt(event.target.value, 10),
            )
          }
        >
          {CAN_BITRATE_OPTIONS.map((rate) => (
            <option key={rate} value={rate}>
              {rate / 1000} kbps
            </option>
          ))}
        </select>
      </label>
    </>
  );
}
