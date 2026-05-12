interface EthercatRestartDialogProps {
  message: string;
  onCancel: () => void;
  onRestart: () => void;
}

export function EthercatRestartDialog({
  message,
  onCancel,
  onRestart,
}: EthercatRestartDialogProps) {
  return (
    <div className="restore-dialog-layer" data-no-window-drag>
      <div
        className="restore-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="ethercat-restart-title"
        aria-describedby="ethercat-restart-description"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="restore-dialog__header">
          <div className="restore-dialog__eyebrow">
            <span className="restore-dialog__eyebrow-dot" />
            <span>EtherCAT 主站错误</span>
          </div>
        </div>

        <div className="restore-dialog__body">
          <strong id="ethercat-restart-title">EtherCAT 主站需要重启应用</strong>
          <p id="ethercat-restart-description">
            {message}
            <br />
            <br />
            当前进程内无法恢复 EtherCAT 连接，需要重启 nazh-desktop。重启后请重新部署工作流。
          </p>
        </div>

        <div className="restore-dialog__actions">
          <button
            type="button"
            className="restore-dialog__action"
            onClick={onCancel}
          >
            取消
          </button>
          <button
            type="button"
            className="restore-dialog__action restore-dialog__action--primary"
            onClick={onRestart}
          >
            重启应用
          </button>
        </div>
      </div>
    </div>
  );
}
