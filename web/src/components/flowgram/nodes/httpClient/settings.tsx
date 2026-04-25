import {
  getDefaultHttpAlarmBodyTemplate,
  getDefaultHttpAlarmTitleTemplate,
} from '../../flowgram-node-library';
import type { NodeSettingsProps } from '../settings-shared';

export function HttpClientNodeSettings({ draft, updateDraft, selectedConnection, resolvedHttpBodyMode, resolvedHttpWebhookKind }: NodeSettingsProps) {
  return (
    <>
      {selectedConnection ? (
        <p className="flowgram-form__hint">
          当前节点的请求地址、方法和超时已由 Connection Studio 中的{' '}
          <strong>{selectedConnection.id}</strong> 统一管理。
        </p>
      ) : (
        <p className="flowgram-form__hint">请先在上方绑定一个 HTTP / Webhook 连接。</p>
      )}
      <label>
        <span>载荷模式</span>
        <select
          value={resolvedHttpBodyMode}
          onChange={(event) =>
            updateDraft({
              httpBodyMode: event.target.value,
              httpTitleTemplate:
                event.target.value === 'dingtalk_markdown' && !draft.httpTitleTemplate.trim()
                  ? getDefaultHttpAlarmTitleTemplate()
                  : draft.httpTitleTemplate,
              httpBodyTemplate:
                event.target.value === 'dingtalk_markdown' && !draft.httpBodyTemplate.trim()
                  ? getDefaultHttpAlarmBodyTemplate()
                  : draft.httpBodyTemplate,
            })
          }
        >
          <option value="json">JSON Payload</option>
          <option value="template">自定义模板</option>
          {resolvedHttpWebhookKind === 'dingtalk' ? (
            <option value="dingtalk_markdown">钉钉 Markdown</option>
          ) : null}
        </select>
      </label>
      {resolvedHttpBodyMode === 'dingtalk_markdown' ? (
        <label>
          <span>标题模板</span>
          <textarea
            value={draft.httpTitleTemplate}
            onChange={(event) => updateDraft({ httpTitleTemplate: event.target.value })}
          />
        </label>
      ) : null}
      {resolvedHttpBodyMode !== 'json' ? (
        <label>
          <span>{resolvedHttpBodyMode === 'dingtalk_markdown' ? '消息模板' : '请求体模板'}</span>
          <textarea
            value={draft.httpBodyTemplate}
            onChange={(event) => updateDraft({ httpBodyTemplate: event.target.value })}
          />
        </label>
      ) : null}
    </>
  );
}
