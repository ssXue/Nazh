import { useEffect, useMemo, useRef, useState } from 'react';

import {
  BackIcon,
  ChevronDownIcon,
  DeleteActionIcon,
  EnvironmentIcon,
  PlusIcon,
  RightSidebarIcon,
  SaveIcon,
  SparklesIcon,
  SnapshotIcon,
} from './AppIcons';
import {
  formatRelativeTimestamp,
  getActiveEnvironment,
  type ProjectEnvironmentDiff,
  type ProjectRecord,
} from '../../lib/projects';

interface ProjectWorkspaceHeaderProps {
  project: ProjectRecord;
  nodeCount: number;
  onBack: () => void;
  onCreateSnapshot: () => void;
  onDeleteSnapshot: (snapshotId: string) => void;
  onRollbackSnapshot: (snapshotId: string) => void;
  onEnvironmentChange: (environmentId: string) => void;
  onEnvironmentSave: (
    environmentId: string,
    patch: { name: string; description: string; diff: ProjectEnvironmentDiff },
  ) => void;
  onDuplicateEnvironment: (environmentId: string) => void;
  onDeleteEnvironment: (environmentId: string) => void;
  onOpenAiComposer: () => void;
  isRuntimeDockCollapsed: boolean;
  onToggleRuntimeDockCollapsed: () => void;
  aiActionTitle: string;
  aiActionDisabled?: boolean;
  aiActionLoading?: boolean;
}

function getSnapshotReasonLabel(reason: ProjectRecord['snapshots'][number]['reason']): string {
  switch (reason) {
    case 'seed':
      return '模板';
    case 'manual':
      return '快照';
    case 'import':
      return '导入';
    case 'migration':
      return '迁移';
    case 'rollback':
      return '保护';
  }
}

function formatEnvironmentDiffText(diff: ProjectEnvironmentDiff): string {
  return JSON.stringify(diff ?? {}, null, 2);
}

