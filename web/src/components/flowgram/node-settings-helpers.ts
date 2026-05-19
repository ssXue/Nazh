import { type FlowNodeEntity } from '@flowgram.ai/free-layout-editor';

import {
  getDefaultBarkBodyTemplate,
  getDefaultBarkTitleTemplate,
  getLogicNodeBranchDefinitions,
  getDefaultHttpAlarmBodyTemplate,
  getDefaultHttpAlarmTitleTemplate,
  inferHttpWebhookKind,
  normalizeNodeKind,
  normalizeHttpBodyMode,
  resolveNodeDisplayLabel,
  type FlowgramLogicBranch,
} from './flowgram-node-library';

import {
  type SelectedNodeDraft,
  isRecord,
  readString,
  readBoolean,
  readNumberString,
  parsePositiveInteger,
  parseNonNegativeInteger,
  parseFiniteNumber,
  isScriptNode,
  supportsScriptAi,
} from './nodes/settings-shared';

import type { NodeConfigMap } from './node-settings-types';

function readEditableBranches(nodeType: string, config: unknown): FlowgramLogicBranch[] {
  if (nodeType !== 'switch') {
    return [];
  }

  return getLogicNodeBranchDefinitions(nodeType, config).filter((branch) => !branch.fixed);
}

export function readNodeDraft(node: FlowNodeEntity): SelectedNodeDraft {
  const rawData = (node.getExtInfo() ?? {}) as {
    label?: string;
    nodeType?: string;
    connectionId?: string | null;
    timeoutMs?: number | null;
    config?: unknown;
  };
  const config = isRecord(rawData.config) ? rawData.config : {};
  const nodeType = normalizeNodeKind(rawData.nodeType ?? node.flowNodeType);
  const httpWebhookKind = readString(
    config.webhook_kind,
    inferHttpWebhookKind(readString(config.url)),
  );
  const httpBodyMode = normalizeHttpBodyMode(config.body_mode, httpWebhookKind);

  return {
    id: node.id,
    nodeType,
    label: resolveNodeDisplayLabel(nodeType, rawData.label),
    connectionId: rawData.connectionId ?? '',
    timeoutMs: rawData.timeoutMs ? String(rawData.timeoutMs) : '',
    message: readString(config.message),
    script: readString(config.script),
    branches: readEditableBranches(nodeType, config),
    timerIntervalMs: readNumberString(config.interval_ms, '5000'),
    timerImmediate: readBoolean(config.immediate, true),
    modbusUnitId: readNumberString(config.unit_id, '1'),
    modbusRegister: readNumberString(config.register, '40001'),
    modbusQuantity: readNumberString(config.quantity, '1'),
    modbusRegisterType: readString(config.register_type, 'holding'),
    modbusBaseValue: readNumberString(config.base_value, '64'),
    modbusAmplitude: readNumberString(config.amplitude, '6'),
    canId: readNumberString(config.can_id, ''),
    canIsExtended: readBoolean(config.is_extended, false),
    canReadTimeoutMs: readNumberString(config.timeout_ms, '1000'),
    mqttMode: readString(config.mode, 'publish'),
    mqttTopic: readString(config.topic, ''),
    mqttQos: typeof config.qos === 'number' ? String(config.qos) : '0',
    mqttPayloadTemplate: readString(config.payload_template, ''),
    httpWebhookKind,
    httpBodyMode,
    httpTitleTemplate: readString(config.title_template, getDefaultHttpAlarmTitleTemplate()),
    httpBodyTemplate: readString(
      config.body_template,
      httpBodyMode === 'dingtalk_markdown' ? getDefaultHttpAlarmBodyTemplate() : '',
    ),
    barkContentMode: readString(config.content_mode, 'body'),
    barkTitleTemplate: readString(config.title_template, getDefaultBarkTitleTemplate()),
    barkSubtitleTemplate: readString(config.subtitle_template, ''),
    barkBodyTemplate: readString(config.body_template, getDefaultBarkBodyTemplate()),
    barkLevel: readString(config.level, 'active'),
    barkBadge:
      typeof config.badge === 'number'
        ? String(config.badge)
        : readString(config.badge, ''),
    barkSound: readString(config.sound, ''),
    barkIcon: readString(config.icon, ''),
    barkGroup: readString(config.group, ''),
    barkUrl: readString(config.url, ''),
    barkCopy: readString(config.copy, ''),
    barkImage: readString(config.image, ''),
    barkAutoCopy: readBoolean(config.auto_copy, false),
    barkCall: readBoolean(config.call, false),
    barkArchiveMode: readString(config.archive_mode, 'inherit'),
    sqlDatabasePath: readString(config.database_path, './nazh-local.sqlite3'),
    sqlTable: readString(config.table, 'workflow_logs'),
    debugLabel: readString(config.label),
    debugPretty: readBoolean(config.pretty, true),
    ethercatSlaveAddress: readNumberString(config.slave_address, '1'),
    capabilityId: readString(config.capability_id),
    capabilityDeviceId: readString(config.device_id),
    capabilityImplementationJson: JSON.stringify(
      isRecord(config.implementation)
        ? config.implementation
        : { type: 'script', content: 'payload' },
      null,
      2,
    ),
    capabilityArgsJson: JSON.stringify(isRecord(config.args) ? config.args : {}, null, 2),
    parameterBindings: (() => {
      const raw = config.parameterBindings;
      if (typeof raw === 'object' && raw !== null && !Array.isArray(raw)) {
        return raw as Record<string, string | number | boolean>;
      }
      return {};
    })(),
    lookupTable: (() => {
      const raw = config.table;
      if (typeof raw === 'object' && raw !== null && !Array.isArray(raw)) {
        return raw as Record<string, unknown>;
      }
      return {};
    })(),
    lookupDefault: (() => {
      const raw = config.default;
      return raw === null || raw === undefined ? '' : JSON.stringify(raw);
    })(),
    hitlTitle: readString(config.title),
    hitlDescription: readString(config.description),
    hitlApprovalTimeoutSec: (() => {
      const raw = config.approval_timeout_ms;
      return typeof raw === 'number' && raw > 0 ? String(Math.round(raw / 1000)) : '';
    })(),
    hitlDefaultAction: (() => {
      const raw = config.default_action;
      return typeof raw === 'string' && ['autoApprove', 'autoReject'].includes(raw) ? raw : 'autoReject';
    })(),
    hitlFormSchemaJson: (() => {
      const raw = config.form_schema;
      if (Array.isArray(raw) && raw.length > 0) return JSON.stringify(raw, null, 2);
      return '';
    })(),
  };
}

