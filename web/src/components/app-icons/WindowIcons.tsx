import { BaseIcon } from './BaseIcon';
import type { IconProps } from './BaseIcon';

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

export function XCloseIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m7 7 10 10" />
      <path d="m17 7-10 10" />
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

export function SlidersIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 21v-7" />
      <path d="M4 10V3" />
      <path d="M12 21v-9" />
      <path d="M12 8V3" />
      <path d="M20 21v-5" />
      <path d="M20 12V3" />
      <line x1="2" y1="14" x2="6" y2="14" />
      <line x1="10" y1="8" x2="14" y2="8" />
      <line x1="18" y1="16" x2="22" y2="16" />
    </BaseIcon>
  );
}

export function PencilIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M15.5 4.5a2.1 2.1 0 1 1 3 3L7 19l-4 1 1-4Z" />
    </BaseIcon>
  );
}

export function CheckCircleIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="12" cy="12" r="8" />
      <path d="m9 12 2 2 4-4" />
    </BaseIcon>
  );
}

export function ResetIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M3 12a9 9 0 1 0 9-9 9.7 9.7 0 0 0-6.7 2.8" />
      <path d="M3 3v5h5" />
    </BaseIcon>
  );
}

export function EnvironmentIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 7h16" />
      <path d="M4 12h10" />
      <path d="M4 17h8" />
      <circle cx="17.5" cy="12" r="2.5" />
      <circle cx="14.5" cy="17" r="2.2" />
    </BaseIcon>
  );
}

export function FolderOpenIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 20h16a2 2 0 0 0 1.8-1.2L22 9.6a1 1 0 0 0-.9-1.4H14l-1.4-2.8A2 2 0 0 0 11 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2Z" />
    </BaseIcon>
  );
}

export function PluginsIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="8" width="10" height="10" rx="2" />
      <rect x="10" y="4" width="10" height="10" rx="2" />
    </BaseIcon>
  );
}

export function AiIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="12" cy="5" r="2" />
      <circle cx="6" cy="14" r="2" />
      <circle cx="18" cy="14" r="2" />
      <path d="M12 7 6 12" />
      <path d="M12 7 18 12" />
      <path d="M8 14h8" />
    </BaseIcon>
  );
}