export function ProjectWorkspaceHeader({
  project,
  nodeCount,
  onBack,
  onCreateSnapshot,
  onDeleteSnapshot,
  onRollbackSnapshot,
  onEnvironmentChange,
  onEnvironmentSave,
  onDuplicateEnvironment,
  onDeleteEnvironment,
  onOpenAiComposer,
  isRuntimeDockCollapsed,
  onToggleRuntimeDockCollapsed,
  aiActionTitle,
  aiActionDisabled = false,
  aiActionLoading = false,
}: ProjectWorkspaceHeaderProps) {
  const historyMenuRef = useRef<HTMLDetailsElement | null>(null);
  const environmentMenuRef = useRef<HTMLDetailsElement | null>(null);
  const activeEnvironment = getActiveEnvironment(project);
  const [environmentName, setEnvironmentName] = useState(activeEnvironment?.name ?? '');
  const [environmentDescription, setEnvironmentDescription] = useState(
    activeEnvironment?.description ?? '',
  );
  const [environmentDiffText, setEnvironmentDiffText] = useState(
    formatEnvironmentDiffText(activeEnvironment?.diff ?? {}),
  );
  const [environmentDiffError, setEnvironmentDiffError] = useState<string | null>(null);

  useEffect(() => {
    setEnvironmentName(activeEnvironment?.name ?? '');
    setEnvironmentDescription(activeEnvironment?.description ?? '');
    setEnvironmentDiffText(formatEnvironmentDiffText(activeEnvironment?.diff ?? {}));
    setEnvironmentDiffError(null);
  }, [
    activeEnvironment?.description,
    activeEnvironment?.id,
    activeEnvironment?.name,
    activeEnvironment?.updatedAt,
  ]);

  const migrationSummary = useMemo(
    () => project.migrationNotes[0] ?? null,
    [project.migrationNotes],
  );

  const handleSaveEnvironment = () => {
    if (!activeEnvironment) {
      return;
    }

    try {
      const parsedDiff = JSON.parse(environmentDiffText) as ProjectEnvironmentDiff;
      onEnvironmentSave(activeEnvironment.id, {
        name: environmentName.trim() || activeEnvironment.name,
        description: environmentDescription.trim(),
        diff: parsedDiff,
      });
      setEnvironmentDiffError(null);
      if (environmentMenuRef.current) {
        environmentMenuRef.current.open = false;
      }
    } catch (error) {
      setEnvironmentDiffError(
        error instanceof Error ? error.message : '环境差异配置 JSON 无法解析。',
      );
    }
  };

  return (
    <div
      className="studio-board-workspace__header window-safe-header"
      data-window-drag-region
    >
      <div className="studio-board-workspace__header-main" data-no-window-drag>
        <div className="studio-board-workspace__title-pill">
          <button
            type="button"
            className="studio-board-workspace__back"
            onClick={onBack}
            aria-label="返回所有看板"
            title="返回所有看板"
          >
            <BackIcon />
          </button>
          <div className="studio-board-workspace__header-heading">
            <h2>{project.name}</h2>
            <div className="studio-board-workspace__header-meta">
              <span className="studio-board-workspace__meta-pill">
                {`${formatRelativeTimestamp(project.updatedAt)} · ${nodeCount} 节点 · ${project.snapshots.length} 个版本`}
              </span>
            </div>
          </div>
          {migrationSummary ? (
            <span className="studio-board-workspace__migration">{migrationSummary}</span>
          ) : null}
        </div>
      </div>

      <div className="studio-board-workspace__controls" data-no-window-drag>
        <button
          type="button"
          className="studio-board-workspace__action studio-board-workspace__action--accent"
          onClick={onOpenAiComposer}
          disabled={aiActionDisabled}
          title={aiActionTitle}
        >
          <SparklesIcon />
          <span>{aiActionLoading ? 'AI 编辑中...' : 'AI 编辑'}</span>
        </button>

        <div className="studio-board-workspace__control-group">
          <label className="studio-board-workspace__environment-select">
            <EnvironmentIcon />
            <select
              value={activeEnvironment?.id ?? ''}
              aria-label="当前环境"
              onChange={(event) => onEnvironmentChange(event.target.value)}
            >
              {project.environments.map((environment) => (
                <option key={environment.id} value={environment.id}>
                  {environment.name}
                </option>
              ))}
            </select>
          </label>

          <details
            ref={environmentMenuRef}
            className="studio-board-workspace__menu studio-board-workspace__menu--segment"
            data-no-window-drag
          >
            <summary
              className="studio-board-workspace__action studio-board-workspace__action--menu-segment"
              aria-label="环境差异配置"
              title="环境差异配置"
            >
              <ChevronDownIcon />
            </summary>
            <div className="studio-board-workspace__menu-panel studio-board-workspace__menu-panel--environment">
              <div className="studio-board-workspace__menu-header">
                <strong>环境差异配置</strong>
                <span>{activeEnvironment?.name ?? '未选择环境'}</span>
              </div>

              <div className="studio-board-workspace__environment-editor">
                <label className="studio-board-workspace__field">
                  <span>环境名称</span>
                  <input
                    type="text"
                    value={environmentName}
                    onChange={(event) => setEnvironmentName(event.target.value)}
                  />
                </label>

                <label className="studio-board-workspace__field">
                  <span>说明</span>
                  <input
                    type="text"
                    value={environmentDescription}
                    onChange={(event) => setEnvironmentDescription(event.target.value)}
                  />
                </label>

                <label className="studio-board-workspace__field studio-board-workspace__field--stacked">
                  <span>差异 JSON</span>
                  <textarea
                    value={environmentDiffText}
                    spellCheck={false}
                    onChange={(event) => setEnvironmentDiffText(event.target.value)}
                  />
                </label>

                {environmentDiffError ? (
                  <p className="studio-board-workspace__field-error">{environmentDiffError}</p>
                ) : null}

                <div className="studio-board-workspace__environment-actions">
                  <button
                    type="button"
                    className="studio-board-workspace__action"
                    onClick={handleSaveEnvironment}
                  >
                    <SaveIcon />
                    <span>应用</span>
                  </button>

                  {activeEnvironment ? (
                    <button
                      type="button"
                      className="studio-board-workspace__action"
                      onClick={() => onDuplicateEnvironment(activeEnvironment.id)}
                    >
                      <PlusIcon />
                      <span>派生</span>
                    </button>
                  ) : null}

                  {activeEnvironment ? (
                    <button
                      type="button"
                      className="studio-board-workspace__action is-danger"
                      disabled={project.environments.length <= 1}
                      onClick={() => onDeleteEnvironment(activeEnvironment.id)}
                    >
                      <span>删除</span>
                    </button>
                  ) : null}
                </div>
              </div>
            </div>
          </details>
        </div>

        <div className="studio-board-workspace__control-group">
          <button
            type="button"
            className="studio-board-workspace__action studio-board-workspace__action--group-primary"
            onClick={onCreateSnapshot}
          >
            <SnapshotIcon />
            <span>快照</span>
          </button>

          <details
            ref={historyMenuRef}
            className="studio-board-workspace__menu studio-board-workspace__menu--segment"
            data-no-window-drag
          >
            <summary
              className="studio-board-workspace__action studio-board-workspace__action--menu-segment"
              aria-label="查看版本快照"
              title="查看版本快照"
            >
              <ChevronDownIcon />
            </summary>
            <div className="studio-board-workspace__menu-panel studio-board-workspace__menu-panel--history">
              <div className="studio-board-workspace__menu-header">
                <strong>版本快照</strong>
                <span>{project.snapshots.length} 个可回滚版本</span>
              </div>
              <div className="studio-board-workspace__snapshot-list">
                {project.snapshots.length === 0 ? (
                  <article className="studio-board-workspace__snapshot-card studio-board-workspace__snapshot-card--empty">
                    <div className="studio-board-workspace__snapshot-copy">
                      <strong>暂无可用快照</strong>
                      <span>点击顶部“快照”按钮后，会在这里保留当前工程版本。</span>
                    </div>
                  </article>
                ) : (
                  project.snapshots.map((snapshot) => (
                    <article key={snapshot.id} className="studio-board-workspace__snapshot-card">
                      <div className="studio-board-workspace__snapshot-copy">
                        <strong>{snapshot.label}</strong>
                        <span>{snapshot.description}</span>
                      </div>
                      <div className="studio-board-workspace__snapshot-meta">
                        <em>{getSnapshotReasonLabel(snapshot.reason)}</em>
                        <span>{formatRelativeTimestamp(snapshot.createdAt)}</span>
                      </div>
                      <div className="studio-board-workspace__snapshot-actions">
                        <button
                          type="button"
                          className="studio-board-workspace__snapshot-action is-danger"
                          aria-label={`删除快照 ${snapshot.label}`}
                          title={`删除快照 ${snapshot.label}`}
                          onClick={() => onDeleteSnapshot(snapshot.id)}
                        >
                          <DeleteActionIcon />
                          <span>删除</span>
                        </button>
                        <button
                          type="button"
                          className="studio-board-workspace__snapshot-action"
                          onClick={() => {
                            onRollbackSnapshot(snapshot.id);
                            if (historyMenuRef.current) {
                              historyMenuRef.current.open = false;
                            }
                          }}
                        >
                          回滚
                        </button>
                      </div>
                    </article>
                  ))
                )}
              </div>
            </div>
          </details>
        </div>

        <button
          type="button"
          className="studio-board-workspace__action studio-board-workspace__action--icon studio-board-workspace__action--dock-toggle"
          aria-expanded={!isRuntimeDockCollapsed}
          aria-controls="runtime-dock-grid"
          aria-label={isRuntimeDockCollapsed ? '展开右侧窗体' : '收起右侧窗体'}
          title={isRuntimeDockCollapsed ? '展开右侧窗体' : '收起右侧窗体'}
          onClick={onToggleRuntimeDockCollapsed}
        >
          <RightSidebarIcon />
        </button>


      </div>
    </div>
  );
}
