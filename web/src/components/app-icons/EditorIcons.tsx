import { BaseIcon } from './BaseIcon';
import type { IconProps } from './BaseIcon';

export function SearchIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="m20 20-4.3-4.3" />
    </BaseIcon>
  );
}

export function MinimapIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="5" width="16" height="14" rx="2.5" />
      <path d="M8 9h8" />
      <path d="M8 13h5" />
      <rect x="14.5" y="11.5" width="3" height="3" rx="0.8" />
    </BaseIcon>
  );
}

export function LockClosedIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="6" y="10" width="12" height="10" rx="2.4" />
      <path d="M8.5 10V8.3a3.5 3.5 0 1 1 7 0V10" />
    </BaseIcon>
  );
}

export function LockOpenIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="6" y="10" width="12" height="10" rx="2.4" />
      <path d="M9 10V8.5a3.5 3.5 0 0 1 6-2.5" />
    </BaseIcon>
  );
}

export function MouseModeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M9 4.8a3 3 0 0 1 6 0v14.4a3 3 0 0 1-6 0Z" />
      <path d="M12 5v5" />
    </BaseIcon>
  );
}

export function TrackpadModeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="6" width="16" height="12" rx="2.6" />
      <path d="M8 14h8" />
      <path d="M12 10v4" />
    </BaseIcon>
  );
}

export function ZoomInIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="10.5" cy="10.5" r="5.5" />
      <path d="M10.5 8v5" />
      <path d="M8 10.5h5" />
      <path d="m15.2 15.2 4.3 4.3" />
    </BaseIcon>
  );
}

export function ZoomOutIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="10.5" cy="10.5" r="5.5" />
      <path d="M8 10.5h5" />
      <path d="m15.2 15.2 4.3 4.3" />
    </BaseIcon>
  );
}

export function FitViewIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M8 5H5v3" />
      <path d="M16 5h3v3" />
      <path d="M8 19H5v-3" />
      <path d="M16 19h3v-3" />
    </BaseIcon>
  );
}

export function AutoLayoutIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="5" width="6" height="5" rx="1.5" />
      <rect x="14" y="5" width="6" height="5" rx="1.5" />
      <rect x="9" y="14" width="6" height="5" rx="1.5" />
      <path d="M10 8h4" />
      <path d="M12 10v4" />
    </BaseIcon>
  );
}

export function BezierIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="6" cy="16" r="1.5" />
      <circle cx="18" cy="8" r="1.5" />
      <path d="M7.5 15.5c2.5 0 3-5 6-5s3.5 0 4.5-1.5" />
    </BaseIcon>
  );
}

export function FoldLineIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="6" cy="16" r="1.5" />
      <circle cx="18" cy="8" r="1.5" />
      <path d="M7.5 16H12V8h4.5" />
    </BaseIcon>
  );
}

export function GroupNodesIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="6" width="7" height="7" rx="1.6" />
      <rect x="13" y="11" width="7" height="7" rx="1.6" />
      <path d="M11 9h2" />
      <path d="M12 8v2" />
    </BaseIcon>
  );
}

export function UngroupNodesIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="6" width="7" height="7" rx="1.6" />
      <rect x="13" y="11" width="7" height="7" rx="1.6" />
      <path d="M11 9h2" />
    </BaseIcon>
  );
}
