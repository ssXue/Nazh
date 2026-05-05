/**
 * ConnectionStudio 辅助工具函数与常量。
 *
 * 连接类型判断、元数据读写、治理策略默认值、模板定义等
 * 无 UI 依赖的纯函数/常量，从 ConnectionStudio.tsx 拆出以降低单文件复杂度。
 */

import type { ComponentType, SVGProps } from 'react';

import type {
  ConnectionDefinition,
  ConnectionHealthSnapshot,
  ConnectionRecord,
  JsonValue,
} from '../types';

export interface ConnectionUsageSummary {
  nodeIds: string[];
  projectNames: string[];
}
import {
  BarkNodeIcon,
  CanNodeIcon,
  ConnectionsIcon,
  HttpClientNodeIcon,
  ModbusNodeIcon,
  SerialNodeIcon,
} from './app/AppIcons';

// ---------------------------------------------------------------------------
// 治理策略默认值
// ---------------------------------------------------------------------------

export const DEFAULT_CONNECTION_GOVERNANCE = {
  connect_timeout_ms: 3000,
  operation_timeout_ms: 5000,
  heartbeat_interval_ms: 3000,
  heartbeat_timeout_ms: 12000,
  rate_limit_max_attempts: 8,
  rate_limit_window_ms: 10000,
  rate_limit_cooldown_ms: 4000,
  circuit_failure_threshold: 3,
  circuit_open_ms: 15000,
  reconnect_base_ms: 800,
  reconnect_max_ms: 8000,
} as const;

// ---------------------------------------------------------------------------
// 连接模板
// ---------------------------------------------------------------------------

export interface ConnectionTemplate {
  key: string;
  label: string;
  description: string;
  idPrefix: string;
  definition: ConnectionDefinition;
}

