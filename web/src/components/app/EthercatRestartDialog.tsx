interface EthercatRestartDialogProps {
  message: string;
  onCancel: () => void;
  onRestart: () => void;
}

/// 开发模式下 webview 从 Vite dev server 加载（http://localhost:1420），
/// `app.restart()` 会杀掉 tauri dev 进程连带 Vite，新进程无法加载前端。
const isDevMode = window.location.protocol === 'http:' || window.location.protocol === 'https:';

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
            {isDevMode ? (
              <>
                当前进程内无法恢复 EtherCAT 连接。开发模式下无法自动重启（Vite dev server
                不随应用重启），请关闭应用后手动重新启动 tauri dev。
              </>
            ) : (
              <>
                当前进程内无法恢复 EtherCAT 连接，需要重启 nazh-desktop。重启后请重新部署工作流。
              </>
            )}
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
            {isDevMode ? '关闭应用' : '重启应用'}
          </button>
        </div>
      </div>
    </div>
  );
}
