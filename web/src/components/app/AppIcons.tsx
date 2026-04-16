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

export function RunActionIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m8 6 9 6-9 6Z" />
    </BaseIcon>
  );
}

export function StopActionIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="7" y="7" width="10" height="10" rx="2.4" />
    </BaseIcon>
  );
}

export function TriggerActionIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m7 6 7 6-7 6Z" />
      <path d="M16 7v10" />
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

export function CopyIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="9" y="9" width="10" height="10" rx="2.2" />
      <path d="M7 15H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h7a2 2 0 0 1 2 2v1" />
    </BaseIcon>
  );
}

export function DownloadIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M12 4v10" />
      <path d="m8 10 4 4 4-4" />
      <path d="M5 18h14" />
    </BaseIcon>
  );
}

export function UploadIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M12 20V10" />
      <path d="m8 14 4-4 4 4" />
      <path d="M5 6h14" />
    </BaseIcon>
  );
}

export function SaveIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M6 4h10l4 4v12H4V6a2 2 0 0 1 2-2Z" />
      <path d="M8 4v5h8" />
      <path d="M8 20v-6h8v6" />
    </BaseIcon>
  );
}

export function SnapshotIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="6" width="16" height="12" rx="2.6" />
      <path d="M8 10h8" />
      <path d="M8 14h5" />
      <path d="M12 4v4" />
      <path d="m10 6 2-2 2 2" />
    </BaseIcon>
  );
}

export function HistoryIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 12a8 8 0 1 0 2.3-5.7" />
      <path d="M4 5v4h4" />
      <path d="M12 8v4l2.5 1.8" />
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

export function PlusIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </BaseIcon>
  );
}

export function FileImageIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 4h7l4 4v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M14 4v4h4" />
      <circle cx="10" cy="11" r="1.5" />
      <path d="m8 17 2.5-2.5 2 2 2.5-3 2 3.5" />
    </BaseIcon>
  );
}

export function FileVectorIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 4h7l4 4v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M14 4v4h4" />
      <circle cx="9" cy="15" r="1.3" />
      <circle cx="15" cy="10" r="1.3" />
      <circle cx="15.5" cy="16" r="1.3" />
      <path d="M10.1 14.4 13.9 10.6" />
      <path d="m10.3 15.5 3.9.4" />
    </BaseIcon>
  );
}

export function FileJsonIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 4h7l4 4v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M14 4v4h4" />
      <path d="M10 10c-.8 0-1.4.6-1.4 1.4v1.2c0 .7-.3 1.2-.9 1.4.6.2.9.7.9 1.4v1.2c0 .8.6 1.4 1.4 1.4" />
      <path d="M14 10c.8 0 1.4.6 1.4 1.4v1.2c0 .7.3 1.2.9 1.4-.6.2-.9.7-.9 1.4v1.2c0 .8-.6 1.4-1.4 1.4" />
    </BaseIcon>
  );
}

export function FileYamlIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 4h7l4 4v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M14 4v4h4" />
      <path d="m8.5 10 2.2 3 2.2-3" />
      <path d="M10.7 13v4" />
      <path d="m14.5 10 1.7 2.4" />
      <path d="m18 10-1.8 2.4" />
      <path d="M16.2 12.4V17" />
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

export function UndoActionIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M9 7 5 11l4 4" />
      <path d="M5 11h8a5 5 0 1 1 0 10h-1" />
    </BaseIcon>
  );
}

export function RedoActionIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m15 7 4 4-4 4" />
      <path d="M19 11h-8a5 5 0 1 0 0 10h1" />
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

export function MoveOutIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <rect x="4" y="5" width="10" height="14" rx="2" />
      <path d="M14 12h6" />
      <path d="m17 9 3 3-3 3" />
    </BaseIcon>
  );
}

export function InspectIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M4 12h16" />
      <path d="M12 4v16" />
      <circle cx="12" cy="12" r="7" />
    </BaseIcon>
  );
}

export function DeleteActionIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M5 7h14" />
      <path d="M9 7V5.5h6V7" />
      <path d="M8 9.5v8" />
      <path d="M12 9.5v8" />
      <path d="M16 9.5v8" />
      <path d="M6.5 7.5 7 19h10l.5-11.5" />
    </BaseIcon>
  );
}

export function AiIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M12 2a4 4 0 0 0-4 4v2H6a2 2 0 0 0-2 2v2a6 6 0 0 0 4.2 5.7" />
      <path d="M12 2a4 4 0 0 1 4 4v2h2a2 2 0 0 1 2 2v2a6 6 0 0 1-4.2 5.7" />
      <path d="M9 16.5c.7.8 1.8 1.5 3 1.5s2.3-.7 3-1.5" />
      <circle cx="9" cy="11" r="1.2" />
      <circle cx="15" cy="11" r="1.2" />
      <path d="M10 21h4" />
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