export const CONNECTION_TEMPLATES: ConnectionTemplate[] = [
  {
    key: 'modbus',
    label: 'Modbus TCP',
    description: '适合 PLC、变频器、温控器等现场设备。',
    idPrefix: 'modbus',
    definition: {
      id: 'modbus',
      type: 'modbus',
      metadata: {
        host: '192.168.10.11',
        port: 502,
        unit_id: 1,
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
  {
    key: 'serial',
    label: '串口设备',
    description: '适合扫码枪、RFID 读卡器等主动上报 ASCII/HEX 的外设。',
    idPrefix: 'serial',
    definition: {
      id: 'serial',
      type: 'serial',
      metadata: {
        port_path: '/dev/tty.usbserial-0001',
        baud_rate: 9600,
        data_bits: 8,
        parity: 'none',
        stop_bits: 1,
        flow_control: 'none',
        encoding: 'ascii',
        delimiter: '\\n',
        read_timeout_ms: 100,
        idle_gap_ms: 80,
        max_frame_bytes: 512,
        trim: true,
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
  {
    key: 'mqtt',
    label: 'MQTT Broker',
    description: '适合边缘网关上报、告警推送和云侧订阅。',
    idPrefix: 'mqtt',
    definition: {
      id: 'mqtt',
      type: 'mqtt',
      metadata: {
        host: 'broker.local',
        port: 1883,
        topic: 'factory/line-a/events',
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
  {
    key: 'http',
    label: 'HTTP / Webhook',
    description: '适合作为 HTTP 上报、Webhook 或钉钉机器人出口。',
    idPrefix: 'http_sink',
    definition: {
      id: 'http_sink',
      type: 'http',
      metadata: {
        url: 'https://example.com/ingest',
        method: 'POST',
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
  {
    key: 'bark',
    label: 'Bark Push',
    description: '适合统一管理 Bark 服务地址、设备 Key 与推送超时。',
    idPrefix: 'bark_push',
    definition: {
      id: 'bark_push',
      type: 'bark',
      metadata: {
        server_url: 'https://api.day.app',
        device_key: '',
        request_timeout_ms: 4000,
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
  {
    key: 'can-slcan',
    label: 'CAN / SLCAN',
    description: '适合 USB-CAN 适配器，使用 Lawicel SLCAN 串口协议。',
    idPrefix: 'can_slcan',
    definition: {
      id: 'can_slcan',
      type: 'can-slcan',
      metadata: {
        interface: 'slcan',
        channel: '/dev/ttyUSB0',
        baud_rate: 115200,
        bitrate: 500000,
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
  {
    key: 'custom',
    label: '空白连接',
    description: '保留完全自定义空间，适配专有协议或插件节点。',
    idPrefix: 'connection',
    definition: {
      id: 'connection',
      type: 'custom',
      metadata: {
        governance: DEFAULT_CONNECTION_GOVERNANCE,
      },
    },
  },
];

export const BAUD_RATE_OPTIONS = [
  1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];

export const CAN_BITRATE_OPTIONS = [
  10000, 20000, 50000, 100000, 125000, 250000, 500000, 800000, 1000000,
];

export const DEFAULT_PORT_PATH: Record<string, string> = {
  darwin: '/dev/cu.usbserial',
  linux: '/dev/ttyUSB0',
  win32: 'COM3',
};

// ---------------------------------------------------------------------------
// 元数据辅助函数
// ---------------------------------------------------------------------------

export function formatMetadata(metadata: JsonValue | undefined): string {
  return JSON.stringify(metadata ?? {}, null, 2);
}

function isRecord(value: unknown): value is Record<string, JsonValue> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function metadataRecord(metadata: JsonValue | undefined): Record<string, JsonValue> {
  return isRecord(metadata) ? metadata : {};
}

export function metadataString(
  metadata: JsonValue | undefined,
  key: string,
  fallback: string,
): string {
  const value = metadataRecord(metadata)[key];
  return typeof value === 'string' ? value : fallback;
}

export function metadataNumber(
  metadata: JsonValue | undefined,
  key: string,
  fallback: number,
): number {
  const value = metadataRecord(metadata)[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

export function metadataBoolean(
  metadata: JsonValue | undefined,
  key: string,
  fallback: boolean,
): boolean {
  const value = metadataRecord(metadata)[key];
  return typeof value === 'boolean' ? value : fallback;
}

export function governanceRecord(metadata: JsonValue | undefined): Record<string, JsonValue> {
  return metadataRecord(metadata).governance && isRecord(metadataRecord(metadata).governance)
    ? (metadataRecord(metadata).governance as Record<string, JsonValue>)
    : {};
}

export function governanceNumber(
  metadata: JsonValue | undefined,
  key: keyof typeof DEFAULT_CONNECTION_GOVERNANCE,
): number {
  const value = governanceRecord(metadata)[key];
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : DEFAULT_CONNECTION_GOVERNANCE[key];
}

export function parseMetadataNumber(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

// ---------------------------------------------------------------------------
// 连接类型判断
// ---------------------------------------------------------------------------

function normalizedConnectionType(connectionType: string): string {
  return connectionType.trim().toLowerCase();
}

export function isSerialConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'serial':
    case 'serialport':
    case 'serial_port':
    case 'uart':
    case 'rs232':
    case 'rs485':
      return true;
    default:
      return false;
  }
}

export function isCanConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'can':
    case 'can-slcan':
    case 'slcan':
      return true;
    default:
      return false;
  }
}

export function isHttpConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'http':
    case 'http_sink':
      return true;
    default:
      return false;
  }
}

export function isBarkConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'bark':
    case 'bark_push':
      return true;
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// 连接图标 & 摘要
// ---------------------------------------------------------------------------

type ConnectionIconComponent = ComponentType<SVGProps<SVGSVGElement>>;

export function connectionIconFor(connectionType: string): ConnectionIconComponent {
  const type = normalizedConnectionType(connectionType);
  if (isSerialConnectionType(type)) {
    return SerialNodeIcon;
  }

  if (isCanConnectionType(type)) {
    return CanNodeIcon;
  }

  if (isBarkConnectionType(type)) {
    return BarkNodeIcon;
  }

  switch (type) {
    case 'modbus':
    case 'modbus_tcp':
      return ModbusNodeIcon;
    case 'http':
    case 'http_sink':
      return HttpClientNodeIcon;
    default:
      return ConnectionsIcon;
  }
}

function compactConnectionValue(value: string, fallback: string): string {
  const normalized = value.trim();
  if (!normalized) {
    return fallback;
  }

  return normalized.length > 38
    ? `${normalized.slice(0, 22)}...${normalized.slice(-10)}`
    : normalized;
}

export function connectionParameterBrief(connection: ConnectionDefinition): string {
  const type = normalizedConnectionType(connection.type);
  if (isSerialConnectionType(type)) {
    const portPath = metadataString(connection.metadata, 'port_path', '未配置端口');
    const baudRate = metadataNumber(connection.metadata, 'baud_rate', 9600);
    const encoding = metadataString(connection.metadata, 'encoding', 'ascii').toUpperCase();
    return `${compactConnectionValue(portPath, '未配置端口')} · ${baudRate} · ${encoding}`;
  }

  if (isCanConnectionType(type)) {
    const channel = metadataString(connection.metadata, 'channel', '未配置通道');
    const baudRate = metadataNumber(connection.metadata, 'baud_rate', 115200);
    const bitrate = metadataNumber(connection.metadata, 'bitrate', 500000);
    return `${compactConnectionValue(channel, '未配置通道')} · ${baudRate} · CAN ${bitrate / 1000} kbps`;
  }

  if (type === 'modbus' || type === 'modbus_tcp') {
    const host = metadataString(connection.metadata, 'host', '未配置主机');
    const port = metadataNumber(connection.metadata, 'port', 502);
    const unitId = metadataNumber(connection.metadata, 'unit_id', 1);
    return `${compactConnectionValue(host, '未配置主机')}:${port} · Unit ${unitId}`;
  }

  if (type === 'mqtt') {
    const host = metadataString(connection.metadata, 'host', '未配置 Broker');
    const port = metadataNumber(connection.metadata, 'port', 1883);
    const topic = metadataString(connection.metadata, 'topic', '未配置 Topic');
    return `${compactConnectionValue(host, '未配置 Broker')}:${port} · ${compactConnectionValue(
      topic,
      '未配置 Topic',
    )}`;
  }

  if (type === 'http' || type === 'http_sink') {
    const method = metadataString(connection.metadata, 'method', 'POST').toUpperCase();
    const url = metadataString(connection.metadata, 'url', '未配置 URL');
    return `${method} · ${compactConnectionValue(url, '未配置 URL')}`;
  }

  if (type === 'bark' || type === 'bark_push') {
    const serverUrl = metadataString(connection.metadata, 'server_url', 'https://api.day.app');
    const deviceKey = metadataString(connection.metadata, 'device_key', '未配置 Key');
    return `${compactConnectionValue(serverUrl, 'https://api.day.app')} · ${compactConnectionValue(
      deviceKey,
      '未配置 Key',
    )}`;
  }

  const metadataKeys = Object.keys(metadataRecord(connection.metadata)).filter(
    (key) => key !== 'governance',
  );
  return metadataKeys.length > 0 ? `${metadataKeys.length} 个参数` : '未配置参数';
}

// ---------------------------------------------------------------------------
// 运行态状态
// ---------------------------------------------------------------------------

export function connectionRuntimeState(runtimeConnection: ConnectionRecord | undefined): {
  label: string;
  state:
    | 'busy'
    | 'local'
    | 'healthy'
    | 'connecting'
    | 'reconnecting'
    | 'warning'
    | 'danger';
  detail: string | null;
  failureReason: string | null;
  health: ConnectionHealthSnapshot | undefined;
} {
  if (!runtimeConnection) {
    return {
      label: '等待部署',
      state: 'local',
      detail: '连接配置已存在，但当前还没有运行态会话。',
      failureReason: null,
      health: undefined,
    };
  }

  const health = runtimeConnection.health;
  const failureReason = health?.lastFailureReason ?? null;

  if (runtimeConnection.in_use) {
    return {
      label:
        health?.phase === 'reconnecting' || health?.phase === 'connecting'
          ? '建连中'
          : '运行占用',
      state: 'busy',
      detail:
        health?.diagnosis ?? '连接已被运行态占用，结束后会自动释放回连接池。',
      failureReason,
      health,
    };
  }

  switch (health?.phase) {
    case 'healthy':
      return {
        label: '连接健康',
        state: 'healthy',
        detail: health.diagnosis ?? '连接可用，最近一次建连与心跳均正常。',
        failureReason,
        health,
      };
    case 'connecting':
      return {
        label: '建连中',
        state: 'connecting',
        detail: health.diagnosis ?? '连接正在建立会话。',
        failureReason,
        health,
      };
    case 'reconnecting':
      return {
        label: '重连中',
        state: 'reconnecting',
        detail: health.diagnosis ?? '连接正在按退避策略重试。',
        failureReason,
        health,
      };
    case 'rateLimited':
      return {
        label: '已限流',
        state: 'warning',
        detail: health.diagnosis ?? '短时间内建连次数过多，暂时拒绝新的会话。',
        failureReason,
        health,
      };
    case 'degraded':
      return {
        label: '待诊断',
        state: 'warning',
        detail: health.diagnosis ?? '连接仍可见，但健康度已经下降。',
        failureReason,
        health,
      };
    case 'timeout':
      return {
        label: '已超时',
        state: 'danger',
        detail: health.diagnosis ?? '连接心跳或运行链路超时。',
        failureReason,
        health,
      };
    case 'circuitOpen':
      return {
        label: '已熔断',
        state: 'danger',
        detail: health.diagnosis ?? '连接连续失败，已被熔断保护。',
        failureReason,
        health,
      };
    case 'invalid':
      return {
        label: '配置无效',
        state: 'danger',
        detail: health.diagnosis ?? '连接配置缺失或格式无效。',
        failureReason,
        health,
      };
    case 'disconnected':
      return {
        label: '已断开',
        state: 'warning',
        detail: health.diagnosis ?? '连接当前未保持活动链路。',
        failureReason,
        health,
      };
    default:
      return {
        label: '等待建连',
        state: 'local',
        detail: health?.diagnosis ?? '连接配置已加载，等待第一次建连。',
        failureReason,
        health,
      };
  }
}

// ---------------------------------------------------------------------------
// 时间戳格式化
// ---------------------------------------------------------------------------

export function formatHealthTimestamp(value: string | null | undefined): string | null {
  if (!value) {
    return null;
  }

  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return value;
  }

  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(timestamp);
}

// ---------------------------------------------------------------------------
// 连接 key 辅助
// ---------------------------------------------------------------------------

export function connectionKey(index: number): string {
  return String(index);
}
