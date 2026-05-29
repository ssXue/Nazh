/// RFC-0006 Phase 5：结构化抽取管道的 Zod Schema 定义。
///
/// Stage 1 大纲抽取 + Stage 2 信号填充。

import { z } from 'zod';

/// Stage 1：设备大纲。
export const DeviceOutlineSchema = z.object({
  id: z.string().describe('设备唯一标识，英文小写下划线格式'),
  type: z.string().describe('设备类型，如 temperature_sensor、hydraulic_press'),
  manufacturer: z.string().optional().describe('厂商名称'),
  model: z.string().optional().describe('设备型号'),
  protocol: z.enum([
    'modbus-tcp', 'modbus-rtu', 'mqtt', 'serial', 'can', 'can-fd', 'ethercat',
  ]).optional().describe('通信协议'),
  signalCount: z.number().describe('从文档中识别到的信号数量'),
  signalSummaries: z.array(z.object({
    id: z.string(),
    signalType: z.enum(['analog_input', 'analog_output', 'digital_input', 'digital_output']),
    description: z.string().describe('信号功能简述'),
  })).describe('信号概要列表（不含寄存器地址等细节）'),
  connection: z.object({
    type: z.string(),
    id: z.string().optional(),
    unit: z.number().optional(),
  }).optional(),
  uncertainties: z.array(z.object({
    fieldPath: z.string(),
    guessedValue: z.string(),
    reason: z.string(),
  })).optional().describe('信息不完整字段的推测'),
  warnings: z.array(z.string()).optional().describe('潜在安全问题'),
});

export type DeviceOutline = z.infer<typeof DeviceOutlineSchema>;

/// Stage 2：信号细节填充。
export const SignalDetailSchema = z.object({
  signals: z.array(z.union([
    z.object({
      id: z.string(),
      signalType: z.enum(['analog_input', 'analog_output']),
      unit: z.string().optional(),
      range: z.tuple([z.number(), z.number()]).optional(),
      source: z.discriminatedUnion('type', [
        z.object({ type: z.literal('register'), register: z.number(), access: z.enum(['read', 'write', 'read_write']).default('read'), data_type: z.enum(['bool', 'u16', 'i16', 'u32', 'i32', 'float32', 'float64', 'string']), bit: z.number().optional() }),
        z.object({ type: z.literal('topic'), topic: z.string() }),
        z.object({ type: z.literal('serial_command'), command: z.string() }),
        z.object({ type: z.literal('can_frame'), can_id: z.number(), is_extended: z.boolean().default(false), byte_offset: z.number(), byte_length: z.number(), data_type: z.enum(['bool', 'u16', 'i16', 'u32', 'i32', 'float32', 'float64', 'string']), byte_order: z.enum(['big_endian', 'little_endian']).default('big_endian') }),
        z.object({ type: z.literal('ethercat_pdo'), pdo_index: z.number(), entry_index: z.number(), sub_index: z.number(), bit_len: z.number(), slave_address: z.number().optional() }),
      ]),
      scale: z.string().optional().describe('Rhai 缩放表达式'),
      confidence: z.enum(['high', 'medium', 'low']),
    }),
    z.object({
      id: z.string(),
      signalType: z.enum(['digital_input', 'digital_output']),
      source: z.discriminatedUnion('type', [
        z.object({ type: z.literal('register'), register: z.number(), access: z.enum(['read', 'write', 'read_write']).default('read'), data_type: z.enum(['bool', 'u16', 'i16', 'u32', 'i32', 'float32', 'float64', 'string']), bit: z.number().optional() }),
        z.object({ type: z.literal('topic'), topic: z.string() }),
        z.object({ type: z.literal('serial_command'), command: z.string() }),
        z.object({ type: z.literal('can_frame'), can_id: z.number(), is_extended: z.boolean().default(false), byte_offset: z.number(), byte_length: z.number(), data_type: z.enum(['bool', 'u16', 'i16', 'u32', 'i32', 'float32', 'float64', 'string']), byte_order: z.enum(['big_endian', 'little_endian']).default('big_endian') }),
        z.object({ type: z.literal('ethercat_pdo'), pdo_index: z.number(), entry_index: z.number(), sub_index: z.number(), bit_len: z.number(), slave_address: z.number().optional() }),
      ]),
      confidence: z.enum(['high', 'medium', 'low']),
    }),
  ])),
  alarms: z.array(z.object({
    id: z.string(),
    condition: z.string(),
    severity: z.enum(['info', 'warning', 'critical']),
    action: z.string().optional(),
  })).optional(),
});

export type SignalDetail = z.infer<typeof SignalDetailSchema>;
