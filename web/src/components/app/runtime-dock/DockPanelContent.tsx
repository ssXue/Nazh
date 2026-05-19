import { useState } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import {
  CopyIcon,
  SparklesIcon,
} from '../AppIcons';
import { sendToCopilot } from '../../../lib/copilot-send';
import { JsonCodeEditor } from '@flowgram.ai/form-materials';
import {
  buildRuntimeConsoleEntries,
  formatLogTimestamp,
} from '../runtime-console';
import { RuntimeVariablesPanel } from '../RuntimeVariablesPanel';
import { GlobalVariablesPanel } from '../GlobalVariablesPanel';
import type { RuntimeDockProps } from '../types';
import { normalizeResultPayload } from './types';
import type { RuntimeDockPanel } from './types';
import { ConnectionTable } from './ConnectionTable';

// ── 变量面板（子标签切换工作流/全局）──

type VariableSubTab = 'workflow' | 'global';

function VariablesPanelWithSubTabs({
  activeWorkflowId,
  onVariableCountChange,
}: {
  activeWorkflowId: RuntimeDockProps['activeWorkflowId'];
  onVariableCountChange: (count: number) => void;
}) {
  const [subTab, setSubTab] = useState<VariableSubTab>('workflow');

  return (
    <section className="runtime-dock__panel is-active" role="tabpanel">
      <div className="runtime-dock__panel-header">
        <h3>运行时变量</h3>
        <div className="runtime-dock__sub-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            data-testid="variable-tab-workflow"
            className={`runtime-dock__sub-tab ${subTab === 'workflow' ? 'is-active' : ''}`}
            aria-selected={subTab === 'workflow'}
            onClick={() => setSubTab('workflow')}
          >
            工作流变量
          </button>
          <button
            type="button"
            role="tab"
            data-testid="variable-tab-global"
            className={`runtime-dock__sub-tab ${subTab === 'global' ? 'is-active' : ''}`}
            aria-selected={subTab === 'global'}
            onClick={() => setSubTab('global')}
          >
            全局变量
          </button>
        </div>
      </div>
      <div className="runtime-dock__panel-body">
        {subTab === 'workflow' ? (
          <RuntimeVariablesPanel
            workflowId={activeWorkflowId}
            onVariableCountChange={onVariableCountChange}
          />
        ) : (
          <GlobalVariablesPanel />
        )}
      </div>
    </section>
  );
}

// ── 面板内容渲染 ──

interface DockPanelContentProps {
  panel: RuntimeDockPanel;
  logViewportRef: React.RefObject<HTMLDivElement | null> | undefined;
  runtimeConsoleEntries: ReturnType<typeof buildRuntimeConsoleEntries>;
  eventFeedText: string;
  hasCopiedEventFeed: boolean;
  onCopyEventFeed: () => void;
  results: RuntimeDockProps['results'];
  connectionPreview: RuntimeDockProps['connectionPreview'];
  themeMode: RuntimeDockProps['themeMode'];
  activeWorkflowId: RuntimeDockProps['activeWorkflowId'];
  onVariableCountChange: (count: number) => void;
  isCollapsed: boolean;
  payloadText: string;
  deployInfo: RuntimeDockProps['deployInfo'];
  onPayloadTextChange: RuntimeDockProps['onPayloadTextChange'];
}

