import { useCallback, useEffect, useMemo, useState } from 'react';

import { JsonCodeEditor } from '@flowgram.ai/form-materials';
import { useClientContext } from '@flowgram.ai/free-layout-editor';
import { type PanelFactory, usePanelManager } from '@flowgram.ai/panel-manager-plugin';
import {
  type IReport,
  type TaskValidateOutput,
  type WorkflowInputs,
  type WorkflowOutputs,
  WorkflowStatus,
} from '@flowgram.ai/runtime-interface';
import {
  TaskCancelAPI,
  TaskReportAPI,
  TaskResultAPI,
  TaskRunAPI,
  TaskValidateAPI,
} from '@flowgram.ai/runtime-js';

export const FLOWGRAM_RUNTIME_PANEL_KEY = 'nazh-flowgram-runtime';

function parseInputText(inputText: string): {
  inputs: WorkflowInputs | null;
  error: string | null;
} {
  const normalized = inputText.trim();

  if (!normalized) {
    return {
      inputs: {},
      error: null,
    };
  }

  try {
    const parsed = JSON.parse(normalized) as WorkflowInputs;
    return {
      inputs: parsed && typeof parsed === 'object' ? parsed : {},
      error: null,
    };
  } catch (error) {
    return {
      inputs: null,
      error: error instanceof Error ? error.message : '输入 JSON 解析失败',
    };
  }
}

function readRuntimeErrors(report: IReport | null): string[] {
  if (!report) {
    return [];
  }

  return report.messages?.error?.map((message) => {
    if (message.nodeID) {
      return `${message.nodeID}: ${message.message}`;
    }

    return message.message;
  }) ?? [];
}

