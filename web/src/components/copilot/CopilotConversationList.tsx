import type { CopilotConversationResponse } from '../../generated/CopilotConversationResponse';

interface Props {
  conversations: CopilotConversationResponse[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
  onCollapse: () => void;
}

export function CopilotConversationList({
  conversations,
  activeId,
  onSelect,
  onNew,
  onDelete,
  onCollapse,
}: Props) {
  return (
    <div className="copilot-conv-list">
      <div className="copilot-conv-list__header">
        <span className="copilot-conv-list__title">对话</span>
        <div className="copilot-conv-list__actions">
          <button type="button" className="copilot-btn-icon" title="新建对话" onClick={onNew}>+</button>
          <button type="button" className="copilot-btn-icon" title="收起面板" onClick={onCollapse}>
            &laquo;
          </button>
        </div>
      </div>
      <div className="copilot-conv-list__items">
        {conversations.map((conv) => (
          <div
            key={conv.id}
            className={`copilot-conv-item${conv.id === activeId ? ' is-active' : ''}`}
            role="button"
            tabIndex={0}
            onClick={() => onSelect(conv.id)}
            onKeyDown={(e) => { if (e.key === 'Enter') onSelect(conv.id); }}
          >
            <span className="copilot-conv-item__title">{conv.title}</span>
            <button
              type="button"
              className="copilot-btn-icon copilot-btn-icon--tiny"
              title="删除对话"
              onClick={(e) => { e.stopPropagation(); onDelete(conv.id); }}
            >
              &times;
            </button>
          </div>
        ))}
        {conversations.length === 0 && (
          <div className="copilot-conv-list__empty">暂无对话</div>
        )}
      </div>
    </div>
  );
}