export function buildNodeConfig(draft: SelectedNodeDraft, currentConfig: NodeConfigMap): NodeConfigMap {
  if (draft.nodeType === 'native') {
    return { ...currentConfig, message: draft.message };
  }

  if (draft.nodeType === 'timer') {
    return { ...currentConfig, interval_ms: parsePositiveInteger(draft.timerIntervalMs) ?? 5000, immediate: draft.timerImmediate, inject: isRecord(currentConfig.inject) ? currentConfig.inject : {} };
  }

  if (draft.nodeType === 'serialTrigger') {
    return { inject: isRecord(currentConfig.inject) ? currentConfig.inject : {} };
  }

  if (draft.nodeType === 'modbusRead') {
    return { ...currentConfig, unit_id: parsePositiveInteger(draft.modbusUnitId) ?? 1, register: parsePositiveInteger(draft.modbusRegister) ?? 40001, quantity: parsePositiveInteger(draft.modbusQuantity) ?? 1, register_type: draft.modbusRegisterType || 'holding', base_value: parseFiniteNumber(draft.modbusBaseValue) ?? 64, amplitude: parseFiniteNumber(draft.modbusAmplitude) ?? 6 };
  }

  if (draft.nodeType === 'canRead') {
    return {
      ...currentConfig,
      can_id: parseNonNegativeInteger(draft.canId),
      is_extended: draft.canIsExtended,
      timeout_ms: parsePositiveInteger(draft.canReadTimeoutMs) ?? 1000,
    };
  }

  if (draft.nodeType === 'canWrite') {
    return {
      ...currentConfig,
      can_id: parseNonNegativeInteger(draft.canId),
      is_extended: draft.canIsExtended,
    };
  }

  if (draft.nodeType === 'ethercatPdoRead' || draft.nodeType === 'ethercatPdoWrite') {
    return {
      ...currentConfig,
      slave_address: parsePositiveInteger(draft.ethercatSlaveAddress) ?? 1,
    };
  }

  if (draft.nodeType === 'ethercatStatus') {
    return { ...currentConfig };
  }

  if (draft.nodeType === 'mqttClient') {
    return { ...currentConfig, mode: draft.mqttMode === 'subscribe' ? 'subscribe' : 'publish', topic: draft.mqttTopic.trim(), qos: [0, 1, 2].includes(Number(draft.mqttQos)) ? Number(draft.mqttQos) : 0, payload_template: draft.mqttPayloadTemplate };
  }

  if (draft.nodeType === 'switch') {
    return { ...currentConfig, script: draft.script, branches: draft.branches.map((branch) => ({ key: branch.key, label: branch.label })) };
  }

  if (draft.nodeType === 'httpClient') {
    const { url: _u, method: _m, headers: _h, webhook_kind: _w, content_type: _c, request_timeout_ms: _r, at_mobiles: _a1, at_all: _a2, ...restConfig } = currentConfig;
    return { ...restConfig, body_mode: normalizeHttpBodyMode(draft.httpBodyMode, draft.httpWebhookKind), title_template: draft.httpTitleTemplate, body_template: draft.httpBodyTemplate };
  }

  if (draft.nodeType === 'barkPush') {
    const { server_url: _s, device_key: _d, request_timeout_ms: _r, ...restConfig } = currentConfig;
    return { ...restConfig, content_mode: draft.barkContentMode === 'markdown' ? 'markdown' : 'body', title_template: draft.barkTitleTemplate, subtitle_template: draft.barkSubtitleTemplate, body_template: draft.barkBodyTemplate, level: ['active', 'timeSensitive', 'passive', 'critical'].includes(draft.barkLevel) ? draft.barkLevel : 'active', badge: parseNonNegativeInteger(draft.barkBadge) ?? '', sound: draft.barkSound, icon: draft.barkIcon, group: draft.barkGroup, url: draft.barkUrl, copy: draft.barkCopy, image: draft.barkImage, auto_copy: draft.barkAutoCopy, call: draft.barkCall, archive_mode: ['inherit', 'archive', 'skip'].includes(draft.barkArchiveMode) ? draft.barkArchiveMode : 'inherit' };
  }

  if (draft.nodeType === 'sqlWriter') {
    return { ...currentConfig, database_path: draft.sqlDatabasePath.trim() || './nazh-local.sqlite3', table: draft.sqlTable.trim() || 'workflow_logs' };
  }

  if (draft.nodeType === 'debugConsole') {
    return { ...currentConfig, label: draft.debugLabel.trim(), pretty: draft.debugPretty };
  }

  if (draft.nodeType === 'capabilityCall') {
    const implementation = (() => {
      try {
        const parsed = JSON.parse(draft.capabilityImplementationJson);
        return isRecord(parsed) ? parsed : currentConfig.implementation;
      } catch {
        return currentConfig.implementation;
      }
    })();
    const args = (() => {
      try {
        const parsed = JSON.parse(draft.capabilityArgsJson);
        return isRecord(parsed) ? parsed : currentConfig.args;
      } catch {
        return currentConfig.args;
      }
    })();
    return {
      ...currentConfig,
      capability_id: draft.capabilityId.trim(),
      device_id: draft.capabilityDeviceId.trim(),
      implementation: isRecord(implementation)
        ? implementation
        : { type: 'script', content: 'payload' },
      args: isRecord(args) ? args : {},
    };
  }

  if (draft.nodeType === 'subgraph') {
    return { ...currentConfig, parameterBindings: draft.parameterBindings };
  }

  if (draft.nodeType === 'lookup') {
    const defaultVal = draft.lookupDefault.trim() === '' ? null
      : (() => { try { return JSON.parse(draft.lookupDefault); } catch { return draft.lookupDefault; } })();
    return { ...currentConfig, table: draft.lookupTable, default: defaultVal };
  }

  if (draft.nodeType === 'humanLoop') {
    const timeoutSec = parsePositiveInteger(draft.hitlApprovalTimeoutSec);
    return {
      ...currentConfig,
      title: draft.hitlTitle.trim() || null,
      description: draft.hitlDescription.trim() || null,
      approval_timeout_ms: timeoutSec != null ? timeoutSec * 1000 : null,
      default_action: draft.hitlDefaultAction || 'autoReject',
      form_schema: (() => {
        const text = draft.hitlFormSchemaJson.trim();
        if (!text) return [];
        try { return JSON.parse(text); } catch { return []; }
      })(),
    };
  }

  if (supportsScriptAi(draft.nodeType)) {
    const { ai: _ai, ...restConfig } = currentConfig;
    return { ...restConfig, script: draft.script };
  }

  if (isScriptNode(draft.nodeType)) {
    return { ...currentConfig, script: draft.script };
  }

  return currentConfig;
}
