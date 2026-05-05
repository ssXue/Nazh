import type { SVGProps } from 'react';

import {
  BarkNodeIcon,
  DebugConsoleNodeIcon,
  HttpClientNodeIcon,
  IfNodeIcon,
  LoopNodeIcon,
  ModbusNodeIcon,
  NativeNodeIcon,
  ScriptNodeIcon,
  SerialNodeIcon,
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
    value === 'code' ||
    value === 'timer' ||
    value === 'serialTrigger' ||
    value === 'modbusRead' ||
    value === 'capabilityCall' ||
    value === 'mqttClient' ||
    value === 'if' ||
    value === 'switch' ||
    value === 'tryCatch' ||
    value === 'loop' ||
    value === 'httpClient' ||
    value === 'barkPush' ||
    value === 'sqlWriter' ||
    value === 'debugConsole' ||
    value === 'subgraph' ||
    value === 'subgraphInput' ||
    value === 'subgraphOutput' ||
    value === 'c2f' ||
    value === 'minutesSince' ||
    value === 'lookup' ||
    value === 'humanLoop'
  ) {
    return value;
  }

  return 'native';
}

export function getFlowgramDisplayLabel(displayType: NazhNodeDisplayType): string {
  switch (displayType) {
    case 'humanLoop':
      return '审批';
    case 'timer':
      return 'Timer';
    case 'serialTrigger':
      return 'Serial';
    case 'modbusRead':
      return 'Modbus';
    case 'capabilityCall':
      return 'Capability';
    case 'mqttClient':
      return 'MQTT';
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
    case 'barkPush':
      return 'Bark';
    case 'sqlWriter':
      return 'SQL';
    case 'debugConsole':
      return 'Debug';
    case 'subgraph':
      return 'SUB';
    case 'subgraphInput':
      return 'IN';
    case 'subgraphOutput':
      return 'OUT';
    case 'c2f':
      return 'C→F';
    case 'minutesSince':
      return '分钟';
    case 'lookup':
      return '查找';
    case 'code':
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
    case 'serialTrigger':
      return <SerialNodeIcon {...props} />;
    case 'modbusRead':
      return <ModbusNodeIcon {...props} />;
    case 'capabilityCall':
      return <ModbusNodeIcon {...props} />;
    case 'mqttClient':
      return <NativeNodeIcon {...props} />;
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
    case 'barkPush':
      return <BarkNodeIcon {...props} />;
    case 'sqlWriter':
      return <SqlWriterNodeIcon {...props} />;
    case 'debugConsole':
      return <DebugConsoleNodeIcon {...props} />;
    case 'subgraph':
      return <SwitchNodeIcon {...props} />;
    case 'subgraphInput':
    case 'subgraphOutput':
      return <NativeNodeIcon {...props} />;
    case 'code':
      return <ScriptNodeIcon {...props} />;
    case 'native':
      return <NativeNodeIcon {...props} />;
    case 'c2f':
    case 'minutesSince':
    case 'lookup':
      return <NativeNodeIcon {...props} />;
    case 'humanLoop':
      return <IfNodeIcon {...props} />;
  }
}
