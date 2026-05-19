import type { ConnectionDefinition, JsonValue } from '../../types';

import {
  metadataBoolean,
  metadataNumber,
  metadataString,
  parseMetadataNumber,
} from '../connection-studio-utils';

export interface HttpFieldsProps {
  connection: ConnectionDefinition;
  connectionIndex: number;
  handleMetadataFieldChange: (index: number, key: string, value: JsonValue) => void;
}

export function HttpFields({
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
