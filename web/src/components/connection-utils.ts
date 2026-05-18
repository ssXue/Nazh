/**
 * ConnectionStudio 拆分后的共享类型定义。
 *
 * 包含主组件 Props、子组件 Props 以及跨组件共享的轻量辅助函数。
 * 纯类型/工具文件，无 UI 依赖。
 */

import type { Dispatch, SetStateAction } from 'react';

import type { DeviceAssetSummary } from '../hooks/use-device-assets';
import type {
  ConnectionDefinition,
  ConnectionRecord,
  JsonValue,
} from '../types';
import type { ConnectionUsageSummary } from './connection-studio-utils';

// ---------------------------------------------------------------------------
// 主组件 Props
// ---------------------------------------------------------------------------

export interface ConnectionStudioProps {
  connections: ConnectionDefinition[];
  setConnections: Dispatch<SetStateAction<ConnectionDefinition[]>>;
  usageByConnection: Map<string, ConnectionUsageSummary>;
  runtimeConnections: ConnectionRecord[];
  /** 连接 ID → 绑定它的设备摘要列表（来自 InfrastructurePanel 的设备资产聚合）。 */
  devicesByConnectionId?: Map<string, DeviceAssetSummary[]>;
  /** 来自外部（InfrastructurePanel）的预选连接 ID——例如从设备 Tab "前往连接"跳转过来时。 */
  focusConnectionId?: string | null;
  /** 当 focusConnectionId 被消费打开后调用，用于清空外部状态防止反复触发。 */
  onConsumeFocus?: () => void;
  isLoading?: boolean;
  storageError?: string | null;
  onStatusMessage: (msg: string) => void;
  /** 由 InfrastructurePanel 传入 true 以跳过顶部 panel__header（外层已统一渲染）。 */
  hideHeader?: boolean;
}

// ---------------------------------------------------------------------------
// 子组件共享状态切片类型
// ---------------------------------------------------------------------------

/** ConnectionCard 列表渲染所需的上下文数据。 */
export interface ConnectionCardListContext {
  connections: ConnectionDefinition[];
  activeConnectionIndex: number | null;
  setActiveConnectionIndex: (index: number | null) => void;
  runtimeById: Map<string, ConnectionRecord>;
  usageByConnection: Map<string, ConnectionUsageSummary>;
  devicesByConnectionId?: Map<string, DeviceAssetSummary[]>;
}

/** ConnectionForm 编辑表单所需的回调集合。 */
export interface ConnectionFormCallbacks {
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
  handleGovernanceFieldChange: (
    index: number,
    key: string,
    value: number,
  ) => void;
  handleMetadataChange: (index: number, value: string) => void;
  handlePortPathChange: (index: number, value: string) => void;
  handleBaudRateChange: (index: number, value: number) => void;
  handleRefreshPorts: () => void;
  handleRefreshInterfaces: () => void;
  scannedPorts: import('../lib/tauri').SerialPortInfo[];
  isScanningPorts: boolean;
  scannedInterfaces: import('../lib/tauri').NetworkInterfaceInfo[];
  isScanningInterfaces: boolean;
}

/** ConnectionHealthPanel 健康面板 + 测试所需的回调。 */
export interface ConnectionHealthCallbacks {
  handleTestConnection: () => void;
  handleResetCircuitBreaker: () => void;
  isTesting: boolean;
  isResettingCircuit: boolean;
  testResult: import('../lib/tauri').TestSerialResult | null;
}
