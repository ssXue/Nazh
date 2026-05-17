import { FlowDownloadFormat, FlowDownloadService } from '@flowgram.ai/export-plugin';
import type { FreeLayoutPluginContext } from '@flowgram.ai/free-layout-editor';
import { useCallback } from 'react';

import { hasTauriRuntime, saveFlowgramExportFile } from '../../lib/tauri';
import { buildFlowgramExportFileName } from './flowgram-canvas-utils';

interface InternalFlowExportImageService {
  export: (options: { format: FlowDownloadFormat; watermarkSVG?: string }) => Promise<string | undefined>;
}

interface InternalFlowDownloadService {
  download: (params: { format: FlowDownloadFormat }) => Promise<void>;
  document: {
    toJSON: () => unknown;
  };
  exportImageService: InternalFlowExportImageService;
  options?: {
    watermarkSVG?: string;
  };
  formatDataContent: (
    json: unknown,
    format: FlowDownloadFormat,
  ) => Promise<{
    content: string;
    mimeType: string;
  }>;
  setDownloading: (value: boolean) => void;
}

interface UseFlowgramExportOptions {
  editorCtx: FreeLayoutPluginContext | null;
  workflowName?: string | null;
  workspacePath?: string;
  onStatusMessage?: (message: string) => void;
  reportFlowgramError: (title: string, error: unknown) => void;
}

export function useFlowgramExport({
  editorCtx,
  workflowName,
  workspacePath,
  onStatusMessage,
  reportFlowgramError,
}: UseFlowgramExportOptions) {
  return useCallback(
    async (format: FlowDownloadFormat) => {
      if (!editorCtx) {
        return;
      }

      try {
        const downloadService = (editorCtx as FreeLayoutPluginContext & {
          get?: <T>(token: unknown) => T;
        }).get?.<FlowDownloadService>(FlowDownloadService) as unknown as
          | InternalFlowDownloadService
          | undefined;
        if (!downloadService) {
          return;
        }

        if (hasTauriRuntime()) {
          downloadService.setDownloading(true);

          try {
            const fileName = buildFlowgramExportFileName(workflowName, format);

            if (format === FlowDownloadFormat.JSON) {
              const json = downloadService.document.toJSON();
              const { content } = await downloadService.formatDataContent(json, format);
              const saved = await saveFlowgramExportFile(workspacePath ?? '', fileName, {
                text: content,
              });
              onStatusMessage?.(`已导出到 ${saved.filePath}`);
              return;
            }

            const imageUrl = await downloadService.exportImageService.export({
              format,
              watermarkSVG: downloadService.options?.watermarkSVG,
            });
            if (!imageUrl) {
              throw new Error('未能生成导出内容。');
            }

            const response = await fetch(imageUrl);
            if (!response.ok) {
              throw new Error(`导出内容读取失败: ${response.status}`);
            }

            const buffer = await response.arrayBuffer();
            const saved = await saveFlowgramExportFile(workspacePath ?? '', fileName, {
              bytes: Array.from(new Uint8Array(buffer)),
            });
            onStatusMessage?.(`已导出到 ${saved.filePath}`);
            return;
          } finally {
            downloadService.setDownloading(false);
          }
        }

        await downloadService.download({ format });
      } catch (error) {
        reportFlowgramError('FlowGram 导出失败', error);
      }
    },
    [editorCtx, onStatusMessage, reportFlowgramError, workflowName, workspacePath],
  );
}
