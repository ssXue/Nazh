import { useCallback, useRef, useState } from 'react';

import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { ExtractionProposal } from '../../hooks/use-device-assets';
import { useCapabilities } from '../../hooks/use-capabilities';
import {
  SparklesIcon,
  FilePdfIcon,
  FileYamlIcon,
  UploadIcon,
  XCloseIcon,
} from './AppIcons';

/** 文件大小上限：6 MB（base64 编码后约 8 MB，留余量给 10 MB IPC 限制）。 */
const MAX_PDF_SIZE = 6 * 1024 * 1024;

type ExtractionPhase =
  | 'idle'
  | 'reading-pdf'
  | 'extracting-text'
  | 'calling-ai'
  | 'parsing-result'
  | 'done'
  | 'error';

interface PhaseInfo {
  phase: ExtractionPhase;
  label: string;
}

const PDF_PHASES: PhaseInfo[] = [
  { phase: 'reading-pdf', label: '读取 PDF 文件...' },
  { phase: 'extracting-text', label: '提取文本内容...' },
  { phase: 'calling-ai', label: 'AI 分析说明书中...' },
  { phase: 'parsing-result', label: '解析抽取结果...' },
];

const TEXT_PHASES: PhaseInfo[] = [
  { phase: 'calling-ai', label: 'AI 分析文本中...' },
  { phase: 'parsing-result', label: '解析抽取结果...' },
];

interface DeviceImportDrawerProps {
  workspacePath: string;
  onClose: () => void;
  onSaved: () => void;
  onStatusMessage: (message: string) => void;
}

type InputMode = 'text' | 'pdf';

