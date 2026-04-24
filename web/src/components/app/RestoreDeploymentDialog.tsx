import type { PersistedDeploymentSession } from '../../lib/deployment-session';

interface RestoreDeploymentDialogProps {
  sessions: PersistedDeploymentSession[];
  leadSession: PersistedDeploymentSession;
  countdown: number;
  onSkip: () => void;
  onConfirm: () => void;
}

export function RestoreDeploymentDialog({
  sessions,
  leadSession,
  countdown,
  onSkip,
  onConfirm,
}: RestoreDeploymentDialogProps) {
  const progress = `${Math.max(0, Math.min(100, (countdown / 10) * 100))}%`;

  return (
    <div className="restore-dialog-layer" data-no-window-drag>
      <div
        className="restore-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="restore-dialog-title"
        aria-describedby="restore-dialog-description"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="restore-dialog__header">
          <div className="restore-dialog__eyebrow">
            <span className="restore-dialog__eyebrow-dot" />
            <span>启动恢复</span>
          </div>
          <span className="restore-dialog__timer">{countdown}s</span>
        </div>

        <div className="restore-dialog__body">
          <strong id="restore-dialog-title">
            {sessions.length > 1
              ? `恢复最近 ${sessions.length} 个工程的上次部署？`
              : `恢复“${leadSession.projectName}”的上次部署？`}
          </strong>
          <p id="restore-dialog-description">
            {sessions.length > 1
              ? `将批量恢复最近 ${sessions.length} 个工程的成功部署，并以“${leadSession.projectName}”作为当前工程。若不操作，${countdown} 秒后自动恢复。`
              : `将恢复环境“${leadSession.environmentName}”下的最后一次成功部署。若不操作，${countdown} 秒后自动恢复。`}
          </p>
        </div>

        <div className="restore-dialog__countdown" aria-hidden="true">
          <div className="restore-dialog__countdown-bar" style={{ width: progress }} />
        </div>

        <div className="restore-dialog__actions">
          <button
            type="button"
            className="restore-dialog__action"
            onClick={onSkip}
          >
            不恢复
          </button>
          <button
            type="button"
            className="restore-dialog__action restore-dialog__action--primary"
            onClick={onConfirm}
          >
            立即恢复
          </button>
        </div>
      </div>
    </div>
  );
}
