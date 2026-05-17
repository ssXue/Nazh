import { flushSync } from 'react-dom';

import { useCallback, useEffect, useRef, useState } from 'react';

import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { ExtractionProposal } from '../../hooks/use-device-assets';
import { useCapabilities } from '../../hooks/use-capabilities';
import {
  SparklesIcon,
  FileJsonIcon,
  FilePdfIcon,
  FileYamlIcon,
  UploadIcon,
  XCloseIcon,
} from './AppIcons';

/** 文件大小上限：6 MB（base64 编码后约 8 MB，留余量给 10 MB IPC 限制）。 */
const MAX_PDF_SIZE = 6 * 1024 * 1024;
/** ESI XML 文件大小上限：2 MB。 */
const MAX_ESI_SIZE = 2 * 1024 * 1024;

/** 从 YAML 行尾片段去掉首尾的单/双引号，处理 serde_yaml 对纯数字字符串自动加引号的情况。 */
function stripYamlQuotes(value: string | undefined): string | undefined {
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
  { phase: 'extracting-text', label: '提取文本内容...' },
  { phase: 'calling-ai', label: 'AI 分析说明书中...' },
];

const TEXT_PHASES: PhaseInfo[] = [
  { phase: 'calling-ai', label: 'AI 分析文本中...' },
  { phase: 'parsing-result', label: '解析抽取结果...' },
];

const ESI_PHASES: PhaseInfo[] = [
  { phase: 'parsing-result', label: '解析 ESI 文件...' },
];

interface DeviceImportDrawerProps {
  workspacePath: string;
  onClose: () => void;
  onSaved: () => void;
  onStatusMessage: (message: string) => void;
}

type InputMode = 'text' | 'pdf' | 'esi';