export function DeviceImportDrawer({
  workspacePath,
  onClose,
  onSaved,
  onStatusMessage,
}: DeviceImportDrawerProps) {
  const { extractFromText, extractProposal, extractTextFromPdf, extractFromPdf, saveAsset } =
    useDeviceAssets(workspacePath);
  const { saveCapability } = useCapabilities(workspacePath);

  const [mode, setMode] = useState<InputMode>('text');

  // 文本粘贴状态
  const [importText, setImportText] = useState('');

  // PDF 上传状态
  const [pdfFile, setPdfFile] = useState<File | null>(null);
  const [pdfBase64, setPdfBase64] = useState<string | null>(null);
  const [extractedText, setExtractedText] = useState('');
  const [showTextPreview, setShowTextPreview] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // AI 抽取状态
  const [extractedYaml, setExtractedYaml] = useState('');
  const [phase, setPhase] = useState<ExtractionPhase>('idle');
  const [extractError, setExtractError] = useState<string | null>(null);
  const [proposal, setProposal] = useState<ExtractionProposal | null>(null);

  const extracting = phase !== 'idle' && phase !== 'done' && phase !== 'error';
  const phases = mode === 'pdf' ? PDF_PHASES : TEXT_PHASES;

  // ---- PDF 文件处理 ----

  const handleFile = useCallback(
    (file: File) => {
      if (file.type !== 'application/pdf') {
        onStatusMessage('请选择 PDF 文件');
        return;
      }
      if (file.size > MAX_PDF_SIZE) {
        onStatusMessage(`文件大小超过 6 MB 限制（当前 ${(file.size / 1024 / 1024).toFixed(1)} MB）`);
        return;
      }

      setPdfFile(file);
      setExtractedText('');
      setExtractedYaml('');
      setProposal(null);
      setPhase('idle');
      setExtractError(null);

      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = reader.result as string;
        const base64 = dataUrl.split(',')[1] ?? '';
        setPdfBase64(base64);
      };
      reader.readAsDataURL(file);
    },
    [onStatusMessage],
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      const file = e.dataTransfer.files[0];
      if (file) handleFile(file);
    },
    [handleFile],
  );

  const handleFileInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) handleFile(file);
      e.target.value = '';
    },
    [handleFile],
  );

  // ---- AI 抽取 ----

  const handleExtractFromText = useCallback(async () => {
    if (!importText.trim()) return;
    setExtractError(null);
    setExtractedYaml('');
    setProposal(null);
    try {
      setPhase('calling-ai');
      const result = await extractProposal(importText);

      setPhase('parsing-result');
      setProposal(result);
      setExtractedYaml(result.deviceYaml);
      const msg = [
        'AI 抽取完成',
        result.uncertainties.length > 0 ? ` · ${result.uncertainties.length} 项待确认` : '',
        result.warnings.length > 0 ? ` · ${result.warnings.length} 条警告` : '',
      ].join('');
      onStatusMessage(msg);
      setPhase('done');
    } catch (error) {
      try {
        setPhase('calling-ai');
        const yaml = await extractFromText(importText);
        setPhase('parsing-result');
        setExtractedYaml(yaml);
        onStatusMessage('AI 抽取完成（基础模式）');
        setPhase('done');
      } catch (fallbackError) {
        setExtractError(`抽取失败: ${fallbackError}`);
        setPhase('error');
      }
    }
  }, [importText, extractProposal, extractFromText, onStatusMessage]);

  const handleExtractFromPdf = useCallback(async () => {
    if (!pdfBase64) return;
    setExtractError(null);
    setExtractedYaml('');
    setProposal(null);
    try {
      // 阶段 1：提取文本
      setPhase('extracting-text');
      const text = await extractTextFromPdf(pdfBase64);
      setExtractedText(text);

      // 阶段 2：AI 结构化抽取
      setPhase('calling-ai');
      const result = await extractFromPdf(pdfBase64);

      // 阶段 3：解析结果
      setPhase('parsing-result');
      setProposal(result);
      setExtractedYaml(result.deviceYaml);
      const msg = [
        'PDF 抽取完成',
        result.uncertainties.length > 0 ? ` · ${result.uncertainties.length} 项待确认` : '',
        result.warnings.length > 0 ? ` · ${result.warnings.length} 条警告` : '',
      ].join('');
      onStatusMessage(msg);
      setPhase('done');
    } catch (error) {
      setExtractError(`PDF 抽取失败: ${error}`);
      setPhase('error');
    }
  }, [pdfBase64, extractTextFromPdf, extractFromPdf, onStatusMessage]);

  // ---- 保存 ----

  const handleSave = useCallback(async () => {
    if (!extractedYaml) return;
    try {
      const idMatch = extractedYaml.match(/^id:\s*(.+)$/m);
      const typeMatch = extractedYaml.match(/^type:\s*(.+)$/m);
      const deviceId = idMatch?.[1]?.trim() ?? `device_${Date.now()}`;
      const deviceType = typeMatch?.[1]?.trim() ?? 'unknown';
      const name = deviceId.replace(/_/g, ' ');

      await saveAsset(deviceId, name, deviceType, extractedYaml);
      onStatusMessage(`设备 ${deviceId} 已保存`);

      if (proposal?.capabilityYamls.length) {
        for (const capYaml of proposal.capabilityYamls) {
          try {
            const capIdMatch = capYaml.match(/^id:\s*(.+)$/m);
            const capId = capIdMatch?.[1]?.trim() ?? `cap_${Date.now()}`;
            const descMatch = capYaml.match(/^description:\s*(.+)$/m);
            const desc = descMatch?.[1]?.trim() ?? capId;
            await saveCapability(capId, deviceId, desc, desc, capYaml);
          } catch {
            /* 单个能力保存失败不阻塞 */
          }
        }
        onStatusMessage(`设备 ${deviceId} + ${proposal.capabilityYamls.length} 个能力已保存`);
      }

      setProposal(null);
      setExtractedYaml('');
      setImportText('');
      setPdfFile(null);
      setPdfBase64(null);
      setExtractedText('');
      setExtractError(null);
      setPhase('idle');
      onSaved();
      onClose();
    } catch (error) {
      onStatusMessage(`保存设备失败: ${error}`);
    }
  }, [extractedYaml, proposal, saveAsset, saveCapability, onSaved, onClose, onStatusMessage]);

  // ---- 关闭时重置 ----

  const handleClose = useCallback(() => {
    setImportText('');
    setPdfFile(null);
    setPdfBase64(null);
    setExtractedText('');
    setExtractedYaml('');
    setProposal(null);
    setExtractError(null);
    setPhase('idle');
    setMode('text');
    onClose();
  }, [onClose]);

  return (
    <div className="dm-drawer__panel" onClick={(e) => e.stopPropagation()}>
        <div className="dm-drawer__header">
          <h2>从说明书导入设备</h2>
          <button type="button" className="dm-drawer__close" onClick={handleClose}>
            <XCloseIcon width={16} height={16} />
          </button>
        </div>

        <div className="dm-drawer__tabs">
          <button
            type="button"
            className={`dm-drawer__tab${mode === 'text' ? ' is-active' : ''}`}
            onClick={() => setMode('text')}
          >
            粘贴文本
          </button>
          <button
            type="button"
            className={`dm-drawer__tab${mode === 'pdf' ? ' is-active' : ''}`}
            onClick={() => setMode('pdf')}
          >
            <FilePdfIcon width={13} height={13} />
            上传 PDF
          </button>
        </div>

        <div className="dm-drawer__body">
          {mode === 'text' ? (
            <TextPasteView
              value={importText}
              onChange={setImportText}
              extracting={extracting}
              onExtract={handleExtractFromText}
            />
          ) : (
            <PdfUploadView
              file={pdfFile}
              dragOver={dragOver}
              extractedText={extractedText}
              showTextPreview={showTextPreview}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOver(true);
              }}
              onDragLeave={() => setDragOver(false)}
              onDrop={handleDrop}
              onFileInputChange={handleFileInputChange}
              onTogglePreview={() => setShowTextPreview((v) => !v)}
              fileInputRef={fileInputRef}
            />
          )}

          {/* 进度指示器 */}
          {extracting && (
            <ProgressIndicator phases={phases} currentPhase={phase} />
          )}

          {extractError && <div className="dm-drawer__error">{extractError}</div>}

          {mode === 'pdf' && pdfBase64 && !extractedYaml && !extracting && (
            <div className="dm-drawer__actions">
              <button
                type="button"
                className="dm-drawer__extract-btn"
                onClick={() => void handleExtractFromPdf()}
              >
                <SparklesIcon width={14} height={14} />
                AI 抽取
              </button>
            </div>
          )}

          {extractedYaml && (
            <ExtractionResult
              yaml={extractedYaml}
              proposal={proposal}
              extracting={extracting}
              onSave={() => void handleSave()}
            />
          )}
        </div>
      </div>
    );
}

