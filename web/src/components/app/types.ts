import type {
  AppErrorRecord,
  ConnectionRecord,
  DeployResponse,
  RuntimeLogEntry,
  WorkflowRuntimeState,
  WorkflowResult,
} from '../../types';
import type { AccentPreset, AccentPresetOption } from '../../lib/theme';

export type ThemeMode = 'light' | 'dark';
export type UiDensity = 'comfortable' | 'compact';
export type MotionMode = 'full' | 'reduced';
export type StartupPage = 'dashboard' | 'boards';
export type SidebarSection =
  | 'dashboard'
  | 'boards'
  | 'source'
  | 'connections'
  | 'payload'
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

export interface RuntimeDockProps {
  deployInfo: DeployResponse | null;
  runtimeState: WorkflowRuntimeState;
  eventFeed: RuntimeLogEntry[];
  appErrors: AppErrorRecord[];
  results: WorkflowResult[];
  connectionPreview: ConnectionRecord[];
  themeMode: ThemeMode;
  isCollapsed: boolean;
  onToggleCollapsed: () => void;
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
  densityMode: UiDensity;
  onDensityModeChange: (mode: UiDensity) => void;
  motionMode: MotionMode;
  onMotionModeChange: (mode: MotionMode) => void;
  startupPage: StartupPage;
  onStartupPageChange: (page: StartupPage) => void;
}

export interface AboutPanelProps {
  isTauriRuntime: boolean;
  runtimeModeLabel: string;
  graphNodeCount: number;
  graphConnectionCount: number;
  deployInfo: DeployResponse | null;
}
