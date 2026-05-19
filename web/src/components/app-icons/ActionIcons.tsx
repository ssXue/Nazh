import { BaseIcon } from './BaseIcon';
import type { IconProps } from './BaseIcon';

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

export function PlusIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </BaseIcon>
  );
}

export function SparklesIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="m12 3 1.6 3.9L17.5 8.5l-3.9 1.6L12 14l-1.6-3.9L6.5 8.5l3.9-1.6L12 3Z" />
      <path d="m18.5 14 0.9 2.1 2.1 0.9-2.1 0.9-0.9 2.1-0.9-2.1-2.1-0.9 2.1-0.9 0.9-2.1Z" />
      <path d="m5.5 13 0.7 1.7 1.8 0.8-1.8 0.7-0.7 1.8-0.8-1.8-1.7-0.7 1.7-0.8 0.8-1.7Z" />
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
