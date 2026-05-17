/**
 * 连接编辑表单组件。
 *
 * 负责渲染连接 ID、协议类型、串口/CAN/EtherCAT/HTTP/Bark 参数、
 * 连接健康治理字段以及 Metadata JSON 编辑器。
 */

import type { ConnectionDefinition, JsonValue } from '../types';

import {
  BAUD_RATE_OPTIONS,
  CAN_BITRATE_OPTIONS,
  DEFAULT_CONNECTION_GOVERNANCE,
  DEFAULT_PORT_PATH,
  governanceNumber,
  isBarkConnectionType,
  isCanConnectionType,
  isEthercatConnectionType,
  isHttpConnectionType,
  isSerialConnectionType,
  metadataBoolean,
  metadataNumber,
  metadataString,
  parseMetadataNumber,
} from './connection-studio-utils';
import type { ConnectionFormCallbacks } from './connection-utils';

// ---------------------------------------------------------------------------
// 表单属性
// ---------------------------------------------------------------------------

export interface ConnectionFormProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  draftKey: string;
  idDrafts: Record<string, string>;
  metadataDrafts: Record<string, string>;
  isAdvancedOpen: boolean;
  setIsAdvancedOpen: (value: boolean) => void;
  callbacks: ConnectionFormCallbacks;
}

// ---------------------------------------------------------------------------
// 平台默认串口路径辅助
// ---------------------------------------------------------------------------

function platformDefaultPortPath(): string {
  if (typeof navigator === 'undefined') {
    return '/dev/ttyUSB0';
  }
  const platformKey = navigator.platform.startsWith('Win')
    ? 'win32'
    : navigator.platform.startsWith('Mac')
      ? 'darwin'
      : 'linux';
  return DEFAULT_PORT_PATH[platformKey] ?? '/dev/ttyUSB0';
}

// ---------------------------------------------------------------------------
// 连接编辑表单
// ---------------------------------------------------------------------------

export function ConnectionForm({
  connection,
  connectionIndex,
  draftKey,
  idDrafts,
  metadataDrafts,
  isAdvancedOpen,
  setIsAdvancedOpen,
  callbacks,
}: ConnectionFormProps) {
  const {
    setIdDrafts,
    commitConnectionId,
    handleTypeChange,
    handleMetadataFieldChange,
    handleGovernanceFieldChange,
    handleMetadataChange,
    handlePortPathChange,
    handleBaudRateChange,
    handleRefreshPorts,
    handleRefreshInterfaces,
    scannedPorts,
    isScanningPorts,
    scannedInterfaces,
    isScanningInterfaces,
  } = callbacks;

  return (
    <div className="connection-form connection-settings-panel__form">
      {/* 连接 ID */}
      <label>
        <span>连接 ID</span>
        <input
          value={idDrafts[draftKey] ?? connection.id}
          onChange={(event) =>
            setIdDrafts((current) => ({
              ...current,
              [draftKey]: event.target.value,
            }))
          }
          onBlur={() => commitConnectionId(connectionIndex)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              commitConnectionId(connectionIndex);
              event.currentTarget.blur();
            }
          }}
          placeholder="例如 plc_main"
        />
      </label>

      {/* 协议类型 */}
      <label>
        <span>协议类型</span>
        <input
          value={connection.type}
          onChange={(event) =>
            handleTypeChange(connectionIndex, event.target.value)
          }
          placeholder="例如 modbus / mqtt / http"
        />
      </label>

      {/* 串口参数 */}
      {isSerialConnectionType(connection.type) ? (
        <SerialFields
          connection={connection}
          connectionIndex={connectionIndex}
          handleMetadataFieldChange={handleMetadataFieldChange}
          handlePortPathChange={handlePortPathChange}
          handleBaudRateChange={handleBaudRateChange}
          handleRefreshPorts={handleRefreshPorts}
          scannedPorts={scannedPorts}
          isScanningPorts={isScanningPorts}
          isAdvancedOpen={isAdvancedOpen}
          setIsAdvancedOpen={setIsAdvancedOpen}
        />
      ) : null}

      {/* CAN / SLCAN 参数 */}
      {isCanConnectionType(connection.type) ? (
        <CanFields
          connection={connection}
          connectionIndex={connectionIndex}
          handleMetadataFieldChange={handleMetadataFieldChange}
          handleRefreshPorts={handleRefreshPorts}
          scannedPorts={scannedPorts}
          isScanningPorts={isScanningPorts}
        />
      ) : null}

      {/* EtherCAT 参数 */}
      {isEthercatConnectionType(connection.type) ? (
        <EthercatFields
          connection={connection}
          connectionIndex={connectionIndex}
          handleMetadataFieldChange={handleMetadataFieldChange}
          handleRefreshInterfaces={handleRefreshInterfaces}
          scannedInterfaces={scannedInterfaces}
          isScanningInterfaces={isScanningInterfaces}
        />
      ) : null}

      {/* HTTP / Webhook 参数 */}
      {isHttpConnectionType(connection.type) ? (
        <HttpFields
          connection={connection}
          connectionIndex={connectionIndex}
          handleMetadataFieldChange={handleMetadataFieldChange}
        />
      ) : null}

      {/* Bark Push 参数 */}
      {isBarkConnectionType(connection.type) ? (
        <BarkFields
          connection={connection}
          connectionIndex={connectionIndex}
          handleMetadataFieldChange={handleMetadataFieldChange}
        />
      ) : null}

      {/* 连接健康治理 */}
      <GovernanceFields
        connection={connection}
        connectionIndex={connectionIndex}
        handleGovernanceFieldChange={handleGovernanceFieldChange}
      />

      {/* Metadata JSON */}
      <label className="connection-form__metadata">
        <span>
          {isSerialConnectionType(connection.type)
            ? '高级 Metadata JSON'
            : 'Metadata JSON'}
        </span>
        <textarea
          value={
            metadataDrafts[draftKey] ?? JSON.stringify(connection.metadata ?? {}, null, 2)
          }
          onChange={(event) =>
            handleMetadataChange(connectionIndex, event.target.value)
          }
          spellCheck={false}
        />
      </label>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 串口参数子表单
