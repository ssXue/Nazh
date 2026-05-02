import type { NodeSettingsProps } from '../settings-shared';
import { SwitchBar } from '../settings-shared';

export function BarkPushNodeSettings({ draft, updateDraft, selectedConnection }: NodeSettingsProps) {
  return (
    <>
      {selectedConnection ? (
        <p className="flowgram-form__hint">
          当前节点的 Bark 服务地址、设备 Key 和超时已由 Connection Studio 中的{' '}
          <strong>{selectedConnection.id}</strong> 统一管理。
        </p>
      ) : (
        <p className="flowgram-form__hint">请先在上方绑定一个 Bark 连接。</p>
      )}
      <label>
        <span>内容模式</span>
        <select
          value={draft.barkContentMode}
          onChange={(event) => updateDraft({ barkContentMode: event.target.value })}
        >
          <option value="body">普通文本</option>
          <option value="markdown">Markdown</option>
        </select>
      </label>
      <label>
        <span>中断级别</span>
        <select
          value={draft.barkLevel}
          onChange={(event) => updateDraft({ barkLevel: event.target.value })}
        >
          <option value="active">active</option>
          <option value="timeSensitive">timeSensitive</option>
          <option value="passive">passive</option>
          <option value="critical">critical</option>
        </select>
      </label>
      <label>
        <span>标题模板</span>
        <input
          value={draft.barkTitleTemplate}
          onChange={(event) => updateDraft({ barkTitleTemplate: event.target.value })}
        />
      </label>
      <label>
        <span>副标题模板</span>
        <input
          value={draft.barkSubtitleTemplate}
          onChange={(event) => updateDraft({ barkSubtitleTemplate: event.target.value })}
        />
      </label>
      <label>
        <span>{draft.barkContentMode === 'markdown' ? 'Markdown 模板' : '消息模板'}</span>
        <textarea
          value={draft.barkBodyTemplate}
          onChange={(event) => updateDraft({ barkBodyTemplate: event.target.value })}
        />
      </label>
      <label>
        <span>分组</span>
        <input
          value={draft.barkGroup}
          onChange={(event) => updateDraft({ barkGroup: event.target.value })}
          placeholder="nazh-alert"
        />
      </label>
      <label>
        <span>点击跳转 URL</span>
        <input
          value={draft.barkUrl}
          onChange={(event) => updateDraft({ barkUrl: event.target.value })}
          placeholder="支持 URL Scheme 或 https://"
        />
      </label>
      <label>
        <span>铃声</span>
        <input
          value={draft.barkSound}
          onChange={(event) => updateDraft({ barkSound: event.target.value })}
          placeholder="minuet"
        />
      </label>
      <label>
        <span>Badge</span>
        <input
          value={draft.barkBadge}
          onChange={(event) => updateDraft({ barkBadge: event.target.value })}
          placeholder="0"
        />
      </label>
      <label>
        <span>图标 URL</span>
        <input
          value={draft.barkIcon}
          onChange={(event) => updateDraft({ barkIcon: event.target.value })}
        />
      </label>
      <label>
        <span>图片 URL</span>
        <input
          value={draft.barkImage}
          onChange={(event) => updateDraft({ barkImage: event.target.value })}
        />
      </label>
      <label>
        <span>复制内容</span>
        <input
          value={draft.barkCopy}
          onChange={(event) => updateDraft({ barkCopy: event.target.value })}
          placeholder="留空时不附带 copy 字段"
        />
      </label>
      <SwitchBar
        label="自动复制"
        checked={draft.barkAutoCopy}
        onChange={(value) => updateDraft({ barkAutoCopy: value })}
      />
      <SwitchBar
        label="重复响铃"
        checked={draft.barkCall}
        onChange={(value) => updateDraft({ barkCall: value })}
      />
      <label>
        <span>历史归档</span>
        <select
          value={draft.barkArchiveMode}
          onChange={(event) => updateDraft({ barkArchiveMode: event.target.value })}
        >
          <option value="inherit">跟随 Bark App 设置</option>
          <option value="archive">强制保存</option>
          <option value="skip">不保存</option>
        </select>
      </label>
    </>
  );
}
