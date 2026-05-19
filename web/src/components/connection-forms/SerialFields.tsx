import type { ConnectionDefinition, JsonValue } from '../../types';

import {
  BAUD_RATE_OPTIONS,
  metadataBoolean,
  metadataNumber,
  metadataString,
  parseMetadataNumber,
  platformDefaultPortPath,
} from '../connection-studio-utils';

export interface SerialFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handlePortPathChange: (index: number, value: string) => void;
  handleBaudRateChange: (index: number, value: number) => void;
  handleRefreshPorts: () => void;
  scannedPorts: import('../../lib/tauri').SerialPortInfo[];
  isScanningPorts: boolean;
}

export function SerialFields({
  connection,
  connectionIndex,
  handleMetadataFieldChange,
  handlePortPathChange,
  handleBaudRateChange,
  handleRefreshPorts,
  scannedPorts,
  isScanningPorts,
}: SerialFieldsProps) {
  return (
    <>
      <label className="serial-port-field">
        <span>串口路径</span>
        <div className="serial-port-select">
          <select
            value={metadataString(
              connection.metadata,
              'port_path',
              platformDefaultPortPath(),
            )}
            onChange={(event) => handlePortPathChange(connectionIndex, event.target.value)}
          >
            <option value="">-- 选择端口 --</option>
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
        <span>波特率</span>
        <select
          value={metadataNumber(connection.metadata, 'baud_rate', 9600)}
          onChange={(event) =>
            handleBaudRateChange(connectionIndex, parseInt(event.target.value, 10))
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
        <span>数据位</span>
        <select
          value={String(metadataNumber(connection.metadata, 'data_bits', 8))}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'data_bits',
              parseMetadataNumber(event.target.value, 8),
            )
          }
        >
          <option value="8">8</option>
          <option value="7">7</option>
          <option value="6">6</option>
          <option value="5">5</option>
        </select>
      </label>
      <label>
        <span>校验位</span>
        <select
          value={metadataString(connection.metadata, 'parity', 'none')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'parity', event.target.value)
          }
        >
          <option value="none">None</option>
          <option value="odd">Odd</option>
          <option value="even">Even</option>
        </select>
      </label>
      <label>
        <span>停止位</span>
        <select
          value={String(metadataNumber(connection.metadata, 'stop_bits', 1))}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'stop_bits',
              parseMetadataNumber(event.target.value, 1),
            )
          }
        >
          <option value="1">1</option>
          <option value="2">2</option>
        </select>
      </label>
      <label>
        <span>流控</span>
        <select
          value={metadataString(connection.metadata, 'flow_control', 'none')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'flow_control', event.target.value)
          }
        >
          <option value="none">None</option>
          <option value="software">Software</option>
          <option value="hardware">Hardware</option>
        </select>
      </label>
      <label>
        <span>主数据格式</span>
        <select
          value={metadataString(connection.metadata, 'encoding', 'ascii')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'encoding', event.target.value)
          }
        >
          <option value="ascii">ASCII</option>
          <option value="hex">HEX</option>
        </select>
      </label>

      <details className="flowgram-advanced-section">
        <summary className="flowgram-advanced-section__toggle">高级串口设置</summary>
        <div className="flowgram-advanced-section__body">
          <label>
            <span>帧分隔符</span>
            <input
              value={metadataString(connection.metadata, 'delimiter', '\\n')}
              onChange={(event) =>
                handleMetadataFieldChange(connectionIndex, 'delimiter', event.target.value)
              }
              placeholder="\\n、\\r\\n 或 hex:0D0A"
            />
          </label>
          <label>
            <span>读超时 ms</span>
            <input
              type="number"
              value={metadataNumber(connection.metadata, 'read_timeout_ms', 100)}
              onChange={(event) =>
                handleMetadataFieldChange(
                  connectionIndex,
                  'read_timeout_ms',
                  parseMetadataNumber(event.target.value, 100),
                )
              }
            />
          </label>
          <label>
            <span>空闲提交 ms</span>
            <input
              type="number"
              value={metadataNumber(connection.metadata, 'idle_gap_ms', 80)}
              onChange={(event) =>
                handleMetadataFieldChange(
                  connectionIndex,
                  'idle_gap_ms',
                  parseMetadataNumber(event.target.value, 80),
                )
              }
            />
          </label>
          <label>
            <span>最大帧字节</span>
            <input
              type="number"
              value={metadataNumber(connection.metadata, 'max_frame_bytes', 512)}
              onChange={(event) =>
                handleMetadataFieldChange(
                  connectionIndex,
                  'max_frame_bytes',
                  parseMetadataNumber(event.target.value, 512),
                )
              }
            />
          </label>
          <label>
            <span>裁剪空白</span>
            <select
              value={
                metadataBoolean(connection.metadata, 'trim', true)
                  ? 'true'
                  : 'false'
              }
              onChange={(event) =>
                handleMetadataFieldChange(
                  connectionIndex,
                  'trim',
                  event.target.value === 'true',
                )
              }
            >
              <option value="true">是</option>
              <option value="false">否</option>
            </select>
          </label>
        </div>
      </details>
    </>
  );
}
