import { BaseIcon } from './BaseIcon';
import type { IconProps } from './BaseIcon';

export function NativeNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="7" cy="7" r="2.4" />
      <circle cx="17" cy="17" r="2.4" />
      <path d="M8.8 8.8 15.2 15.2" />
      <path d="M7 9.8v6.4" />
    </BaseIcon>
  );
}

export function ScriptNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m9 7-4 5 4 5" />
      <path d="m15 7 4 5-4 5" />
      <path d="m13 5-2 14" />
    </BaseIcon>
  );
}

export function IfNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M12 4 18 10 12 16 6 10 12 4Z" />
      <path d="M12 16v4" />
      <path d="M8.5 20h7" />
    </BaseIcon>
  );
}

export function SwitchNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M6 7h8" />
      <path d="m10 3 4 4-4 4" />
      <path d="M14 17H6" />
      <path d="m10 13 4 4-4 4" />
      <path d="M18 7v10" />
    </BaseIcon>
  );
}

export function TryCatchNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 5h10v4c0 4-2.2 7.5-5 9-2.8-1.5-5-5-5-9V5Z" />
      <path d="M10 10h4" />
      <path d="M10 13h3" />
    </BaseIcon>
  );
}

export function LoopNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 8a5 5 0 0 1 8.6-2.9" />
      <path d="m16 3 1.4 2.7L20 5" />
      <path d="M17 16a5 5 0 0 1-8.6 2.9" />
      <path d="m8 21-1.4-2.7L4 19" />
    </BaseIcon>
  );
}

export function TimerNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <circle cx="12" cy="12" r="7" />
      <path d="M12 8v4l2.8 1.8" />
      <path d="M9 3h6" />
    </BaseIcon>
  );
}

export function SerialNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M6 7h6" />
      <path d="M6 12h12" />
      <path d="M12 17h6" />
      <circle cx="4.8" cy="7" r="1.4" />
      <circle cx="19.2" cy="17" r="1.4" />
      <path d="M14.5 7h2.5a2 2 0 0 1 2 2v1" />
      <path d="M9.5 17H7a2 2 0 0 1-2-2v-1" />
    </BaseIcon>
  );
}

export function ModbusNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="5" y="5" width="14" height="14" rx="3" />
      <path d="M9 9h6" />
      <path d="M9 13h6" />
      <path d="M9 17h3" />
    </BaseIcon>
  );
}

export function CanNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 9h16" />
      <path d="M4 15h16" />
      <circle cx="7" cy="9" r="2.2" />
      <circle cx="17" cy="15" r="2.2" />
      <path d="M9.2 9h5.6" />
      <path d="M9.2 15h5.6" />
    </BaseIcon>
  );
}

export function HttpClientNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 12h10" />
      <path d="m11 6 6 6-6 6" />
      <path d="M5 6h3" />
      <path d="M5 18h3" />
    </BaseIcon>
  );
}

export function BarkNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M9 8a3 3 0 0 1 6 0v2.6c0 .8.3 1.6.86 2.16L17 14v1H7v-1l1.14-1.24A3.08 3.08 0 0 0 9 10.6V8" />
      <path d="M10.2 18a1.8 1.8 0 0 0 3.6 0" />
      <path d="M18.2 9.1a4.2 4.2 0 0 1 0 5.8" />
      <path d="M5.8 14.9a4.2 4.2 0 0 1 0-5.8" />
    </BaseIcon>
  );
}

export function SqlWriterNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <ellipse cx="12" cy="6.5" rx="6.5" ry="2.8" />
      <path d="M5.5 6.5v8.5c0 1.5 2.9 2.8 6.5 2.8s6.5-1.3 6.5-2.8V6.5" />
      <path d="M5.5 11c0 1.5 2.9 2.8 6.5 2.8s6.5-1.3 6.5-2.8" />
    </BaseIcon>
  );
}

export function DebugConsoleNodeIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="5" width="16" height="12" rx="2.5" />
      <path d="m8 10 2 2-2 2" />
      <path d="M12.5 14H16" />
      <path d="M10 19h4" />
    </BaseIcon>
  );
}

export function SignalIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M2 20h2" />
      <path d="M6 20h2" />
      <path d="M10 20h2" />
      <path d="M14 20h2" />
      <path d="M18 20h2" />
      <path d="M5 16a7 7 0 0 1 14 0" />
      <path d="M8.5 16a3.5 3.5 0 0 1 7 0" />
    </BaseIcon>
  );
}

export function DeviceIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="5" y="5" width="14" height="14" rx="3" />
      <circle cx="9" cy="10" r="1.5" />
      <circle cx="15" cy="10" r="1.5" />
      <path d="M9 15h6" />
    </BaseIcon>
  );
}