export function DockPanelContent({
  panel,
  logViewportRef,
  runtimeConsoleEntries,
  eventFeedText,
  hasCopiedEventFeed,
  onCopyEventFeed,
  results,
  connectionPreview,
  themeMode,
  activeWorkflowId,
  onVariableCountChange,
  isCollapsed,
  payloadText,
  deployInfo,
  onPayloadTextChange,
}: DockPanelContentProps) {
  if (panel === 'events') {
    return (
      <section className="runtime-dock__panel is-active" role="tabpanel">
        <div className="runtime-dock__panel-header">
          <h3>执行事件流</h3>
            <button
              type="button"
              className={`runtime-dock__panel-tool ${hasCopiedEventFeed ? 'is-active' : ''}`}
              data-testid="runtime-dock-copy-events"
              onClick={onCopyEventFeed}
              disabled={!eventFeedText.trim()}
              aria-label={hasCopiedEventFeed ? '执行事件流已复制' : '复制执行事件流'}
              title={hasCopiedEventFeed ? '已复制' : '复制执行事件流'}
            >
              <CopyIcon width={14} height={14} />
            </button>
        </div>
        <div className="runtime-dock__panel-body">
          <div
            ref={(el) => { if (logViewportRef) { (logViewportRef as React.MutableRefObject<HTMLDivElement | null>).current = el; } }}
            className="runtime-log"
            role="log"
            aria-live="polite"
            data-testid="event-feed"
          >
            {runtimeConsoleEntries.length === 0 ? (
              <p className="runtime-log__empty">暂无事件与异常</p>
            ) : (
              runtimeConsoleEntries.map((entry) => (
                <div key={entry.id} className={`runtime-log__line is-${entry.level}`}>
                  <span className="runtime-log__time">
                    {formatLogTimestamp(entry.timestamp)}
                  </span>
                  <span className="runtime-log__source">
                    {entry.source}
                    {entry.tag ? (
                      <em className="runtime-log__badge">{entry.tag}</em>
                    ) : null}
                  </span>
                  <span className="runtime-log__message">
                    {entry.message}
                    {entry.detail ? (
                      <span className="runtime-log__detail">{entry.detail}</span>
                    ) : null}
                  </span>
                  {entry.level === 'error' ? (
                    <span
                      role="button"
                      tabIndex={0}
                      className="runtime-log__ai-icon"
                      title="发送给 AI 分析"
                      onClick={() => {
                        const parts = [`运行时错误：${entry.message}`];
                        if (entry.detail) parts.push(`详情：${entry.detail}`);
                        if (entry.source) parts.push(`来源：${entry.source}`);
                        if (entry.nodeId) parts.push(`节点：${entry.nodeId}`);
                        if (entry.traceId) parts.push(`Trace：${entry.traceId}`);
                        sendToCopilot(parts.join('\n'));
                      }}
                    ><SparklesIcon width={13} height={13} /></span>
                  ) : null}
                </div>
              ))
            )}
          </div>
        </div>
      </section>
    );
  }

  if (panel === 'results') {
    return (
      <section className="runtime-dock__panel is-active" role="tabpanel">
        <div className="runtime-dock__panel-header">
          <h3>结果载荷</h3>
        </div>
        <div className="runtime-dock__panel-body">
          <div className="runtime-results" data-testid="result-list">
            {results.length === 0 ? (
              <p className="runtime-results__empty">暂无输出</p>
            ) : (
              <div className="runtime-results__stream" role="list">
                {results.map((result) => {
                  const resultKey = `${result.trace_id}-${result.timestamp}`;
                  const topLevelCount =
                    result.payload && typeof result.payload === 'object'
                      ? Object.keys(result.payload).length
                      : 1;

                  return (
                    <article
                      key={resultKey}
                      className="runtime-results__entry"
                      role="listitem"
                    >
                      <div className="runtime-results__entry-meta">
                        <strong>
                          {formatLogTimestamp(new Date(result.timestamp).getTime())}
                        </strong>
                        <span>{result.trace_id}</span>
                        <em>{`${topLevelCount} 个条目`}</em>
                      </div>
                      <div className="runtime-json-view">
                        <JsonView
                          data={normalizeResultPayload(result.payload)}
                          shouldExpandNode={collapseAllNested}
                          clickToExpandNode
                          style={themeMode === 'dark' ? darkStyles : defaultStyles}
                        />
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </section>
    );
  }

  if (panel === 'payload') {
    return <PayloadEditorPanel payloadText={payloadText} deployInfo={deployInfo} onPayloadTextChange={onPayloadTextChange} themeMode={themeMode} />;
  }

  if (panel === 'connections') {
    return <ConnectionTable connections={connectionPreview} />;
  }

  // variables
  return (
    <VariablesPanelWithSubTabs
      activeWorkflowId={activeWorkflowId}
      onVariableCountChange={onVariableCountChange}
    />
  );
}

// ── 载荷编辑面板 ──

function PayloadEditorPanel({
  payloadText,
  deployInfo,
  onPayloadTextChange,
  themeMode,
}: {
  payloadText: string;
  deployInfo: RuntimeDockProps['deployInfo'];
  onPayloadTextChange: RuntimeDockProps['onPayloadTextChange'];
  themeMode: RuntimeDockProps['themeMode'];
}) {
  return (
    <section className="runtime-dock__panel is-active" role="tabpanel">
      <div className="runtime-dock__panel-header">
        <h3>测试载荷</h3>
        <span className="runtime-dock__payload-badge">{deployInfo ? '已可发送' : '等待部署'}</span>
      </div>
      <div className="runtime-dock__panel-body runtime-dock__panel-body--payload">
        <div className="runtime-dock__payload-editor">
          <JsonCodeEditor value={payloadText} onChange={onPayloadTextChange} theme={themeMode} />
        </div>
      </div>
    </section>
  );
}
