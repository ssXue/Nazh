import type { SVGProps } from 'react';

type IconProps = SVGProps<SVGSVGElement>;

function BaseIcon(props: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    />
  );
}

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
      <rect x="3" y="3" width="8" height="8" rx="2" />
      <rect x="13" y="3" width="8" height="4" rx="1.5" />
      <rect x="13" y="9" width="8" height="12" rx="2" />
      <rect x="3" y="13" width="8" height="8" rx="2" />
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

export function SourceIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m9 8-4 4 4 4" />
      <path d="m15 8 4 4-4 4" />
      <path d="m13 5-2 14" />
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

export function SettingsIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.5-2.4.7a7.3 7.3 0 0 0-1.7-1l-.4-2.5h-4l-.4 2.5a7.3 7.3 0 0 0-1.7 1L5 6 3 9.5 5 11a8 8 0 0 0 0 2L3 14.5 5 18l2.4-.7a7.3 7.3 0 0 0 1.7 1l.4 2.5h4l.4-2.5a7.3 7.3 0 0 0 1.7-1l2.4.7 2-3.5-2-1.5c.1-.3.1-.7.1-1Z" />
    </BaseIcon>
  );
}

export function AboutIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="12" cy="12" r="8" />
      <path d="M12 10v6" />
      <path d="M12 7h.01" />
    </BaseIcon>
  );
}

export function SunIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2.5v2.2" />
      <path d="M12 19.3v2.2" />
      <path d="m4.9 4.9 1.6 1.6" />
      <path d="m17.5 17.5 1.6 1.6" />
      <path d="M2.5 12h2.2" />
      <path d="M19.3 12h2.2" />
      <path d="m4.9 19.1 1.6-1.6" />
      <path d="m17.5 6.5 1.6-1.6" />
    </BaseIcon>
  );
}

export function MoonIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M20.2 14.4A7.9 7.9 0 0 1 9.6 3.8a8.6 8.6 0 1 0 10.6 10.6Z" />
    </BaseIcon>
  );
}

export function SwitchUserIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 7h9" />
      <path d="m13 3 4 4-4 4" />
      <path d="M17 17H8" />
      <path d="m11 13-4 4 4 4" />
    </BaseIcon>
  );
}

export function WindowMinimizeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M5 12h14" />
    </BaseIcon>
  );
}

export function WindowMaximizeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="5.5" y="5.5" width="13" height="13" rx="2.2" />
    </BaseIcon>
  );
}

export function WindowRestoreIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M8 8h8v8" />
      <path d="M16 8 8 16" />
    </BaseIcon>
  );
}

export function WindowCloseIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m7 7 10 10" />
      <path d="m17 7-10 10" />
    </BaseIcon>
  );
}