// ---------------------------------------------------------------------------

interface SerialFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handlePortPathChange: (index: number, value: string) => void;
  handleBaudRateChange: (index: number, value: number) => void;
  handleRefreshPorts: () => void;
  scannedPorts: import('../lib/tauri').SerialPortInfo[];
  isScanningPorts: boolean;
  isAdvancedOpen: boolean;
  setIsAdvancedOpen: (value: boolean) => void;
}

function SerialFields({
  connection,
  connectionIndex,
  handleMetadataFieldChange,
  handlePortPathChange,
  handleBaudRateChange,
  handleRefreshPorts,
  scannedPorts,
  isScanningPorts,
  isAdvancedOpen,
  setIsAdvancedOpen,
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

      <div className="serial-advanced-toggle">
        <button
          type="button"
          className="ghost"
          onClick={() => setIsAdvancedOpen(!isAdvancedOpen)}
        >
          {isAdvancedOpen ? '收起高级设置' : '高级设置'}
        </button>
      </div>

      {isAdvancedOpen ? (
        <>
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
        </>
      ) : null}
    </>
  );
}

// ---------------------------------------------------------------------------
// CAN / SLCAN 参数子表单
// ---------------------------------------------------------------------------

interface CanFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handleRefreshPorts: () => void;
  scannedPorts: import('../lib/tauri').SerialPortInfo[];
  isScanningPorts: boolean;
}

