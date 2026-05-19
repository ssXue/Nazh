import { flushSync } from 'react-dom';

import { useCallback, useEffect, useRef, useState } from 'react';

import { useDeviceAssets } from '../../hooks/use-device-assets';
import { useCapabilities } from '../../hooks/use-capabilities';
import {
  FileJsonIcon,
  FilePdfIcon,
  FileYamlIcon,
  XCloseIcon,
} from './AppIcons';
import {
  ESI_PHASES,
  MAX_ESI_SIZE,
  MAX_PDF_SIZE,
  PDF_PHASES,
  TEXT_PHASES,
} from './device-import/types';
import {
  stripYamlQuotes,
} from './device-import/types';
import type {
  DeviceImportDrawerProps,
  ExtractionPhase,
  ExtractionProposal,
  InputMode,
} from './device-import/types';
import { ExtractionResult } from './device-import/ExtractionResult';
import { EsiUploadView } from './device-import/EsiUploadView';
import { PdfUploadView } from './device-import/PdfUploadView';
import { ProgressIndicator } from './device-import/ProgressIndicator';
import { TextPasteView } from './device-import/TextPasteView';

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

    await saveAsset(deviceId, name, deviceType, yaml);

    let savedCaps = 0;
    for (const capYaml of caps) {
      try {
        const capIdMatch = capYaml.match(/^id:\s*(.+)$/m);
        const capId = stripYamlQuotes(capIdMatch?.[1]) ?? `cap_${Date.now()}`;
        const descMatch = capYaml.match(/^description:\s*(.+)$/m);
        const desc = stripYamlQuotes(descMatch?.[1]) ?? capId;
        await saveCapability(capId, deviceId, desc, desc, capYaml);
        savedCaps += 1;
      } catch (_capErr) {
        // 单个能力保存失败不阻塞整体流程
      }
    }

    const msg = caps.length > 0
      ? `${label}：设备 ${deviceId} + ${savedCaps}/${caps.length} 个能力已保存`
      : `${label}：设备 ${deviceId} 已保存`;
    onStatusMessage(msg);
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

    const corrected = parseAiStreamJson(rawJson);

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

    let yaml = '';
    let caps: string[] = [];

    // 阶段 1：抽取
    try {
      setPhase('calling-ai');
      const result = await extractProposal(importText);
      setPhase('parsing-result');
      yaml = result.deviceYamls[0] ?? '';
      caps = result.capabilityYamls;
      setProposal(result);
      setExtractedYaml(yaml);
    } catch (_proposalError) {
      // Proposal 模式失败，回退到基础模式
      try {
        setPhase('calling-ai');
        yaml = await extractFromText(importText);
        setPhase('parsing-result');
        setExtractedYaml(yaml);
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

    let yaml = '';
    let caps: string[] = [];

    try {
      // 阶段 1：提取文本
      setPhase('extracting-text');
      const text = await extractTextFromPdf(b64);

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

      // 解析 AI 输出
      setPhase('parsing-result');
      const parsed = parseAiStreamJson(rawJson);
      yaml = parsed.yaml;
      caps = parsed.caps;
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
    try {
      setPhase('parsing-result');
      const result = await importEthercatEsi(esiXml);

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
