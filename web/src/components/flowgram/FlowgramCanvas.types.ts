import type { EdgeHeatMap } from '../../hooks/use-edge-heatmap';
import type {
  AiGenerationParams,
  AiProviderView,
  ConnectionDefinition,
  WorkflowGraph,
  WorkflowRuntimeState,
  WorkflowWindowStatus,
} from '../../types';
import type { ResolvedThemeMode } from '../app/types';

export interface FlowgramCanvasResources {
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
}

export interface FlowgramCanvasRuntime {
  runtimeState: WorkflowRuntimeState;
  workflowStatus: WorkflowWindowStatus;
  canTestRun?: boolean;
  /** ADR-0016：获取边热力图快照。 */
  getEdgeHeatmap?: () => EdgeHeatMap;
  /** ADR-0016：注册热力图数据变更回调。 */
  registerEdgeHeatUpdate?: (callback: (() => void) | null) => void;
}

export interface FlowgramCanvasAppearance {
  accentHex: string;
  themeMode: ResolvedThemeMode;
  nodeCodeColor: string;
}

export interface FlowgramCanvasExportTarget {
  workspacePath?: string;
  workflowName?: string | null;
}

export interface FlowgramCanvasActions {
  onRunRequested?: () => void;
  onStopRequested?: () => void;
  onTestRunRequested?: () => void;
  onGraphChange: (nextAstText: string) => void;
  onError?: (title: string, detail?: string | null) => void;
  onStatusMessage?: (message: string) => void;
}

export interface FlowgramCanvasProps {
  graph: WorkflowGraph | null;
  resources: FlowgramCanvasResources;
  runtime: FlowgramCanvasRuntime;
  appearance: FlowgramCanvasAppearance;
  exportTarget?: FlowgramCanvasExportTarget;
  actions: FlowgramCanvasActions;
}

export interface CanvasNodeOp {
  id: string;
  type: string;
  label?: string;
  config?: Record<string, unknown>;
  connection_id?: string;
}

export interface CanvasEdgeOp {
  from: string;
  to: string;
  source_port_id?: string;
  target_port_id?: string;
}

export interface CanvasOps {
  nodes: CanvasNodeOp[];
  edges: CanvasEdgeOp[];
}

export interface CanvasNodePatch {
  label?: string;
  config?: Record<string, unknown>;
  connectionId?: string;
}

export interface FlowgramCanvasHandle {
  isReady: () => boolean;
  getCurrentWorkflowGraph: () => WorkflowGraph | null;
  loadWorkflowGraph: (graph: WorkflowGraph) => void;
  addCanvasOps: (ops: CanvasOps) => void;
  autoLayout: () => void;
  /** 当前选中节点的摘要信息，供 copilot 等外部消费者使用。 */
  getSelectedNode: () => { id: string; type: string; label?: string } | null;
  /** 修改已有节点的配置（浅合并）。 */
  updateCanvasNode: (nodeId: string, patch: CanvasNodePatch) => boolean;
  /** 删除节点及其所有连线。 */
  deleteCanvasNode: (nodeId: string) => boolean;
  /** 删除两节点间的第一条匹配连线。 */
  deleteCanvasEdge: (from: string, to: string) => boolean;
}
