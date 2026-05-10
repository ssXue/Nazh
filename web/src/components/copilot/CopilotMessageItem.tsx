interface Props {
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
}

export function CopilotMessageItem({ role, content, streaming }: Props) {
  const isUser = role === 'user';

  return (
    <div className={`copilot-msg${isUser ? ' copilot-msg--user' : ' copilot-msg--assistant'}`}>
      <div className="copilot-msg__bubble">
        <div className="copilot-msg__content">
          {content || (streaming ? '...' : '')}
        </div>
        {streaming && <span className="copilot-msg__cursor" />}
      </div>
    </div>
  );
}
