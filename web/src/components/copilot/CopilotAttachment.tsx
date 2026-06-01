/// Copilot 输入区附件 chip 组件（RFC-0006 Phase 6 + P0-1 多文件支持）。

export interface CopilotAttachment {
  file: File;
  name: string;
  size: number;
  type: 'pdf' | 'esi';
}

export const MAX_ATTACHMENT_COUNT = 3;
export const MAX_TOTAL_SIZE = 10 * 1024 * 1024; // 10 MB

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

/// 批量校验文件列表，返回合法附件 + 错误消息列表。
export function validateAttachments(
  files: FileList | File[],
  existing: CopilotAttachment[],
): { attachments: CopilotAttachment[]; errors: string[] } {
  const errors: string[] = [];
  const validated: CopilotAttachment[] = [];

  for (const file of files) {
    if (existing.length + validated.length >= MAX_ATTACHMENT_COUNT) {
      errors.push(`最多 ${MAX_ATTACHMENT_COUNT} 个附件`);
      break;
    }
    const result = validateAttachment(file);
    if (typeof result === 'string') {
      errors.push(result);
    } else {
      const totalSize = existing.reduce((s, a) => s + a.size, 0) + validated.reduce((s, a) => s + a.size, 0) + result.size;
      if (totalSize > MAX_TOTAL_SIZE) {
        errors.push(`总附件大小超过 10 MB`);
        break;
      }
      validated.push(result);
    }
  }

  return { attachments: validated, errors };
}

interface ChipProps {
  attachment: CopilotAttachment;
  onRemove: () => void;
}

export function CopilotAttachmentChip({ attachment, onRemove }: ChipProps) {
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
