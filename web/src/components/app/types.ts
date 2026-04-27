import type {
  AppErrorRecord,
  DeadLetterRecord,
  ObservabilityQueryResult,
  ConnectionRecord,
  DeployResponse,
  RuntimeWorkflowSummary,
  RuntimeLogEntry,
  WorkflowResult,
} from '../../types';
import type { AccentPreset, AccentPresetOption } from '../../lib/theme';

export type ThemeMode = 'light' | 'dark';
export type MotionMode = 'full' | 'reduced';
export type StartupPage = 'dashboard' | 'boards';
export type SidebarSection =
  | 'dashboard'
  | 'boards'
  | 'runtime'
  | 'connections'
  | 'plugins'
  | 'payload'
  | 'logs'
  | 'ai'
  | 'settings'
  | 'about';
export type SidebarGroup = 'top' | 'main' | 'settings';

export interface SidebarSectionConfig {
  key: SidebarSection;
  group: SidebarGroup;
  label: string;
  badge: string;
}

export interface SidebarNavProps {
  activeSection: SidebarSection;
  sections: SidebarSectionConfig[];
  onSectionChange: (section: SidebarSection) => void;
  userName: string;
  userRole: string;
  onUserSwitch: () => void;
  workflowStatusLabel: string;
  workflowStatusPillClass: string;
  themeMode: ThemeMode;
  onToggleTheme: () => void;
  isCollapsed: boolean;
  onToggleCollapsed: () => void;
}

export interface PayloadPanelProps {
  payloadText: string;
  deployInfo: DeployResponse | null;
  onPayloadTextChange: (value: string) => void;
}

export interface LogsPanelProps {
  eventFeed: RuntimeLogEntry[];
  appErrors: AppErrorRecord[];
  resultCount: number;
  themeMode: ThemeMode;
  activeBoardName: string | null;
  workflowStatusLabel: string;
  workspacePath: string;
  activeTraceId: string | null;
  observability?: ObservabilityQueryResult | null;
}

export interface RuntimeDockProps {
  eventFeed: RuntimeLogEntry[];
  appErrors: AppErrorRecord[];
  results: WorkflowResult[];
  connectionPreview: ConnectionRecord[];
  themeMode: ThemeMode;
  isCollapsed: boolean;
  onToggleCollapsed: () => void;
  /** 当前活跃部署的 workflow_id，用于变量面板。null 时面板显示占位。 */
  activeWorkflowId: string | null;
}

export interface SettingsPanelProps {
  isTauriRuntime: boolean;
  runtimeModeLabel: string;
  workflowStatusLabel: string;
  statusMessage: string;
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
  accentPreset: AccentPreset;
  accentOptions: AccentPresetOption[];
  customAccentHex: string;
  onAccentPresetChange: (preset: AccentPreset) => void;
  onCustomAccentChange: (hex: string) => void;
  motionMode: MotionMode;
  onMotionModeChange: (mode: MotionMode) => void;
  startupPage: StartupPage;
  onStartupPageChange: (page: StartupPage) => void;
  projectWorkspacePath: string;
  projectWorkspaceResolvedPath: string | null;
  projectWorkspaceBoardsDirectoryPath: string | null;
  projectWorkspaceUsingDefault: boolean;
  projectWorkspaceIsSyncing: boolean;
  projectWorkspaceError: string | null;
  onProjectWorkspacePathChange: (path: string) => void;
}

export interface AiConfigPanelProps {
  isTauriRuntime: boolean;
  aiConfig: import('../../types').AiConfigView | null;
  aiConfigLoading: boolean;
  aiConfigError: string | null;
  onAiConfigSave: (update: import('../../types').AiConfigUpdate) => Promise<void>;
  onAiProviderTest: (draft: import('../../types').AiProviderDraft) => Promise<void>;
  aiTestResult: import('../../types').AiTestResult | null;
  aiTesting: boolean;
}

export interface RuntimeManagerPanelProps {
  workspacePath: string;
  themeMode: ThemeMode;
  activeBoardId: string | null;
  onOpenBoard: (boardId: string) => void;
  onPersistActiveProject?: (projectId: string | null) => Promise<void> | void;
  onBeforeWorkflowStop?: () => void;
  onAfterWorkflowStop?: () => void;
  onRemovePersistedDeployment?: (projectId: string) => Promise<void>;
  onStatusMessage: (message: string) => void;
  onRuntimeCountChange?: (count: number) => void;
  initialWorkflows?: RuntimeWorkflowSummary[];
  initialDeadLetters?: DeadLetterRecord[];
}
