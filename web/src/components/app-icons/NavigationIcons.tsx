import { BaseIcon } from './BaseIcon';
import type { IconProps } from './BaseIcon';

export function CanvasIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="4" width="16" height="16" rx="3" />
      <path d="M8 8h8" />
      <path d="M8 12h8" />
      <path d="M8 16h5" />
    </BaseIcon>
  );
}

export function DashboardIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 14a8 8 0 0 1 16 0" />
      <path d="M12 14 15.5 9" />
      <circle cx="12" cy="14" r="1.5" />
    </BaseIcon>
  );
}

export function BoardsIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="3" y="3" width="7" height="9" rx="1.5" />
      <rect x="14" y="3" width="7" height="5" rx="1.5" />
      <rect x="14" y="12" width="7" height="9" rx="1.5" />
      <rect x="3" y="16" width="7" height="5" rx="1.5" />
    </BaseIcon>
  );
}

export function BackIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m15 18-6-6 6-6" />
    </BaseIcon>
  );
}

export function ConnectionsIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="6" cy="6" r="2.5" />
      <circle cx="18" cy="6" r="2.5" />
      <circle cx="12" cy="18" r="2.5" />
      <path d="M8.1 7.3 10.6 15" />
      <path d="M15.9 7.3 13.4 15" />
    </BaseIcon>
  );
}

export function PayloadIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 12h12" />
      <path d="m12 6 6 6-6 6" />
    </BaseIcon>
  );
}

export function LogsIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M5 7h14" />
      <path d="M5 12h10" />
      <path d="M5 17h8" />
      <circle cx="18" cy="12" r="1.6" />
      <circle cx="16" cy="17" r="1.6" />
    </BaseIcon>
  );
}

export function DockToggleIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m7 10 5 5 5-5" />
    </BaseIcon>
  );
}

export function BottomPanelIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="3.5" y="5" width="17" height="14" rx="2.4" />
      <path d="M3.5 14h17" />
    </BaseIcon>
  );
}

export function ChevronDownIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m7 10 5 5 5-5" />
    </BaseIcon>
  );
}

export function SidebarToggleIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="3.5" y="5" width="17" height="14" rx="2.4" />
      <path d="M9.5 5v14" />
      <path d="M6 9h1" />
      <path d="M6 12h1" />
      <path d="M6 15h1" />
    </BaseIcon>
  );
}
