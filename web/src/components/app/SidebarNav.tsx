import {
  AboutIcon,
  AiIcon,
  BoardsIcon,
  ConnectionsIcon,
  DashboardIcon,
  DeviceIcon,
  FileYamlIcon,
  HistoryIcon,
  LogsIcon,
  MoonIcon,
  SettingsIcon,
  SunIcon,
  SwitchUserIcon,
} from './AppIcons';
import { useScrollEdgeEffect } from '../../hooks/use-scroll-edge-effect';
import type { SidebarNavProps, SidebarSection } from './types';

const SECTION_ICONS: Record<SidebarSection, typeof DashboardIcon> = {
  dashboard: DashboardIcon,
  boards: BoardsIcon,
  runtime: HistoryIcon,
  connections: ConnectionsIcon,
  devices: DeviceIcon,
  'dsl-editor': FileYamlIcon,
  'ai-orchestration': AiIcon,
  plugins: ConnectionsIcon,
  logs: LogsIcon,
  ai: AiIcon,
  settings: SettingsIcon,
  about: AboutIcon,
};

function getUserInitials(userName: string): string {
  const normalized = userName.trim();
  if (!normalized) {
    return 'NZ';
  }

  return normalized.replace(/\s+/g, '').slice(0, 2).toUpperCase();
}

export function SidebarNav({
  activeSection,
  sections,
  onSectionChange,
  userName,
  userRole,
  onUserSwitch,
  workflowStatusLabel,
  workflowStatusPillClass,
  themeMode,
  onToggleTheme,
  isCollapsed: _isCollapsed,
  onToggleCollapsed: _onToggleCollapsed,
}: SidebarNavProps) {
  const isDarkMode = themeMode === 'dark';
  const groupsRef = useScrollEdgeEffect<HTMLDivElement>();
  const groupedSections = [
    {
      key: 'top',
      label: '',
      sections: sections.filter((section) => section.group === 'top'),
    },
    {
      key: 'main',
      label: 'Main Menu',
      sections: sections.filter((section) => section.group === 'main'),
    },
    {
      key: 'settings',
      label: 'Settings',
      sections: sections.filter((section) => section.group === 'settings'),
    },
  ];

  return (
    <div className="studio-navrail">
      <div className="studio-nav-safe-region" data-window-drag-region aria-hidden="true" />

      <div ref={groupsRef} className="studio-nav-groups liquid-scroll-surface">
        {groupedSections.map((group) => (
          <section
            key={group.key}
            className={
              group.key === 'top' ? 'studio-nav-group studio-nav-group--top' : 'studio-nav-group'
            }
          >
            {group.label ? <div className="studio-nav-group__title">{group.label}</div> : null}

            <nav className="studio-nav" aria-label={group.label || '导航'}>
              {group.sections.map((section) => {
                const Icon = SECTION_ICONS[section.key];

                return (
                  <button
                    key={section.key}
                    type="button"
                    className={
                      activeSection === section.key
                        ? 'studio-nav__button is-active'
                        : 'studio-nav__button'
                    }
                    aria-current={activeSection === section.key ? 'page' : undefined}
                    data-testid={`sidebar-${section.key}`}
                    onClick={() => onSectionChange(section.key)}
                  >
                    <span className="studio-nav__icon" aria-hidden="true">
                      <Icon />
                    </span>
                    <span className="studio-nav__copy">
                      <span className="studio-nav__label">{section.label}</span>
                      <span className="studio-nav__meta">{section.badge}</span>
                    </span>
                  </button>
                );
              })}
            </nav>
          </section>
        ))}
      </div>

      <div className="studio-nav__footer">
        <div className="studio-nav-status">
          <button
            type="button"
            className="studio-nav-theme-toggle"
            aria-label={isDarkMode ? '切换到亮色主题' : '切换到暗色主题'}
            title="切换主题"
            onClick={onToggleTheme}
          >
            {isDarkMode ? <SunIcon /> : <MoonIcon />}
          </button>
          <span className={`runtime-pill ${workflowStatusPillClass}`} data-testid="workflow-status">
            {workflowStatusLabel}
          </span>
        </div>

        <section className="studio-nav-user" aria-label="当前用户">
          <div className="studio-nav-user__avatar" aria-hidden="true">
            {getUserInitials(userName)}
          </div>
          <div className="studio-nav-user__copy">
            <strong>{userName}</strong>
            <span>{userRole}</span>
          </div>

          <button
            type="button"
            className="studio-nav-user__action"
            aria-label="切换用户"
            title="切换用户"
            onClick={onUserSwitch}
          >
            <SwitchUserIcon />
          </button>
        </section>
      </div>
    </div>
  );
}
