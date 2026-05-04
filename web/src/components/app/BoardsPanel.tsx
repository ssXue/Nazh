import type { KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useEffect, useRef, useState } from 'react';

import { CanvasIcon, DeleteActionIcon, PlusIcon, UploadIcon } from './AppIcons';

export interface BoardItem {
  id: string;
  name: string;
  description: string;
  nodeCount: number;
  updatedAt: string;
  snapshotCount: number;
  environmentCount: number;
  environmentName: string;
  migrationNote?: string | null;
}

interface BoardsPanelProps {
  boards: BoardItem[];
  onOpenBoard: (board: BoardItem) => void;
  onCreateBoard: () => void;
  onImportBoardFile: (file: File) => void | Promise<void>;
  onDeleteBoard: (board: BoardItem) => void;
}

export function BoardsPanel({
  boards,
  onOpenBoard,
  onCreateBoard,
  onImportBoardFile,
  onDeleteBoard,
}: BoardsPanelProps) {
  const importInputRef = useRef<HTMLInputElement | null>(null);
  const [pendingDeleteBoard, setPendingDeleteBoard] = useState<BoardItem | null>(null);

  useEffect(() => {
    if (!pendingDeleteBoard) {
      return;
    }

    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        setPendingDeleteBoard(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pendingDeleteBoard]);

  function handleBoardKeyDown(event: ReactKeyboardEvent<HTMLElement>, board: BoardItem) {
    if (event.key !== 'Enter' && event.key !== ' ') {
      return;
    }

    event.preventDefault();
    onOpenBoard(board);
  }

  return (
    <div className="boards-panel">
      <div className="boards-panel__header window-safe-header" data-window-drag-region>
        <h2>所有看板</h2>
      </div>

      <div className="boards-panel__toolbar">
        <div className="boards-panel__summary">
          <strong>{boards.length} 个工程</strong>
          <span>项目、版本与环境差异都将自动持久化到本地。</span>
        </div>

        <div className="boards-panel__actions" data-no-window-drag>
          <button
            type="button"
            className="boards-panel__action"
            data-testid="board-import"
            onClick={() => importInputRef.current?.click()}
          >
            <UploadIcon />
            <span>导入工程</span>
          </button>
          <input
            ref={importInputRef}
            type="file"
            accept=".json,application/json"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (!file) {
                return;
              }

              void onImportBoardFile(file);
              event.target.value = '';
            }}
          />
        </div>
      </div>

      <div className="boards-panel__grid">
        <button
          type="button"
          className="board-card board-card--create"
          data-testid="board-create"
          onClick={onCreateBoard}
        >
          <div className="board-card__icon board-card__icon--create">
            <PlusIcon />
          </div>

          <div className="board-card__body">
            <strong className="board-card__name">新建工程</strong>
            <span className="board-card__desc">
              从空白工作流开始，立即生成可保存、可快照、可回滚的新工程。
            </span>
          </div>

          <div className="board-card__chips">
            <span className="board-card__chip board-card__chip--create">空白模板</span>
            <span className="board-card__chip board-card__chip--create">本地持久化</span>
          </div>

          <div className="board-card__footer">
            <span className="board-card__meta">
              {boards.length === 0 ? '当前还没有工程' : '从这里开始新的工程'}
            </span>
          </div>
        </button>

        {boards.length === 0 ? (
          <div className="boards-panel__empty" data-testid="board-empty-state">
            <strong>当前没有工程</strong>
            <span>可以先创建一个工程，或从右上角导入已有项目包。</span>
          </div>
        ) : (
          boards.map((board) => (
            <article
              key={board.id}
              className="board-card board-card--entry"
              data-testid="board-entry"
              role="button"
              tabIndex={0}
              aria-label={`进入工程 ${board.name}`}
              onClick={() => onOpenBoard(board)}
              onKeyDown={(event) => handleBoardKeyDown(event, board)}
            >
              <div className="board-card__icon">
                <CanvasIcon />
              </div>

              <div className="board-card__body">
                <strong className="board-card__name">{board.name}</strong>
                <span className="board-card__desc">{board.description}</span>
              </div>

              <div className="board-card__chips">
                <span className="board-card__chip">{`${board.nodeCount} 节点`}</span>
                <span className="board-card__chip">{`${board.snapshotCount} 版本`}</span>
                <span className="board-card__chip">{`${board.environmentCount} 环境`}</span>
                <span className="board-card__chip">{board.environmentName}</span>
              </div>

              {board.migrationNote ? (
                <div className="board-card__migration">{board.migrationNote}</div>
              ) : null}

              <div className="board-card__footer">
                <span className="board-card__meta">{board.updatedAt}</span>
                <button
                  type="button"
                  className="board-card__delete"
                  aria-label={`删除工程 ${board.name}`}
                  title={`删除工程 ${board.name}`}
                  data-testid="board-delete"
                  data-no-window-drag
                  onClick={(event) => {
                    event.stopPropagation();
                    setPendingDeleteBoard(board);
                  }}
                >
                  <DeleteActionIcon />
                </button>
              </div>
            </article>
          ))
        )}
      </div>

      {pendingDeleteBoard ? (
        <div
          className="boards-panel__confirm-layer"
          data-no-window-drag
          onClick={() => setPendingDeleteBoard(null)}
        >
          <div
            className="boards-panel__confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="boards-delete-title"
            aria-describedby="boards-delete-description"
            onClick={(event) => event.stopPropagation()}
          >
            <span className="boards-panel__confirm-kicker">删除工程</span>
            <strong id="boards-delete-title">确定删除“{pendingDeleteBoard.name}”？</strong>
            <p id="boards-delete-description">
              这会移除本地保存的工作流、版本快照和环境配置，且无法撤销。
            </p>

            <div className="boards-panel__confirm-actions">
              <button
                type="button"
                className="boards-panel__confirm-action"
                data-testid="board-delete-cancel"
                onClick={() => setPendingDeleteBoard(null)}
              >
                取消
              </button>
              <button
                type="button"
                className="boards-panel__confirm-action boards-panel__confirm-action--danger"
                data-testid="board-delete-confirm"
                onClick={() => {
                  onDeleteBoard(pendingDeleteBoard);
                  setPendingDeleteBoard(null);
                }}
              >
                删除
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