// ---- 进度指示器 ----

function ProgressIndicator({
  phases,
  currentPhase,
}: {
  phases: PhaseInfo[];
  currentPhase: ExtractionPhase;
}) {
  const currentIdx = phases.findIndex((p) => p.phase === currentPhase);
  const currentLabel = phases[Math.max(currentIdx, 0)]?.label ?? '处理中...';

  return (
    <div className="dm-drawer__progress">
      <div className="dm-drawer__progress-bar">
        {phases.map((p, idx) => (
          <div
            key={p.phase}
            className={`dm-drawer__progress-step${
              idx < currentIdx ? ' is-done' : idx === currentIdx ? ' is-active' : ''
            }`}
          />
        ))}
      </div>
      <div className="dm-drawer__progress-label">
        <span className="dm-drawer__spinner" />
        {currentLabel}
      </div>
    </div>
  );
}

// ---- 子组件 ----

function TextPasteView({
  value,
  onChange,
  extracting,
  onExtract,
}: {
  value: string;
  onChange: (v: string) => void;
  extracting: boolean;
  onExtract: () => void;
}) {
  return (
    <div className="dm-drawer__text-area">
      <textarea
        className="dm-drawer__textarea"
        placeholder="粘贴设备说明书文本..."
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={10}
        disabled={extracting}
      />
      <div className="dm-drawer__actions">
        <button
          type="button"
          className="dm-drawer__extract-btn"
          disabled={extracting || !value.trim()}
          onClick={onExtract}
        >
          <SparklesIcon width={14} height={14} />
          AI 抽取
        </button>
      </div>
    </div>
  );
}