function FlowgramRuntimePanel() {
  const panelManager = usePanelManager();
  const { document } = useClientContext();
  const [inputText, setInputText] = useState('{}');
  const [inputError, setInputError] = useState<string | null>(null);
  const [validation, setValidation] = useState<TaskValidateOutput | null>(null);
  const [isValidating, setIsValidating] = useState(false);
  const [isRunning, setIsRunning] = useState(false);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [report, setReport] = useState<IReport | null>(null);
  const [outputs, setOutputs] = useState<WorkflowOutputs | null>(null);
  const [runtimeErrors, setRuntimeErrors] = useState<string[]>([]);

  const closePanel = useCallback(() => {
    panelManager.close(FLOWGRAM_RUNTIME_PANEL_KEY, 'bottom');
  }, [panelManager]);

  const cancelTask = useCallback(async () => {
    if (!taskId) {
      return;
    }

    try {
      await TaskCancelAPI({
        taskID: taskId,
      });
    } finally {
      setTaskId(null);
      setIsRunning(false);
    }
  }, [taskId]);

  const validateCurrentSchema = useCallback(async () => {
    const schema = JSON.stringify(document.toJSON());
    const parsedInputState = parseInputText(inputText);

    setInputError(parsedInputState.error);

    if (!parsedInputState.inputs) {
      setValidation({
        valid: false,
        errors: ['输入 JSON 无法解析。'],
      });
      return {
        inputs: null,
        validation: {
          valid: false,
          errors: ['输入 JSON 无法解析。'],
        } satisfies TaskValidateOutput,
      };
    }

    setIsValidating(true);

    try {
      const nextValidation = await TaskValidateAPI({
        schema,
        inputs: parsedInputState.inputs,
      });

      setValidation(nextValidation);
      return {
        inputs: parsedInputState.inputs,
        validation: nextValidation,
      };
    } finally {
      setIsValidating(false);
    }
  }, [document, inputText]);

  const handleRun = useCallback(async () => {
    const validationResult = await validateCurrentSchema();

    if (!validationResult?.inputs || !validationResult.validation.valid) {
      return;
    }

    const schema = JSON.stringify(document.toJSON());

    setOutputs(null);
    setRuntimeErrors([]);
    setReport(null);

    const runResult = await TaskRunAPI({
      schema,
      inputs: validationResult.inputs,
    });

    setTaskId(runResult.taskID);
    setIsRunning(true);
  }, [document, validateCurrentSchema]);

  const handleClose = useCallback(async () => {
    await cancelTask();
    closePanel();
  }, [cancelTask, closePanel]);

  useEffect(() => {
    void validateCurrentSchema();
  }, [validateCurrentSchema]);

  useEffect(() => {
    if (!taskId) {
      return;
    }

    let cancelled = false;

    const intervalId = window.setInterval(() => {
      void (async () => {
        const nextReport = await TaskReportAPI({
          taskID: taskId,
        });

        if (cancelled || !nextReport) {
          return;
        }

        setReport(nextReport);
        setRuntimeErrors(readRuntimeErrors(nextReport));

        if (!nextReport.workflowStatus.terminated) {
          return;
        }

        window.clearInterval(intervalId);

        const nextOutputs = await TaskResultAPI({
          taskID: taskId,
        });

        if (cancelled) {
          return;
        }

        setOutputs(nextOutputs ?? null);
        setTaskId(null);
        setIsRunning(false);
      })();
    }, 420);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [taskId]);

  const outputsText = useMemo(
    () => (outputs ? JSON.stringify(outputs, null, 2) : '{}'),
    [outputs],
  );
  const workflowStatusLabel = report?.workflowStatus.status ?? WorkflowStatus.Pending;
  const validationErrors = validation?.errors ?? [];

  return (
    <section className="flowgram-floating-panel flowgram-floating-panel--runtime">
      <div className="flowgram-floating-panel__header">
        <h3>FlowGram 预检</h3>
        <div className="flowgram-floating-panel__actions">
          <button
            type="button"
            className="ghost flowgram-floating-panel__close"
            onClick={() => void validateCurrentSchema()}
            disabled={isValidating}
          >
            校验
          </button>
          <button
            type="button"
            className="ghost flowgram-floating-panel__close"
            onClick={() => void handleRun()}
            disabled={isValidating || isRunning || validation?.valid === false}
          >
            试运行
          </button>
          {isRunning ? (
            <button
              type="button"
              className="ghost flowgram-floating-panel__close"
              onClick={() => void cancelTask()}
            >
              停止
            </button>
          ) : null}
          <button type="button" className="ghost flowgram-floating-panel__close" onClick={() => void handleClose()}>
            关闭
          </button>
        </div>
      </div>

      <div className="flowgram-runtime-grid">
        <article className="flowgram-runtime-card">
          <div className="flowgram-runtime-card__header">
            <strong>输入</strong>
            <span>{isValidating ? '校验中' : inputError ? '格式错误' : '已就绪'}</span>
          </div>
          <div className="flowgram-runtime-editor">
            <JsonCodeEditor value={inputText} onChange={setInputText} />
          </div>
          {inputError ? <p className="panel__error">{inputError}</p> : null}
        </article>

        <article className="flowgram-runtime-card">
          <div className="flowgram-runtime-card__header">
            <strong>状态</strong>
            <span>{workflowStatusLabel}</span>
          </div>
          <div className="flowgram-runtime-status">
            <span>{validation?.valid ? '兼容' : '待兼容'}</span>
            <span>{report?.workflowStatus.timeCost ? `${report.workflowStatus.timeCost} ms` : '--'}</span>
            <span>{taskId ?? '未运行'}</span>
          </div>
          <div className="flowgram-notes">
            {(validationErrors.length > 0 ? validationErrors : ['当前 schema 通过 FlowGram runtime 校验。']).map(
              (message) => (
                <article
                  key={`validation:${message}`}
                  className={`flowgram-note ${validation?.valid ? 'flowgram-note--info' : 'flowgram-note--warning'}`}
                >
                  {message}
                </article>
              ),
            )}
            {runtimeErrors.map((message) => (
              <article key={`runtime:${message}`} className="flowgram-note flowgram-note--danger">
                {message}
              </article>
            ))}
          </div>
        </article>

        <article className="flowgram-runtime-card flowgram-runtime-card--wide">
          <div className="flowgram-runtime-card__header">
            <strong>输出</strong>
            <span>{outputs ? '已返回' : '暂无'}</span>
          </div>
          <div className="flowgram-runtime-editor">
            <JsonCodeEditor value={outputsText} readonly />
          </div>
        </article>
      </div>
    </section>
  );
}

export const flowgramRuntimePanelFactory: PanelFactory<Record<string, never>> = {
  key: FLOWGRAM_RUNTIME_PANEL_KEY,
  render: () => <FlowgramRuntimePanel />,
};
