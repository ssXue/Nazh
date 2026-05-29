/// Copilot 输入区附件 chip 组件（RFC-0006 Phase 6）。

export interface CopilotAttachment {
  file: File;
  name: string;
  size: number;
  type: 'pdf' | 'esi';
}

const MAX_PDF_SIZE = 6 * 1024 * 1024; // 6 MB
const MAX_ESI_SIZE = 2 * 1024 * 1024; // 2 MB

/// 校验文件类型和大小，返回附件或错误消息。
export function validateAttachment(file: File): CopilotAttachment | string {
  const ext = file.name.toLowerCase();

  if (ext.endsWith('.pdf')) {
    if (file.size > MAX_PDF_SIZE) {
      return `PDF 文件过大（${(file.size / 1024 / 1024).toFixed(1)} MB），上限 6 MB`;
    }
    return { file, name: file.name, size: file.size, type: 'pdf' };
  }

  if (ext.endsWith('.xml') || ext.endsWith('.esi')) {
    if (file.size > MAX_ESI_SIZE) {
      return `ESI 文件过大（${(file.size / 1024 / 1024).toFixed(1)} MB），上限 2 MB`;
    }
    return { file, name: file.name, size: file.size, type: 'esi' };
  }

  return '不支持的文件类型（仅接受 .pdf / .xml / .esi）';
}

interface Props {
  attachment: CopilotAttachment;
  onRemove: () => void;
}

export function CopilotAttachmentChip({ attachment, onRemove }: Props) {
  return (
    <span className="copilot-attachment-chip" data-testid="copilot-attachment">
      <button
        type="button"
        className="copilot-attachment-chip__remove"
        data-testid="copilot-attachment-remove"
        title="移除附件"
        onClick={onRemove}
      >
        &times;
      </button>
      <span className="copilot-attachment-chip__icon">
        {attachment.type === 'pdf' ? '📄' : '📋'}
      </span>
      <span className="copilot-attachment-chip__name" title={attachment.name}>
        {attachment.name}
      </span>
    </span>
  );
}
