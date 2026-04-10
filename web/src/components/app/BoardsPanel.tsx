import { CanvasIcon } from './AppIcons';

export interface BoardItem {
  id: string;
  name: string;
  description: string;
  nodeCount: number;
  updatedAt: string;
}

interface BoardsPanelProps {
  onOpenBoard: (board: BoardItem) => void;
}

export const BOARD_LIBRARY: BoardItem[] = [
  {
    id: 'default',
    name: '工业告警联动',
    description: 'Timer + Modbus + Code + Switch + HTTP / SQLite / Debug 的完整示例工程',
    nodeCount: 7,
    updatedAt: '刚刚',
  },
  {
    id: 'data-pipeline',
    name: '数据管道',
    description: '从数据源到输出的 ETL 数据清洗管道',
    nodeCount: 3,
    updatedAt: '2 小时前',
  },
  {
    id: 'notification',
    name: '告警通知流',
    description: '监控事件触发的多通道告警通知流程',
    nodeCount: 3,
    updatedAt: '昨天',
  },
];

export function BoardsPanel({ onOpenBoard }: BoardsPanelProps) {
  return (
    <div className="boards-panel">
      <div className="boards-panel__header window-safe-header" data-window-drag-region>
        <h2>所有看板</h2>
      </div>

      <div className="boards-panel__grid">
        {BOARD_LIBRARY.map((board) => (
          <button
            key={board.id}
            type="button"
            className="board-card"
            data-testid="board-entry"
            onClick={() => onOpenBoard(board)}
          >
            <div className="board-card__icon">
              <CanvasIcon />
            </div>
            <div className="board-card__body">
              <strong className="board-card__name">{board.name}</strong>
              <span className="board-card__desc">{board.description}</span>
            </div>
            <div className="board-card__footer">
              <span className="board-card__meta">{`${board.nodeCount} 节点 · ${board.updatedAt}`}</span>
              <span className="board-card__badge">进入 →</span>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
