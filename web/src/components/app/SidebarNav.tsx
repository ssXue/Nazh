import {
  AboutIcon,
  CanvasIcon,
  ConnectionsIcon,
  OverviewIcon,
  PayloadIcon,
  SettingsIcon,
  SourceIcon,
  SwitchUserIcon,
} from './AppIcons';
import type { SidebarNavProps } from './types';

const SECTION_ICONS = {
  canvas: CanvasIcon,
  overview: OverviewIcon,
  source: SourceIcon,
  connections: ConnectionsIcon,
  payload: PayloadIcon,
  settings: SettingsIcon,
  about: AboutIcon,
} as const;

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
}: SidebarNavProps) {
  const groupedSections = [
    {
      key: 'main',
      label: '主菜单',
      sections: sections.filter((section) => section.group === 'main'),
    },
    {
      key: 'settings',
      label: '设置',
      sections: sections.filter((section) => section.group === 'settings'),
    },
  ];

  return (
    <div className="studio-navrail">
      <div className="studio-navrail__header">
        <strong>导航</strong>
      </div>

      <div className="studio-nav-groups">
        {groupedSections.map((group) => (
          <section key={group.key} className="studio-nav-group">
            <div className="studio-nav-group__title">{group.label}</div>

            <nav className="studio-nav" aria-label={`${group.label} 导航`}>
              {group.sections.map((section) => {
                const Icon = SECTION_ICONS[section.key];

                return (
                  <button
                    key={section.key}
                    type="button"
                    className={activeSection === section.key ? 'studio-nav__button is-active' : 'studio-nav__button'}
                    aria-current={activeSection === section.key ? 'page' : undefined}
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
