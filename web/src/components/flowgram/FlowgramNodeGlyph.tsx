import type { SVGProps } from 'react';

import {
  DebugConsoleNodeIcon,
  HttpClientNodeIcon,
  IfNodeIcon,
  LoopNodeIcon,
  ModbusNodeIcon,
  NativeNodeIcon,
  ScriptNodeIcon,
  SqlWriterNodeIcon,
  SwitchNodeIcon,
  TimerNodeIcon,
  TryCatchNodeIcon,
} from '../app/AppIcons';
import type { NazhNodeDisplayType } from './flowgram-node-library';

type IconProps = SVGProps<SVGSVGElement>;

export function normalizeFlowgramDisplayType(value: unknown): NazhNodeDisplayType {
  if (
    value === 'native' ||
    value === 'rhai' ||
    value === 'code' ||
    value === 'timer' ||
    value === 'modbusRead' ||
    value === 'if' ||
    value === 'switch' ||
    value === 'tryCatch' ||
    value === 'loop' ||
    value === 'httpClient' ||
    value === 'sqlWriter' ||
    value === 'debugConsole'
  ) {
    return value;
  }

  return value === 'rhai' ? 'rhai' : 'native';
}

export function getFlowgramDisplayLabel(displayType: NazhNodeDisplayType): string {
  switch (displayType) {
    case 'timer':
      return 'Timer';
    case 'modbusRead':
      return 'Modbus';
    case 'if':
      return 'IF';
    case 'switch':
      return 'SWITCH';
    case 'tryCatch':
      return 'TRY';
    case 'loop':
      return 'LOOP';
    case 'httpClient':
      return 'HTTP';
    case 'sqlWriter':
      return 'SQL';
    case 'debugConsole':
      return 'Debug';
    case 'code':
    case 'rhai':
      return 'Code';
    case 'native':
      return 'Native';
  }
}

export function FlowgramNodeGlyph({
  displayType,
  ...props
}: IconProps & { displayType: NazhNodeDisplayType }) {
  switch (displayType) {
    case 'timer':
      return <TimerNodeIcon {...props} />;
    case 'modbusRead':
      return <ModbusNodeIcon {...props} />;
    case 'if':
      return <IfNodeIcon {...props} />;
    case 'switch':
      return <SwitchNodeIcon {...props} />;
    case 'tryCatch':
      return <TryCatchNodeIcon {...props} />;
    case 'loop':
      return <LoopNodeIcon {...props} />;
    case 'httpClient':
      return <HttpClientNodeIcon {...props} />;
    case 'sqlWriter':
      return <SqlWriterNodeIcon {...props} />;
    case 'debugConsole':
      return <DebugConsoleNodeIcon {...props} />;
    case 'code':
    case 'rhai':
      return <ScriptNodeIcon {...props} />;
    case 'native':
      return <NativeNodeIcon {...props} />;
  }
}