function PdfUploadView({
  file,
  dragOver,
  extractedText,
  showTextPreview,
  onDragOver,
  onDragLeave,
  onDrop,
  onFileInputChange,
  onTogglePreview,
  fileInputRef,
}: {
  file: File | null;
  dragOver: boolean;
  extractedText: string;
  showTextPreview: boolean;
  onDragOver: (e: React.DragEvent) => void;
  onDragLeave: () => void;
  onDrop: (e: React.DragEvent) => void;
  onFileInputChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onTogglePreview: () => void;
  fileInputRef: React.RefObject<HTMLInputElement | null>;
}) {
  return (
    <div className="dm-drawer__pdf-area">
      <div
        className={`dm-drawer__dropzone${dragOver ? ' is-active' : ''}${file ? ' has-file' : ''}`}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
        onClick={() => fileInputRef.current?.click()}
      >
        <UploadIcon width={24} height={24} />
        {file ? (
          <div className="dm-drawer__file-info">
            <FilePdfIcon width={16} height={16} />
            <span className="dm-drawer__file-name">{file.name}</span>
            <span className="dm-drawer__file-size">{(file.size / 1024).toFixed(0)} KB</span>
          </div>
        ) : (
          <p>拖拽 PDF 文件到此处，或点击选择</p>
        )}
      </div>
      <input
        ref={fileInputRef}
        type="file"
        accept=".pdf,application/pdf"
        hidden
        onChange={onFileInputChange}
      />

      {extractedText && (
        <details open={showTextPreview} className="dm-drawer__preview">
          <summary onClick={(e) => { e.preventDefault(); onTogglePreview(); }}>
            提取文本（{extractedText.length} 字符）
          </summary>
          <pre className="dm-drawer__preview-text">{extractedText}</pre>
          {extractedText.length > 5000 && (
            <span className="dm-drawer__preview-hint">
              完整文本已送入 AI 抽取，预览区域可滚动查看
            </span>
          )}
        </details>
      )}
    </div>
  );
}

function ExtractionResult({
  yaml,
  proposal,
  extracting,
  onSave,
}: {
  yaml: string;
  proposal: ExtractionProposal | null;
  extracting: boolean;
  onSave: () => void;
}) {
  return (
    <div className="dm-drawer__result">
      <div className="dm-drawer__result-header">
        <FileYamlIcon width={14} height={14} />
        <span>抽取结果</span>
        <button
          type="button"
          className="dm-drawer__save-btn"
          disabled={extracting}
          onClick={onSave}
        >
          保存{proposal?.capabilityYamls.length ? `（+${proposal.capabilityYamls.length} 能力）` : ''}
        </button>
      </div>
      <pre className="dm-drawer__result-yaml">{yaml}</pre>

      {proposal?.capabilityYamls.length ? (
        <details className="dm-drawer__capabilities">
          <summary>推断能力 ({proposal.capabilityYamls.length})</summary>
          {proposal.capabilityYamls.map((cap, idx) => (
            <pre key={idx} className="dm-drawer__result-yaml dm-drawer__result-yaml--small">{cap}</pre>
          ))}
        </details>
      ) : null}

      {proposal?.uncertainties.length ? (
        <div className="dm-drawer__uncertainties">
          <h4>待确认项 ({proposal.uncertainties.length})</h4>
          <ul>
            {proposal.uncertainties.map((u, idx) => (
              <li key={idx}>
                <code>{u.fieldPath}</code>：{u.guessedValue}
                <span className="dm-drawer__reason">{u.reason}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {proposal?.warnings.length ? (
        <div className="dm-drawer__warnings">
          <h4>警告 ({proposal.warnings.length})</h4>
          <ul>
            {proposal.warnings.map((w, idx) => (
              <li key={idx}>{w}</li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