export function DeviceImportDrawer({
  workspacePath,
  onClose,
  onSaved,
  onStatusMessage,
}: DeviceImportDrawerProps) {
  const { extractFromText, extractProposal, extractTextFromPdf, extractProposalStream, importEthercatEsi, saveAsset } =
    useDeviceAssets(workspacePath);
  const { saveCapability } = useCapabilities(workspacePath);

  const [mode, setMode] = useState<InputMode>('text');

  // 文本粘贴状态
  const [importText, setImportText] = useState('');

  // PDF 上传状态
  const [pdfFile, setPdfFile] = useState<File | null>(null);
  const [pdfBase64, setPdfBase64] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // EtherCAT ESI 上传状态
  const [esiFile, setEsiFile] = useState<File | null>(null);
  const [esiXml, setEsiXml] = useState('');
  const [esiDragOver, setEsiDragOver] = useState(false);
  const esiFileInputRef = useRef<HTMLInputElement>(null);

  // AI 抽取状态
  const [extractedYaml, setExtractedYaml] = useState('');
  const [streamingText, setStreamingText] = useState('');
  const [thinkingText, setThinkingText] = useState('');
  const [phase, setPhase] = useState<ExtractionPhase>('idle');
  const [extractError, setExtractError] = useState<string | null>(null);
  const [proposal, setProposal] = useState<ExtractionProposal | null>(null);

  const extracting = phase !== 'idle' && phase !== 'done' && phase !== 'error';
  const phases = mode === 'pdf' ? PDF_PHASES : mode === 'esi' ? ESI_PHASES : TEXT_PHASES;

  // 流式区域自动滚动到底部
  const streamingPreRef = useRef<HTMLPreElement>(null);
  const thinkingPreRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    streamingPreRef.current?.scrollTo(0, streamingPreRef.current.scrollHeight);
  }, [streamingText]);

  useEffect(() => {
    thinkingPreRef.current?.scrollTo(0, thinkingPreRef.current.scrollHeight);
  }, [thinkingText]);

  // 自动抽取的 ref（避免 handleFile ↔ handleExtractFromPdf 循环依赖）
  const extractFromPdfRef = useRef<(base64: string) => Promise<void>>(undefined as unknown as (base64: string) => Promise<void>);

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
      setExtractedYaml('');
      setStreamingText('');
      setProposal(null);
      setPhase('idle');
      setExtractError(null);

      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = reader.result as string;
        const base64 = dataUrl.split(',')[1] ?? '';
        setPdfBase64(base64);
        // 自动开始抽取
        if (extractFromPdfRef.current) {
          setTimeout(() => void extractFromPdfRef.current?.(base64), 0);
        }
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

  // ---- EtherCAT ESI 文件处理 ----

  const handleEsiFile = useCallback(
    (file: File) => {
      const lowerName = file.name.toLowerCase();
      if (!lowerName.endsWith('.xml') && !lowerName.endsWith('.esi')) {
        onStatusMessage('请选择 ESI XML 文件');
        return;
      }
      if (file.size > MAX_ESI_SIZE) {
        onStatusMessage(`文件大小超过 2 MB 限制（当前 ${(file.size / 1024 / 1024).toFixed(1)} MB）`);
        return;
      }

      setEsiFile(file);
      setEsiXml('');
      setExtractedYaml('');
      setProposal(null);
      setPhase('idle');
      setExtractError(null);

      const reader = new FileReader();
      reader.onload = () => setEsiXml(typeof reader.result === 'string' ? reader.result : '');
      reader.onerror = () => onStatusMessage('读取 ESI 文件失败');
      reader.readAsText(file);
    },
    [onStatusMessage],
  );

  const handleEsiDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setEsiDragOver(false);
      const file = e.dataTransfer.files[0];
      if (file) handleEsiFile(file);
    },
    [handleEsiFile],
  );

  const handleEsiFileInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) handleEsiFile(file);
      e.target.value = '';
    },
    [handleEsiFile],
  );

  // ---- 保存 ----

  const resetState = useCallback(() => {
    setProposal(null);
    setExtractedYaml('');
    setStreamingText('');
    setThinkingText('');
    setImportText('');
    setPdfFile(null);
    setPdfBase64(null);
    setEsiFile(null);
    setEsiXml('');
    setExtractError(null);
    setPhase('idle');
  }, []);

  /** 从抽取结果 YAML 中提取 id/type/model 并保存设备 + 关联能力。 */
  const saveExtractedDevice = useCallback(async (
    yaml: string,
    caps: string[],
    label: string,
  ): Promise<string> => {
    const idMatch = yaml.match(/^id:\s*(.+)$/m);
    const typeMatch = yaml.match(/^type:\s*(.+)$/m);
    const modelMatch = yaml.match(/^model:\s*(.+)$/m);
    const deviceId = stripYamlQuotes(idMatch?.[1]) ?? `device_${Date.now()}`;
    const deviceType = stripYamlQuotes(typeMatch?.[1]) ?? 'unknown';
    const name = stripYamlQuotes(modelMatch?.[1]) ?? deviceId.replace(/_/g, ' ');

    console.log('[DeviceImport] 开始保存:', { label, deviceId, deviceType, name, yamlLen: yaml.length, capCount: caps.length });

    await saveAsset(deviceId, name, deviceType, yaml);
    console.log('[DeviceImport] 设备保存成功:', deviceId);

    let savedCaps = 0;
    for (const capYaml of caps) {
      try {
        const capIdMatch = capYaml.match(/^id:\s*(.+)$/m);
        const capId = stripYamlQuotes(capIdMatch?.[1]) ?? `cap_${Date.now()}`;
        const descMatch = capYaml.match(/^description:\s*(.+)$/m);
        const desc = stripYamlQuotes(descMatch?.[1]) ?? capId;
        await saveCapability(capId, deviceId, desc, desc, capYaml);
        savedCaps += 1;
      } catch (capErr) {
        console.warn('[DeviceImport] 单个能力保存失败（不阻塞）:', capErr);
      }
    }

    const msg = caps.length > 0
      ? `${label}：设备 ${deviceId} + ${savedCaps}/${caps.length} 个能力已保存`
      : `${label}：设备 ${deviceId} 已保存`;
    onStatusMessage(msg);
    console.log('[DeviceImport] 保存流程完成:', { deviceId, savedCaps });
    return deviceId;
  }, [saveAsset, saveCapability, onStatusMessage]);

  /** 解析 AI 流式输出的原始 JSON，提取 deviceYaml + capabilityYamls。 */
  const parseAiStreamJson = useCallback((rawJson: string): { yaml: string; caps: string[] } => {
    const trimmed = rawJson.trim();
    let jsonStr = trimmed;
    if (jsonStr.startsWith('```json') || jsonStr.startsWith('```')) {
      const start = jsonStr.indexOf('\n') + 1;
      const end = jsonStr.lastIndexOf('```');
      if (end > start) {
        jsonStr = jsonStr.slice(start, end).trim();
      }
    }
    const raw = JSON.parse(jsonStr) as {
      deviceYaml: string;
      capabilityYamls?: string[];
      uncertainties?: Array<{ fieldPath: string; guessedValue: string; reason: string }>;
      warnings?: string[];
    };
    return { yaml: raw.deviceYaml, caps: raw.capabilityYamls ?? [] };
  }, []);

  /** 保存失败后，将错误回传 AI 进行一次自校正重试。 */
  const retryWithAiCorrection = useCallback(async (
    failedYaml: string,
    caps: string[],
    errorMessage: string,
    label: string,
  ): Promise<string> => {
    console.log('[DeviceImport] AI 自校正开始, 错误:', errorMessage);
    setPhase('calling-ai');
    onStatusMessage(`${label}：保存验证失败，AI 正在自动修正...`);

    const rawJson = await extractProposalStream(
      '', // 校正模式不使用原始文本
      (accumulated) => {
        flushSync(() => setStreamingText(accumulated));
      },
      (thinking) => {
        flushSync(() => setThinkingText(thinking));
      },
      undefined,
      { yaml: failedYaml, error: errorMessage },
    );

    console.log('[DeviceImport] AI 校正输出完成, 长度:', rawJson.length);
    const corrected = parseAiStreamJson(rawJson);
    console.log('[DeviceImport] AI 校正解析完成:', {
      yamlLen: corrected.yaml.length,
      capCount: corrected.caps.length,
    });

    setExtractedYaml(corrected.yaml);
    setStreamingText('');

    // 用修正后的 YAML 重试保存
    return saveExtractedDevice(corrected.yaml, corrected.caps.length > 0 ? corrected.caps : caps, `${label}（AI 修正）`);
  }, [extractProposalStream, parseAiStreamJson, saveExtractedDevice, onStatusMessage]);

  // ---- AI 抽取 ----

  const handleExtractFromText = useCallback(async () => {
    if (!importText.trim()) return;
    setExtractError(null);
    setExtractedYaml('');
    setProposal(null);
    console.log('[DeviceImport] 文本抽取开始, 输入长度:', importText.length);

    let yaml = '';
    let caps: string[] = [];

    // 阶段 1：抽取
    try {
      setPhase('calling-ai');
      const result = await extractProposal(importText);
      console.log('[DeviceImport] 文本抽取完成:', {
        deviceCount: result.deviceYamls.length,
        capCount: result.capabilityYamls.length,
        uncertaintyCount: result.uncertainties.length,
        warningCount: result.warnings.length,
      });
      setPhase('parsing-result');
      yaml = result.deviceYamls[0] ?? '';
      caps = result.capabilityYamls;
      setProposal(result);
      setExtractedYaml(yaml);
    } catch (error) {
      console.warn('[DeviceImport] 文本 Proposal 抽取失败, 尝试基础模式:', error);
      try {
        setPhase('calling-ai');
        yaml = await extractFromText(importText);
        setPhase('parsing-result');
        setExtractedYaml(yaml);
        console.log('[DeviceImport] 基础模式抽取完成, yaml 长度:', yaml.length);
      } catch (fallbackError) {
        console.error('[DeviceImport] 基础模式也失败:', fallbackError);
        setExtractError(`抽取失败: ${fallbackError}`);
        setPhase('error');
        return;
      }
    }

    // 阶段 2：自动保存（失败则 AI 校正重试一次）
    try {
      await saveExtractedDevice(yaml, caps, caps.length > 0 ? '文本抽取' : '文本抽取（基础模式）');
      resetState();
      onSaved();
      onClose();
    } catch (saveError) {
      console.error('[DeviceImport] 自动保存失败, 尝试 AI 校正:', saveError);
      try {
        await retryWithAiCorrection(yaml, caps, String(saveError), '文本抽取');
        resetState();
        onSaved();
        onClose();
      } catch (correctionError) {
        console.error('[DeviceImport] AI 校正后仍然失败:', correctionError);
        setExtractError(`保存设备失败: ${correctionError}`);
        setPhase('error');
      }
    }
  }, [importText, extractProposal, extractFromText, saveExtractedDevice, retryWithAiCorrection, resetState, onSaved, onClose]);

  const handleExtractFromPdf = useCallback(async (base64?: string) => {
    const b64 = base64 ?? pdfBase64;
    if (!b64) return;
    setExtractError(null);
    setExtractedYaml('');
    setStreamingText('');
    setThinkingText('');
    setProposal(null);
    console.log('[DeviceImport] PDF 抽取开始');

    let yaml = '';
    let caps: string[] = [];

    try {
      // 阶段 1：提取文本
      setPhase('extracting-text');
      const text = await extractTextFromPdf(b64);
      console.log('[DeviceImport] PDF 文本提取完成, 长度:', text.length);

      // 阶段 2：流式 AI 结构化抽取
      setPhase('calling-ai');
      const rawJson = await extractProposalStream(
        text,
        (accumulated) => {
          flushSync(() => setStreamingText(accumulated));
        },
        (thinking) => {
          flushSync(() => setThinkingText(thinking));
        },
      );
      console.log('[DeviceImport] AI 流式输出完成, 原始长度:', rawJson.length, '前 200 字符:', rawJson.slice(0, 200));

      // 解析 AI 输出
      setPhase('parsing-result');
      const parsed = parseAiStreamJson(rawJson);
      yaml = parsed.yaml;
      caps = parsed.caps;
      console.log('[DeviceImport] AI 输出解析完成:', { yamlLen: yaml.length, capCount: caps.length });
      setExtractedYaml(yaml);
    } catch (error) {
      console.error('[DeviceImport] PDF 抽取失败:', error);
      setExtractError(`PDF 抽取失败: ${error}`);
      setPhase('error');
      return;
    }

    // 阶段 3：自动保存（失败则 AI 校正重试一次）
    try {
      await saveExtractedDevice(yaml, caps, 'PDF 抽取');
      resetState();
      onSaved();
      onClose();
    } catch (saveError) {
      console.error('[DeviceImport] PDF 自动保存失败, 尝试 AI 校正:', saveError);
      try {
        await retryWithAiCorrection(yaml, caps, String(saveError), 'PDF 抽取');
        resetState();
        onSaved();
        onClose();
      } catch (correctionError) {
        console.error('[DeviceImport] AI 校正后仍然失败:', correctionError);
        setExtractError(`保存设备失败: ${correctionError}`);
        setPhase('error');
      }
    }
  }, [pdfBase64, extractTextFromPdf, extractProposalStream, parseAiStreamJson, saveExtractedDevice, retryWithAiCorrection, resetState, onSaved, onClose]);

  // 注册到 ref 供 handleFile 自动调用
  extractFromPdfRef.current = handleExtractFromPdf;

  const handleImportEsi = useCallback(async () => {
    if (!esiXml.trim()) return;
    setExtractError(null);
    setExtractedYaml('');
    setProposal(null);
    console.log('[DeviceImport] ESI 导入开始, XML 长度:', esiXml.length);
    try {
      setPhase('parsing-result');
      const result = await importEthercatEsi(esiXml);
      console.log('[DeviceImport] ESI 解析完成:', {
        deviceCount: result.deviceYamls.length,
        capCount: result.capabilityYamls.length,
        warningCount: result.warnings.length,
      });

      // ESI/ENI 导入结果确定性，直接保存到磁盘
      for (const yaml of result.deviceYamls) {
        await saveExtractedDevice(yaml, result.capabilityYamls, 'ESI 导入');
      }

      const deviceCount = result.deviceYamls.length;
      const msg = [
        deviceCount > 1 ? `ESI 导入完成 · ${deviceCount} 个设备` : 'ESI 导入完成',
        result.warnings.length > 0 ? ` · ${result.warnings.length} 条提示` : '',
      ].join('');
      onStatusMessage(msg);
      resetState();
      onSaved();
      onClose();
    } catch (error) {
      console.error('[DeviceImport] ESI 导入失败:', error);
      setExtractError(`ESI 导入失败: ${error}`);
      setPhase('error');
    }
  }, [esiXml, importEthercatEsi, saveExtractedDevice, resetState, onSaved, onClose, onStatusMessage]);

  // ---- 关闭时重置 ----

  const handleClose = useCallback(() => {
    setImportText('');
    setPdfFile(null);
    setPdfBase64(null);
    setEsiFile(null);
    setEsiXml('');
    setStreamingText('');
    setThinkingText('');
    setExtractedYaml('');
    setProposal(null);
    setExtractError(null);
    setPhase('idle');
    setMode('text');
    onClose();
  }, [onClose]);

  return (
    <div className="dm-drawer__panel" data-testid="device-import-drawer" onClick={(e) => e.stopPropagation()}>
        <div className="dm-drawer__header">
          <h2>从说明书导入设备</h2>
          <button type="button" className="dm-drawer__close" data-testid="device-import-close" onClick={handleClose}>
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
          <button
            type="button"
            className={`dm-drawer__tab${mode === 'esi' ? ' is-active' : ''}`}
            onClick={() => setMode('esi')}
          >
            <FileJsonIcon width={13} height={13} />
            EtherCAT ESI
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
          ) : mode === 'pdf' ? (
            <PdfUploadView
              file={pdfFile}
              dragOver={dragOver}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOver(true);
              }}
              onDragLeave={() => setDragOver(false)}
              onDrop={handleDrop}
              onFileInputChange={handleFileInputChange}
              fileInputRef={fileInputRef}
            />
          ) : (
            <EsiUploadView
              file={esiFile}
              dragOver={esiDragOver}
              xml={esiXml}
              onDragOver={(e) => {
                e.preventDefault();
                setEsiDragOver(true);
              }}
              onDragLeave={() => setEsiDragOver(false)}
              onDrop={handleEsiDrop}
              onFileInputChange={handleEsiFileInputChange}
              fileInputRef={esiFileInputRef}
            />
          )}

          {/* 进度指示器 */}
          {extracting && (
            <ProgressIndicator phases={phases} currentPhase={phase} />
          )}

          {extractError && <div className="dm-drawer__error">{extractError}</div>}

          {/* AI 思考过程（结果出来后自动隐藏） */}
          {thinkingText && !extractedYaml && (
            <details className="dm-drawer__thinking" open>
              <summary>AI 思考中...</summary>
              <pre className="dm-drawer__thinking-text" ref={thinkingPreRef}>{thinkingText}</pre>
            </details>
          )}

          {/* 流式输出区（结果出来后自动隐藏） */}
          {streamingText && !extractedYaml && (
            <div className="dm-drawer__streaming">
              <div className="dm-drawer__streaming-header">
                <span className="dm-drawer__spinner" />
                AI 输出中...
              </div>
              <pre className="dm-drawer__streaming-text" ref={streamingPreRef}>{streamingText}</pre>
            </div>
          )}

          {mode === 'esi' && esiXml && !extracting && (
            <div className="dm-drawer__actions">
              <button
                type="button"
                className="dm-drawer__extract-btn"
                onClick={() => void handleImportEsi()}
              >
                <FileYamlIcon width={14} height={14} />
                导入并保存
              </button>
            </div>
          )}

          {extractedYaml && (
            <ExtractionResult
              yaml={extractedYaml}
              proposal={proposal}
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
  onDragOver,
  onDragLeave,
  onDrop,
  onFileInputChange,
  fileInputRef,
}: {
  file: File | null;
  dragOver: boolean;
  onDragOver: (e: React.DragEvent) => void;
  onDragLeave: () => void;
  onDrop: (e: React.DragEvent) => void;
  onFileInputChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
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
    </div>
  );
}

