import type { ExtractionProposal } from '../../../hooks/use-device-assets';

/** 文件大小上限：6 MB（base64 编码后约 8 MB，留余量给 10 MB IPC 限制）。 */
export const MAX_PDF_SIZE = 6 * 1024 * 1024;
/** ESI XML 文件大小上限：2 MB。 */
export const MAX_ESI_SIZE = 2 * 1024 * 1024;

/** 从 YAML 行尾片段去掉首尾的单/双引号，处理 serde_yaml 对纯数字字符串自动加引号的情况。 */
export function stripYamlQuotes(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  if (!trimmed) return undefined;
  if (
    (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
    (trimmed.startsWith('"') && trimmed.endsWith('"'))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

export type ExtractionPhase =
  | 'idle'
  | 'reading-pdf'
  | 'extracting-text'
  | 'calling-ai'
  | 'parsing-result'
  | 'done'
  | 'error';

export interface PhaseInfo {
  phase: ExtractionPhase;
  label: string;
}

export const PDF_PHASES: PhaseInfo[] = [
  { phase: 'extracting-text', label: '提取文本内容...' },
  { phase: 'calling-ai', label: 'AI 分析说明书中...' },
];

export const TEXT_PHASES: PhaseInfo[] = [
  { phase: 'calling-ai', label: 'AI 分析文本中...' },
  { phase: 'parsing-result', label: '解析抽取结果...' },
];

export const ESI_PHASES: PhaseInfo[] = [
  { phase: 'parsing-result', label: '解析 ESI 文件...' },
];

export interface DeviceImportDrawerProps {
  workspacePath: string;
  onClose: () => void;
  onSaved: () => void;
  onStatusMessage: (message: string) => void;
}

export type InputMode = 'text' | 'pdf' | 'esi';

export type { ExtractionProposal };
