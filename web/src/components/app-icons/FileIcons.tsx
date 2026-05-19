import { BaseIcon } from './BaseIcon';
import type { IconProps } from './BaseIcon';

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

export function FilePdfIcon(props: IconProps) {
  return (
    <BaseIcon {...props}>
      <path d="M7 4h7l4 4v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M14 4v4h4" />
      <path d="M9 10h3" />
      <path d="M9 13h6" />
      <path d="M9 16h4" />
    </BaseIcon>
  );
}