function EsiUploadView({
  file,
  dragOver,
  xml,
  onDragOver,
  onDragLeave,
  onDrop,
  onFileInputChange,
  fileInputRef,
}: {
  file: File | null;
  dragOver: boolean;
  xml: string;
  onDragOver: (e: React.DragEvent) => void;
  onDragLeave: () => void;
  onDrop: (e: React.DragEvent) => void;
  onFileInputChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
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
            <FileJsonIcon width={16} height={16} />
            <span className="dm-drawer__file-name">{file.name}</span>
            <span className="dm-drawer__file-size">{(file.size / 1024).toFixed(0)} KB</span>
          </div>
        ) : (
          <p>拖拽 .xml / .esi 文件到此处，或点击选择</p>
        )}
      </div>
      <input
        ref={fileInputRef}
        type="file"
        accept=".xml,.esi,text/xml,application/xml"
        hidden
        onChange={onFileInputChange}
      />
      {xml ? (
        <details className="dm-drawer__preview">
          <summary>ESI XML（{xml.length} 字符）</summary>
          <pre className="dm-drawer__preview-text">{xml.slice(0, 5000)}</pre>
        </details>
      ) : null}
    </div>
  );
}

function ExtractionResult({
  yaml,
  proposal,
}: {
  yaml: string;
  proposal: ExtractionProposal | null;
}) {
  return (
    <div className="dm-drawer__result">
      <div className="dm-drawer__result-header">
        <FileYamlIcon width={14} height={14} />
        <span>抽取结果</span>
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
