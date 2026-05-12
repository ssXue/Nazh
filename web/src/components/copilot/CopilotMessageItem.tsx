import type { CanvasOpEvent } from '../../lib/copilot-stream';
import { MarkdownContent } from './MarkdownContent';

interface Props {
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
  toolCalls?: import('../../lib/copilot-stream').ToolCallInfo[];
  toolResults?: import('../../lib/copilot-stream').ToolResultInfo[];
  canvasOps?: CanvasOpEvent[];
}

function CanvasOpsCard({ ops }: { ops: CanvasOpEvent[] }) {
  const entries: { key: string; label: string; className: string }[] = [];

  for (const op of ops) {
    switch (op.type) {
      case 'create_workflow':
        entries.push({ key: `p-${entries.length}`, label: `创建工程${op.name ? `「${op.name}」` : ''}`, className: 'copilot-node-chip' });
        break;
      case 'add_node':
        entries.push({ key: `n-${op.ref ?? entries.length}`, label: `${op.label ?? op.nodeType ?? 'node'}`, className: 'copilot-node-chip' });
        break;
      case 'add_edge':
        entries.push({ key: `e-${op.fromRef}-${op.toRef}-${entries.length}`, label: `${op.fromRef} → ${op.toRef}`, className: 'copilot-edge-tag' });
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
    </div>
  );
}

export function CopilotMessageItem({
  role,
  content,
  streaming,
  toolCalls,
  toolResults,
  canvasOps,
}: Props) {
  const isUser = role === 'user';
  const hasCanvasOps = canvasOps && canvasOps.length > 0;

  return (
    <div className={`copilot-msg${isUser ? ' copilot-msg--user' : ' copilot-msg--assistant'}`}>
      <div className="copilot-msg__bubble">
        {toolCalls && toolCalls.length > 0 && (
          <div className="copilot-msg__tools">
            {toolCalls.flatMap((tc) =>
              tc.names.map((name, i) => {
                const result = toolResults?.find((r) => r.name === name);
                const statusClass = result
                  ? result.isError
                    ? 'copilot-tool-chip--error'
                    : 'copilot-tool-chip--done'
                  : 'copilot-tool-chip--running';
                return (
                  <span key={`${name}-${i}`} className={`copilot-tool-chip ${statusClass}`}>
                    {name}
                  </span>
                );
              }),
            )}
          </div>
        )}
        {hasCanvasOps && (
          <CanvasOpsCard ops={canvasOps!} />
        )}
        {isUser ? (
          <div className="copilot-msg__content">
            {content || (streaming ? '...' : '')}
          </div>
        ) : (
          <MarkdownContent content={content} streaming={streaming} />
        )}
        {streaming && <span className="copilot-msg__cursor" />}
      </div>
    </div>
  );
}
