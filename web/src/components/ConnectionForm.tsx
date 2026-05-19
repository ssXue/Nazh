/**
 * 连接编辑表单组件。
 *
 * 渲染协议参数（串口/CAN/EtherCAT/HTTP/Bark）、
 * 连接健康治理字段以及 Metadata JSON 编辑器。
 * 连接 ID 和协议类型已收归 header 只读展示，不在此表单中编辑。
 */

import type { ConnectionDefinition } from '../types';

import {
  isBarkConnectionType,
  isCanConnectionType,
  isEthercatConnectionType,
  isHttpConnectionType,
  isSerialConnectionType,
} from './connection-studio-utils';
import type { ConnectionFormCallbacks } from './connection-utils';
import {
  BarkFields,
  CanFields,
  EthercatFields,
  GovernanceFields,
  HttpFields,
  SerialFields,
} from './connection-forms';

// ---------------------------------------------------------------------------
// 表单属性
// ---------------------------------------------------------------------------

export interface ConnectionFormProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  draftKey: string;
  metadataDrafts: Record<string, string>;
  callbacks: ConnectionFormCallbacks;
}

// ---------------------------------------------------------------------------
// 连接编辑表单
// ---------------------------------------------------------------------------

export function ConnectionForm({
  connection,
  connectionIndex,
  draftKey,
  metadataDrafts,
  callbacks,
}: ConnectionFormProps) {
  const {
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

      <hr className="flowgram-form__divider" />

      {/* 连接健康治理（折叠） */}
      <GovernanceFields
        connection={connection}
        connectionIndex={connectionIndex}
        handleGovernanceFieldChange={handleGovernanceFieldChange}
      />

      {/* Metadata JSON（折叠） */}
      <details className="flowgram-advanced-section">
        <summary className="flowgram-advanced-section__toggle">Metadata JSON</summary>
        <div className="flowgram-advanced-section__body">
          <label className="connection-form__metadata">
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
      </details>
    </div>
  );
}
