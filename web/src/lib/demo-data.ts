//! 示例工作流 AST 和项目草稿构建。

import { BOARD_LIBRARY } from '../components/app/BoardsPanel';
import type { JsonValue, WorkflowGraph } from '../types';
import { SAMPLE_AST, SAMPLE_PAYLOAD } from '../types';

/** 单个工程画板的编辑草稿（AST 文本 + 载荷文本）。 */
export interface ProjectDraft {
  astText: string;
  payloadText: string;
}

/** 默认操作员用户名，用于手动触发载荷。 */
export const CURRENT_USER_NAME = 'ssxue';

/** 默认激活的看板 ID（取库首项）。 */
export const DEFAULT_BOARD_ID = BOARD_LIBRARY[0]?.id ?? 'default';

/** 构建工业告警示例工作流图，适用于 "default" 看板。 */
export function buildIndustrialAlarmExample(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [
      {
        id: 'plc-main',
        type: 'modbus',
        metadata: {
          host: '192.168.10.11',
          port: 502,
          unit_id: 1,
          register: 40001,
        },
      },
    ],
    nodes: {
      timer_trigger: {
        type: 'timer',
        ai_description: 'Poll the PLC on a steady interval and seed runtime metadata.',
        config: {
          interval_ms: 5000,
          immediate: true,
          inject: {
            gateway: 'edge-a',
            scene: boardName,
          },
        },
        meta: {
          position: { x: 48, y: 88 },
        },
      },
      modbus_read: {
        type: 'modbusRead',
        connection_id: 'plc-main',
        ai_description: 'Read a simulated Modbus register from the main PLC.',
        timeout_ms: 1000,
        config: {
          unit_id: 1,
          register: 40001,
          quantity: 1,
          base_value: 68,
          amplitude: 6,
        },
        meta: {
          position: { x: 348, y: 88 },
        },
      },
      code_clean: {
        type: 'code',
        ai_description: 'Normalize the PLC value and derive route-ready severity fields.',
        timeout_ms: 1000,
        config: {
          script:
            'let value = payload["value"]; payload["temperature_c"] = value; payload["temperature_f"] = (value * 1.8) + 32.0; payload["severity"] = value > 120 ? "alert" : "nominal"; payload["route"] = payload["severity"]; payload["tag"] = `${payload["gateway"]}:boiler-a`; payload',
        },
        meta: {
          position: { x: 648, y: 88 },
        },
      },
      route_switch: {
        type: 'switch',
        ai_description: 'Route nominal telemetry into SQLite and alert telemetry into DingTalk.',
        timeout_ms: 1000,
        config: {
          script: 'payload["route"]',
          branches: [
            { key: 'nominal', label: 'Nominal' },
            { key: 'alert', label: 'Alert' },
          ],
        },
        meta: {
          position: { x: 968, y: 72 },
        },
      },
      sql_writer: {
        type: 'sqlWriter',
        ai_description: 'Persist nominal telemetry into a local SQLite audit table.',
        timeout_ms: 1500,
        config: {
          database_path: './data/edge-runtime.sqlite3',
          table: 'temperature_audit',
        },
        meta: {
          position: { x: 1288, y: 176 },
        },
      },
      http_alarm: {
        type: 'httpClient',
        ai_description: 'Send high severity telemetry to a DingTalk robot webhook with a rendered markdown alarm body.',
        timeout_ms: 1500,
        config: {
          method: 'POST',
          url: 'https://oapi.dingtalk.com/robot/send?access_token=replace_me',
          webhook_kind: 'dingtalk',
          body_mode: 'dingtalk_markdown',
          content_type: 'application/json',
          request_timeout_ms: 4000,
          title_template: 'Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}',
          body_template:
            '### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 场景：{{payload.scene}}\n- 温度：{{payload.temperature_c}} °C / {{payload.temperature_f}} °F\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 时间：{{timestamp}}',
          at_mobiles: [],
          at_all: false,
          headers: {
            'X-Alarm-Source': 'nazh',
          },
        },
        meta: {
          position: { x: 1288, y: -8 },
        },
      },
      debug_console: {
        type: 'debugConsole',
        ai_description: 'Mirror the final branch payload into the desktop debug console.',
        timeout_ms: 500,
        config: {
          label: 'final-output',
          pretty: true,
        },
        meta: {
          position: { x: 1608, y: 88 },
        },
      },
    },
    edges: [
      { from: 'timer_trigger', to: 'modbus_read' },
      { from: 'modbus_read', to: 'code_clean' },
      { from: 'code_clean', to: 'route_switch' },
      { from: 'route_switch', to: 'sql_writer', source_port_id: 'nominal' },
      { from: 'route_switch', to: 'http_alarm', source_port_id: 'alert' },
      { from: 'sql_writer', to: 'debug_console' },
      { from: 'http_alarm', to: 'debug_console' },
    ],
  };
}

/** 根据看板 ID 和名称构建对应的 AST 文本。 */
export function buildProjectAst(boardId: string, boardName: string): string {
  if (boardId === 'default') {
    return JSON.stringify(buildIndustrialAlarmExample(boardName), null, 2);
  }

  const base = JSON.parse(SAMPLE_AST) as {
    name?: string;
    nodes?: Record<string, { config?: Record<string, JsonValue> }>;
  };

  base.name = boardName;

  if (base.nodes?.ingress?.config) {
    base.nodes.ingress.config.message = `${boardName} 已接收边缘输入`;
  }

  return JSON.stringify(base, null, 2);
}

/** 为看板库中所有看板构建初始工程草稿映射。 */
export function buildInitialProjectDrafts(): Record<string, ProjectDraft> {
  return BOARD_LIBRARY.reduce<Record<string, ProjectDraft>>((drafts, board) => {
    drafts[board.id] = {
      astText: buildProjectAst(board.id, board.name),
      payloadText:
        board.id === 'default'
          ? JSON.stringify(
              {
                manual: true,
                operator: CURRENT_USER_NAME,
                reason: 'manual override',
              },
              null,
              2,
            )
          : SAMPLE_PAYLOAD,
    };
    return drafts;
  }, {});
}