function CanFields({
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

// ---------------------------------------------------------------------------
// EtherCAT 参数子表单
// ---------------------------------------------------------------------------

interface EthercatFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handleRefreshInterfaces: () => void;
  scannedInterfaces: import('../lib/tauri').NetworkInterfaceInfo[];
  isScanningInterfaces: boolean;
}

function EthercatFields({
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

// ---------------------------------------------------------------------------
// HTTP / Webhook 参数子表单
// ---------------------------------------------------------------------------

interface HttpFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
}

function HttpFields({
  connection,
  connectionIndex,
  handleMetadataFieldChange,
}: HttpFieldsProps) {
  return (
    <>
      <label>
        <span>请求地址</span>
        <input
          value={metadataString(connection.metadata, 'url', '')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'url', event.target.value)
          }
          placeholder="https://example.com/webhook"
        />
      </label>
      <label>
        <span>请求方法</span>
        <select
          value={metadataString(connection.metadata, 'method', 'POST').toUpperCase()}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'method', event.target.value)
          }
        >
          <option value="POST">POST</option>
          <option value="PUT">PUT</option>
          <option value="PATCH">PATCH</option>
          <option value="GET">GET</option>
        </select>
      </label>
      <label>
        <span>Webhook 类型</span>
        <select
          value={metadataString(connection.metadata, 'webhook_kind', 'generic')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'webhook_kind', event.target.value)
          }
        >
          <option value="generic">通用 Webhook</option>
          <option value="dingtalk">钉钉机器人</option>
        </select>
      </label>
      {metadataString(connection.metadata, 'webhook_kind', 'generic') === 'dingtalk' ? (
        <>
          <label>
            <span>@ 手机号</span>
            <input
              value={metadataString(connection.metadata, 'at_mobiles', '')}
              onChange={(event) =>
                handleMetadataFieldChange(connectionIndex, 'at_mobiles', event.target.value)
              }
              placeholder="13800000000,13900000000"
            />
          </label>
          <label>
            <span>@ 所有人</span>
            <select
              value={
                metadataBoolean(connection.metadata, 'at_all', false)
                  ? 'true'
                  : 'false'
              }
              onChange={(event) =>
                handleMetadataFieldChange(
                  connectionIndex,
                  'at_all',
                  event.target.value === 'true',
                )
              }
            >
              <option value="false">否</option>
              <option value="true">是</option>
            </select>
          </label>
        </>
      ) : null}
      <label>
        <span>内容类型</span>
        <input
          value={metadataString(
            connection.metadata,
            'content_type',
            'application/json',
          )}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'content_type', event.target.value)
          }
        />
      </label>
      <label>
        <span>请求超时 ms</span>
        <input
          type="number"
          value={metadataNumber(connection.metadata, 'request_timeout_ms', 4000)}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'request_timeout_ms',
              parseMetadataNumber(event.target.value, 4000),
            )
          }
        />
      </label>
    </>
  );
}

// ---------------------------------------------------------------------------
// Bark Push 参数子表单
// ---------------------------------------------------------------------------

interface BarkFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
}

function BarkFields({
  connection,
  connectionIndex,
  handleMetadataFieldChange,
}: BarkFieldsProps) {
  return (
    <>
      <label>
        <span>服务地址</span>
        <input
          value={metadataString(connection.metadata, 'server_url', 'https://api.day.app')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'server_url', event.target.value)
          }
          placeholder="https://api.day.app"
        />
      </label>
      <label>
        <span>设备 Key / 推送 URL</span>
        <input
          value={metadataString(connection.metadata, 'device_key', '')}
          onChange={(event) =>
            handleMetadataFieldChange(connectionIndex, 'device_key', event.target.value)
          }
          placeholder="填写 device_key，或粘贴 https://api.day.app/{key}"
        />
      </label>
      <label>
        <span>请求超时 ms</span>
        <input
          type="number"
          value={metadataNumber(connection.metadata, 'request_timeout_ms', 4000)}
          onChange={(event) =>
            handleMetadataFieldChange(
              connectionIndex,
              'request_timeout_ms',
              parseMetadataNumber(event.target.value, 4000),
            )
          }
        />
      </label>
    </>
  );
}

// ---------------------------------------------------------------------------
// 连接健康治理子表单
// ---------------------------------------------------------------------------

interface GovernanceFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleGovernanceFieldChange: (
    index: number,
    key: string,
    value: number,
  ) => void;
}

function GovernanceFields({
  connection,
  connectionIndex,
  handleGovernanceFieldChange,
}: GovernanceFieldsProps) {
  return (
    <>
      <div className="connection-form__section connection-form__section--full">
        <strong className="connection-form__section-title">连接健康治理</strong>
      </div>

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
    </>
  );
}
