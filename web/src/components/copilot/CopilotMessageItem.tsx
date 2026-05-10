import type { ProtocolOperation } from '../../lib/copilot-protocol';
import { MarkdownContent } from './MarkdownContent';

interface Props {
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
  toolCalls?: import('../../lib/copilot-stream').ToolCallInfo[];
  toolResults?: import('../../lib/copilot-stream').ToolResultInfo[];
  protocolOps?: ProtocolOperation[];
  protocolDoneSummary?: string;
}

function ProtocolOpsCard({ ops, doneSummary }: { ops: ProtocolOperation[]; doneSummary?: string }) {
  const entries: { key: string; label: string; className: string }[] = [];

  for (const op of ops) {
    switch (op.type) {
      case 'project':
        entries.push({ key: `p-${entries.length}`, label: `创建工程${op.name ? `「${op.name}」` : ''}`, className: 'copilot-node-chip' });
        break;
      case 'create_node':
        entries.push({ key: `n-${op.ref}`, label: `${op.label ?? op.nodeType}`, className: 'copilot-node-chip' });
        break;
      case 'create_edge':
        entries.push({ key: `e-${op.fromRef}-${op.toRef}-${entries.length}`, label: `${op.fromRef} → ${op.toRef}`, className: 'copilot-edge-tag' });
        break;
      case 'done':
        break;
    }
  }

  return (
    <div className="copilot-protocol-ops">
      <div className="copilot-canvas-ops__nodes">
        {entries.map((entry) => (
          <span key={entry.key} className={entry.className}>{entry.label}</span>
        ))}
      </div>
      {doneSummary && (
        <div className="copilot-canvas-ops__status">{doneSummary}</div>
      )}
    </div>
  );
}

export function CopilotMessageItem({
  role,
  content,
  streaming,
  toolCalls,
  toolResults,
  protocolOps,
  protocolDoneSummary,
}: Props) {
  const isUser = role === 'user';
  const hasProtocolOps = protocolOps && protocolOps.length > 0;

  return (
    <div className={`copilot-msg${isUser ? ' copilot-msg--user' : ' copilot-msg--assistant'}`}>
      <div className="copilot-msg__bubble">
        {toolCalls && toolCalls.length > 0 && (
          <div className="copilot-msg__tools">
            {toolCalls.map((tc) =>
              tc.calls.map((call) => {
                const result = toolResults?.find((r) => r.toolCallId === call.id);
                const statusClass = result
                  ? result.isError
                    ? 'copilot-tool-chip--error'
                    : 'copilot-tool-chip--done'
                  : 'copilot-tool-chip--running';
                return (
                  <span key={call.id} className={`copilot-tool-chip ${statusClass}`}>
                    {call.name}
                  </span>
                );
              }),
            )}
          </div>
        )}
        {hasProtocolOps && (
          <ProtocolOpsCard ops={protocolOps} doneSummary={protocolDoneSummary} />
        )}
        {isUser ? (
          <div className="copilot-msg__content">
            {content || (streaming ? '...' : '')}
          </div>
        ) : (
          !hasProtocolOps && (
            <MarkdownContent content={content} streaming={streaming} />
          )
        )}
        {hasProtocolOps && !protocolDoneSummary && streaming && (
          <span className="copilot-msg__cursor" />
        )}
        {!hasProtocolOps && streaming && <span className="copilot-msg__cursor" />}
      </div>
    </div>
  );
}
