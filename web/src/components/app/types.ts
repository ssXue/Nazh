import type {
  ConnectionRecord,
  DeployResponse,
  WorkflowRuntimeState,
  WorkflowResult,
} from '../../types';

export type ThemeMode = 'light' | 'dark';
export type SidebarSection =
  | 'canvas'
  | 'overview'
  | 'source'
  | 'connections'
  | 'payload'
  | 'settings'
  | 'about';
export type SidebarGroup = 'main' | 'settings';

export interface SidebarSectionConfig {
  key: SidebarSection;
  group: SidebarGroup;
  label: string;
  badge: string;
}

export interface StudioTitleBarProps {
  isTauriRuntime: boolean;
  runtimeModeLabel: string;
  workflowStatusLabel: string;
  workflowStatusPillClass: string;
  themeMode: ThemeMode;
  onToggleTheme: () => void;
}

export interface SidebarNavProps {
  activeSection: SidebarSection;
  sections: SidebarSectionConfig[];
  onSectionChange: (section: SidebarSection) => void;
  userName: string;
  userRole: string;
  onUserSwitch: () => void;
}

export interface OverviewPanelProps {
  graphNodeCount: number;
  graphEdgeCount: number;
  graphConnectionCount: number;
  activeNodeCount: number;
  workflowStatusLabel: string;
  workflowStatusPillClass: string;
  statusMessage: string;
  runtimeSnapshot: string;
  runtimeUpdatedLabel: string;
  deployInfo: DeployResponse | null;
}

export interface SourcePanelProps {
  astText: string;
  graphError: string | null;
  onAstTextChange: (value: string) => void;
}

export interface PayloadPanelProps {
  payloadText: string;
  deployInfo: DeployResponse | null;
  onPayloadTextChange: (value: string) => void;
}

export interface StudioControlBarProps {
  workflowStatusLabel: string;
  workflowStatusPillClass: string;
  isTauriRuntime: boolean;
  runtimeModeLabel: string;
  runtimeSnapshot: string;
  runtimeUpdatedLabel: string;
  statusMessage: string;
  graphNodeCount: number;
  graphEdgeCount: number;
  graphConnectionCount: number;
  activeNodeCount: number;
  canDispatchPayload: boolean;
  onDeploy: () => void;
  onDispatchPayload: () => void;
  onRefreshConnections: () => void;
}

export interface RuntimeDockProps {
  deployInfo: DeployResponse | null;
  runtimeState: WorkflowRuntimeState;
  eventFeed: string[];
  results: WorkflowResult[];
  connectionPreview: ConnectionRecord[];
}

export interface SettingsPanelProps {
  isTauriRuntime: boolean;
  runtimeModeLabel: string;
  workflowStatusLabel: string;
  statusMessage: string;
  themeMode: ThemeMode;
}

export interface AboutPanelProps {
  isTauriRuntime: boolean;
  runtimeModeLabel: string;
  graphNodeCount: number;
  graphConnectionCount: number;
  deployInfo: DeployResponse | null;
}
