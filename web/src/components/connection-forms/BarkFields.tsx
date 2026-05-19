import type { ConnectionDefinition, JsonValue } from '../../types';

import { metadataNumber, metadataString, parseMetadataNumber } from '../connection-studio-utils';

export interface BarkFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
}

export function BarkFields({
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
