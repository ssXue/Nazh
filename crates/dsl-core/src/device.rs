//! Device DSL 类型定义（RFC-0004 §7.1）。
//!
//! 描述设备实体、信号、协议连接和数据转换。

use serde::{Deserialize, Serialize};

use crate::workflow::Range;

/// 校验诊断级别（RFC-0006 Phase 1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationLevel {
    Error,
    Warning,
}

/// 单条校验诊断。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationDiagnostic {
    pub level: ValidationLevel,
    pub path: String,
    pub message: String,
}

/// 设备校验结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub diagnostics: Vec<ValidationDiagnostic>,
}

impl ValidationResult {
    /// 是否无 error 级诊断。
    pub fn is_valid(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|d| d.level != ValidationLevel::Error)
    }
}

fn error(path: impl Into<String>, message: impl Into<String>) -> ValidationDiagnostic {
    ValidationDiagnostic {
        level: ValidationLevel::Error,
        path: path.into(),
        message: message.into(),
    }
}

fn warning(path: impl Into<String>, message: impl Into<String>) -> ValidationDiagnostic {
    ValidationDiagnostic {
        level: ValidationLevel::Warning,
        path: path.into(),
        message: message.into(),
    }
}

/// 设备 DSL 结构化模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSpec {
    pub id: String,
    #[serde(rename = "type")]
    pub device_type: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionRef>,
    /// 设备所属网络组（如 ENI 导入时同一网络的所有从站共享此 ID）。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_group: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ethercat_identity: Option<EthercatIdentity>,
    #[serde(default)]
    pub signals: Vec<SignalSpec>,
    #[serde(default)]
    pub alarms: Vec<AlarmSpec>,
}

/// `EtherCAT` 设备标识信息（ESI/ENI 导入时填充）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EthercatIdentity {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_code: Option<u32>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_no: Option<u32>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slave_address: Option<u16>,
}

/// 对 `ConnectionManager` 中连接的引用。
///
/// `connection_type` 匹配 `ConnectionManager` 的协议名称
/// （例如 "modbus-tcp"、"mqtt"、"serial"）。
/// `id` 引用 `ConnectionDefinition` 的 `id` 字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionRef {
    #[serde(rename = "type")]
    pub connection_type: String,
    pub id: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<u8>,
}

/// 信号方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    AnalogInput,
    AnalogOutput,
    DigitalInput,
    DigitalOutput,
}

/// 信号数据来源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalSource {
    /// Modbus 寄存器。
    Register {
        register: u16,
        #[serde(default)]
        access: AccessMode,
        data_type: DataType,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        bit: Option<u8>,
    },
    /// MQTT 主题订阅。
    Topic { topic: String },
    /// 串口命令。
    SerialCommand { command: String },
    /// CAN 帧信号解码。
    CanFrame {
        can_id: u32,
        #[serde(default)]
        is_extended: bool,
        byte_offset: u8,
        byte_length: u8,
        data_type: DataType,
        #[serde(default)]
        byte_order: ByteOrder,
    },
    /// `EtherCAT` PDO 条目。
    EthercatPdo {
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        slave_address: Option<u16>,
        pdo_index: u16,
        entry_index: u16,
        sub_index: u8,
        bit_len: u16,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        data_type: Option<String>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        pdo_name: Option<String>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        entry_name: Option<String>,
    },
}

/// 寄存器访问模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    #[default]
    Read,
    Write,
    ReadWrite,
}

/// 字节序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByteOrder {
    #[default]
    BigEndian,
    LittleEndian,
}

/// Modbus 寄存器数据类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Bool,
    U16,
    I16,
    U32,
    I32,
    Float32,
    Float64,
    String,
}

/// 信号定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalSpec {
    pub id: String,
    pub signal_type: SignalType,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    pub source: SignalSource,
    /// 可选缩放表达式（Rhai 表达式，如 `"raw * 35 / 65535"`）。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
}

/// 告警严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlarmSeverity {
    Info,
    Warning,
    Critical,
}

/// 告警定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmSpec {
    pub id: String,
    /// Rhai 条件表达式（如 `"pressure > 34"`）。
    pub condition: String,
    pub severity: AlarmSeverity,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl DeviceSpec {
    /// 校验设备定义的语义完整性（RFC-0006 Phase 1）。
    ///
    /// 覆盖：信号 ID 唯一性 + 格式、量程合法性、协议一致性、
    /// scale 表达式语法、告警 ID 唯一性 + condition 语法、写信号配套告警建议。
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> ValidationResult {
        let mut diagnostics = Vec::new();

        // 信号校验
        let mut signal_ids = std::collections::HashSet::new();
        let has_write_signal = self.signals.iter().any(|s| {
            matches!(
                s.signal_type,
                SignalType::AnalogOutput | SignalType::DigitalOutput
            )
        });

        for (i, signal) in self.signals.iter().enumerate() {
            let path_prefix = format!("signals[{i}]");

            // 信号 ID 非空
            if signal.id.trim().is_empty() {
                diagnostics.push(error(format!("{path_prefix}.id"), "信号 ID 不能为空"));
            } else {
                // 信号 ID 格式：^[a-zA-Z_][a-zA-Z0-9_]*$
                let valid_id = signal.id.chars().enumerate().all(|(pos, c)| {
                    if pos == 0 {
                        c.is_ascii_alphabetic() || c == '_'
                    } else {
                        c.is_ascii_alphanumeric() || c == '_'
                    }
                });
                if !valid_id {
                    diagnostics.push(error(
                        format!("{path_prefix}.id"),
                        format!(
                            "信号 ID `{}` 不符合格式要求（须匹配 ^[a-zA-Z_][a-zA-Z0-9_]*$）",
                            signal.id
                        ),
                    ));
                }

                // 信号 ID 唯一性
                if !signal_ids.insert(signal.id.clone()) {
                    diagnostics.push(error(
                        format!("{path_prefix}.id"),
                        format!("重复信号 ID `{}`", signal.id),
                    ));
                }
            }

            // 量程校验
            if let Some(range) = &signal.range
                && range.min >= range.max
            {
                diagnostics.push(error(
                    format!("{path_prefix}.range"),
                    format!("量程 min({}) 必须小于 max({})", range.min, range.max),
                ));
            }

            // 模拟信号应有 unit 和 range
            if matches!(
                signal.signal_type,
                SignalType::AnalogInput | SignalType::AnalogOutput
            ) {
                if signal.unit.is_none() {
                    diagnostics.push(warning(
                        format!("{path_prefix}.unit"),
                        format!("模拟信号 `{}` 建议声明 unit", signal.id),
                    ));
                }
                if signal.range.is_none() {
                    diagnostics.push(warning(
                        format!("{path_prefix}.range"),
                        format!("模拟信号 `{}` 建议声明 range", signal.id),
                    ));
                }
            }

            // source.type 与 connection.type 协议一致性
            if let Some(conn) = &self.connection {
                let expected = match &signal.source {
                    SignalSource::Register { .. } => Some("modbus-tcp"),
                    SignalSource::Topic { .. } => Some("mqtt"),
                    SignalSource::SerialCommand { .. } => Some("serial"),
                    SignalSource::CanFrame { .. } => Some("can"),
                    SignalSource::EthercatPdo { .. } => Some("ethercat"),
                };
                if let Some(expected_protocol) = expected
                    && conn.connection_type != expected_protocol
                    && !(conn.connection_type == "modbus-rtu"
                        && matches!(signal.source, SignalSource::Register { .. }))
                {
                    diagnostics.push(warning(
                        format!("{path_prefix}.source"),
                        format!(
                            "信号 source 类型为 `{}`，但设备连接类型为 `{}`",
                            expected_protocol, conn.connection_type
                        ),
                    ));
                }
            }

            // scale 表达式括号匹配
            if let Some(scale) = &signal.scale {
                let mut depth = 0i32;
                for ch in scale.chars() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth < 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if depth != 0 {
                    diagnostics.push(error(
                        format!("{path_prefix}.scale"),
                        format!("scale 表达式括号不匹配: `{scale}`"),
                    ));
                }
                // scale 不含除法（除零风险）
                if scale.contains('/') {
                    diagnostics.push(warning(
                        format!("{path_prefix}.scale"),
                        format!("scale 表达式含除法运算，存在除零风险: `{scale}`"),
                    ));
                }
            }
        }

        // 写信号建议配套 alarm
        if has_write_signal && self.alarms.is_empty() {
            diagnostics.push(warning(
                "alarms",
                "设备包含写信号但未声明任何告警，建议为写操作配套安全告警",
            ));
        }

        // 告警校验
        let mut alarm_ids = std::collections::HashSet::new();
        for (i, alarm) in self.alarms.iter().enumerate() {
            let path_prefix = format!("alarms[{i}]");

            // 告警 ID 唯一性
            if alarm.id.trim().is_empty() {
                diagnostics.push(error(format!("{path_prefix}.id"), "告警 ID 不能为空"));
            } else if !alarm_ids.insert(alarm.id.clone()) {
                diagnostics.push(error(
                    format!("{path_prefix}.id"),
                    format!("重复告警 ID `{}`", alarm.id),
                ));
            }

            // 告警 condition 非空 + 括号匹配
            let cond = alarm.condition.trim();
            if cond.is_empty() {
                diagnostics.push(error(
                    format!("{path_prefix}.condition"),
                    "告警条件不能为空",
                ));
            } else {
                let mut depth = 0i32;
                for ch in cond.chars() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth < 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if depth != 0 {
                    diagnostics.push(error(
                        format!("{path_prefix}.condition"),
                        format!("告警条件括号不匹配: `{}`", alarm.condition),
                    ));
                }
            }
        }

        ValidationResult { diagnostics }
    }
}
/// 指定协议的信号源字段模板，供 AI 生成信号时参考（RFC-0006 Phase 2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSchemaTemplate {
    pub protocol: String,
    pub source_variants: Vec<SourceVariant>,
    pub signal_types: Vec<String>,
    pub data_types: Vec<String>,
    pub example_signal_yaml: String,
}

/// 信号源变体描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceVariant {
    #[serde(rename = "type")]
    pub source_type: String,
    pub description: String,
    pub fields: Vec<FieldSpec>,
}

/// 字段规格。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// 返回指定协议的信号源字段模板。
///
/// 协议名接受 `modbus-tcp` / `modbus-rtu` / `mqtt` / `serial` / `can` / `can-fd` / `ethercat`。
pub fn signal_schema_template(protocol: &str) -> Result<SignalSchemaTemplate, String> {
    match protocol {
        "modbus-tcp" | "modbus-rtu" => Ok(SignalSchemaTemplate {
            protocol: protocol.to_owned(),
            source_variants: vec![SourceVariant {
                source_type: "register".to_owned(),
                description: "Modbus 寄存器地址".to_owned(),
                fields: vec![
                    FieldSpec { name: "register".into(), field_type: "u16".into(), required: true, description: "寄存器地址（如 40001）".into(), enum_values: None, default: None },
                    FieldSpec { name: "access".into(), field_type: "string".into(), required: false, description: "访问模式".into(), enum_values: Some(vec!["read".into(), "write".into(), "read_write".into()]), default: Some("read".into()) },
                    FieldSpec { name: "data_type".into(), field_type: "string".into(), required: true, description: "数据类型".into(), enum_values: Some(vec!["bool".into(), "u16".into(), "i16".into(), "u32".into(), "i32".into(), "float32".into(), "float64".into(), "string".into()]), default: None },
                    FieldSpec { name: "bit".into(), field_type: "u8".into(), required: false, description: "位偏移（仅 bool 类型需要）".into(), enum_values: None, default: None },
                ],
            }],
            signal_types: vec!["analog_input".into(), "analog_output".into(), "digital_input".into(), "digital_output".into()],
            data_types: vec!["bool".into(), "u16".into(), "i16".into(), "u32".into(), "i32".into(), "float32".into(), "float64".into(), "string".into()],
            example_signal_yaml: "id: temperature\nsignal_type: analog_input\nunit: C\nrange: [0, 100]\nsource:\n  type: register\n  register: 40001\n  access: read\n  data_type: float32".into(),
        }),
        "mqtt" => Ok(SignalSchemaTemplate {
            protocol: "mqtt".to_owned(),
            source_variants: vec![SourceVariant {
                source_type: "topic".to_owned(),
                description: "MQTT 主题订阅".to_owned(),
                fields: vec![
                    FieldSpec { name: "topic".into(), field_type: "string".into(), required: true, description: "MQTT 主题路径（如 factory/press/pressure）".into(), enum_values: None, default: None },
                ],
            }],
            signal_types: vec!["analog_input".into(), "analog_output".into(), "digital_input".into(), "digital_output".into()],
            data_types: vec![],
            example_signal_yaml: "id: pressure\nsignal_type: analog_input\nunit: MPa\nsource:\n  type: topic\n  topic: factory/press/pressure".into(),
        }),
        "serial" => Ok(SignalSchemaTemplate {
            protocol: "serial".to_owned(),
            source_variants: vec![SourceVariant {
                source_type: "serial_command".to_owned(),
                description: "串口命令".to_owned(),
                fields: vec![
                    FieldSpec { name: "command".into(), field_type: "string".into(), required: true, description: "串口命令字符串（如 READ_TEMP）".into(), enum_values: None, default: None },
                ],
            }],
            signal_types: vec!["analog_input".into(), "analog_output".into(), "digital_input".into(), "digital_output".into()],
            data_types: vec![],
            example_signal_yaml: "id: temperature\nsignal_type: analog_input\nunit: C\nsource:\n  type: serial_command\n  command: READ_TEMP".into(),
        }),
        "can" | "can-fd" => Ok(SignalSchemaTemplate {
            protocol: protocol.to_owned(),
            source_variants: vec![SourceVariant {
                source_type: "can_frame".to_owned(),
                description: "CAN 帧信号解码".to_owned(),
                fields: vec![
                    FieldSpec { name: "can_id".into(), field_type: "u32".into(), required: true, description: "CAN 帧标识符".into(), enum_values: None, default: None },
                    FieldSpec { name: "is_extended".into(), field_type: "bool".into(), required: false, description: "是否扩展帧".into(), enum_values: None, default: Some("false".into()) },
                    FieldSpec { name: "byte_offset".into(), field_type: "u8".into(), required: true, description: "数据字节偏移".into(), enum_values: None, default: None },
                    FieldSpec { name: "byte_length".into(), field_type: "u8".into(), required: true, description: "数据字节长度".into(), enum_values: None, default: None },
                    FieldSpec { name: "data_type".into(), field_type: "string".into(), required: true, description: "数据类型".into(), enum_values: Some(vec!["bool".into(), "u16".into(), "i16".into(), "u32".into(), "i32".into(), "float32".into(), "float64".into(), "string".into()]), default: None },
                    FieldSpec { name: "byte_order".into(), field_type: "string".into(), required: false, description: "字节序".into(), enum_values: Some(vec!["big_endian".into(), "little_endian".into()]), default: Some("big_endian".into()) },
                ],
            }],
            signal_types: vec!["analog_input".into(), "analog_output".into(), "digital_input".into(), "digital_output".into()],
            data_types: vec!["bool".into(), "u16".into(), "i16".into(), "u32".into(), "i32".into(), "float32".into(), "float64".into(), "string".into()],
            example_signal_yaml: "id: motor_speed\nsignal_type: analog_input\nunit: RPM\nrange: [0, 3000]\nsource:\n  type: can_frame\n  can_id: 256\n  byte_offset: 0\n  byte_length: 2\n  data_type: u16\n  byte_order: big_endian".into(),
        }),
        "ethercat" => Ok(SignalSchemaTemplate {
            protocol: "ethercat".to_owned(),
            source_variants: vec![SourceVariant {
                source_type: "ethercat_pdo".to_owned(),
                description: "EtherCAT PDO 条目".to_owned(),
                fields: vec![
                    FieldSpec { name: "slave_address".into(), field_type: "u16".into(), required: false, description: "从站地址（多从站拓扑时必填）".into(), enum_values: None, default: None },
                    FieldSpec { name: "pdo_index".into(), field_type: "u16".into(), required: true, description: "PDO 索引（如 0x1A00）".into(), enum_values: None, default: None },
                    FieldSpec { name: "entry_index".into(), field_type: "u16".into(), required: true, description: "PDO 条目索引".into(), enum_values: None, default: None },
                    FieldSpec { name: "sub_index".into(), field_type: "u8".into(), required: true, description: "子索引".into(), enum_values: None, default: None },
                    FieldSpec { name: "bit_len".into(), field_type: "u16".into(), required: true, description: "位长度".into(), enum_values: None, default: None },
                    FieldSpec { name: "data_type".into(), field_type: "string".into(), required: false, description: "数据类型名称（如 UINT16）".into(), enum_values: None, default: None },
                ],
            }],
            signal_types: vec!["analog_input".into(), "analog_output".into(), "digital_input".into(), "digital_output".into()],
            data_types: vec![],
            example_signal_yaml: "id: status_word\nsignal_type: analog_input\nsource:\n  type: ethercat_pdo\n  slave_address: 1001\n  pdo_index: 26880\n  entry_index: 24641\n  sub_index: 1\n  bit_len: 16\n  data_type: UINT16".into(),
        }),
        _ => Err(format!("不支持的协议: {protocol}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;

    #[test]
    fn 完整的_device_spec_从_yaml_解析成功() {
        let yaml = r#"
id: hydraulic_press_1
type: hydraulic_press
manufacturer: "某液压"
model: YP-320T
connection:
  type: modbus-tcp
  id: press_modbus
  unit: 1
signals:
  - id: pressure
    signal_type: analog_input
    unit: MPa
    range: [0, 35]
    source:
      type: register
      register: 40001
      access: read
      data_type: float32
    scale: "raw * 35 / 65535"
  - id: position
    signal_type: analog_input
    unit: mm
    range: [0, 150]
    source:
      type: register
      register: 40003
      access: read
      data_type: float32
  - id: servo_ready
    signal_type: digital_input
    source:
      type: register
      register: 40100
      access: read
      data_type: u16
      bit: 0
  - id: target_position
    signal_type: analog_output
    unit: mm
    range: [0, 150]
    source:
      type: register
      register: 40010
      access: write
      data_type: float32
alarms:
  - id: over_pressure
    condition: "pressure > 34"
    severity: critical
    action: emergency_stop
"#;
        let spec: DeviceSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.id, "hydraulic_press_1");
        assert_eq!(spec.device_type, "hydraulic_press");
        assert_eq!(spec.manufacturer, Some("某液压".to_owned()));
        assert_eq!(spec.model, Some("YP-320T".to_owned()));
        assert_eq!(
            spec.connection.as_ref().unwrap().connection_type,
            "modbus-tcp"
        );
        assert_eq!(spec.connection.as_ref().unwrap().id, "press_modbus");
        assert_eq!(spec.connection.as_ref().unwrap().unit, Some(1));
        assert_eq!(spec.signals.len(), 4);

        // pressure signal
        let pressure = &spec.signals[0];
        assert_eq!(pressure.id, "pressure");
        assert_eq!(pressure.signal_type, SignalType::AnalogInput);
        assert_eq!(pressure.unit, Some("MPa".to_owned()));
        assert_eq!(pressure.range.map(|r| r.max), Some(35.0));
        assert_eq!(pressure.scale, Some("raw * 35 / 65535".to_owned()));

        // servo_ready (digital, with bit field)
        let servo = &spec.signals[2];
        assert_eq!(servo.signal_type, SignalType::DigitalInput);
        if let SignalSource::Register { bit, .. } = &servo.source {
            assert_eq!(*bit, Some(0));
        } else {
            panic!("servo_ready source 应为 Register");
        }

        // alarms
        assert_eq!(spec.alarms.len(), 1);
        assert_eq!(spec.alarms[0].severity, AlarmSeverity::Critical);
    }

    #[test]
    fn 最小_device_spec_解析成功() {
        let yaml = r#"
id: sensor_1
type: temperature_sensor
"#;
        let spec: DeviceSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.id, "sensor_1");
        assert!(spec.signals.is_empty());
        assert!(spec.alarms.is_empty());
        assert!(spec.manufacturer.is_none());
        assert!(spec.model.is_none());
        assert!(spec.connection.is_none());
    }

    #[test]
    fn signal_type_四种变体序列化() {
        let types = [
            SignalType::AnalogInput,
            SignalType::AnalogOutput,
            SignalType::DigitalInput,
            SignalType::DigitalOutput,
        ];
        for st in &types {
            let yaml = serde_yaml::to_string(st).unwrap();
            let back: SignalType = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(*st, back);
        }
    }

    #[test]
    fn signal_source_topic_解析() {
        let yaml = r#"
id: mqtt_signal
signal_type: analog_input
source:
  type: topic
  topic: "factory/press/pressure"
"#;
        let signal: SignalSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(signal.source, SignalSource::Topic { .. }));
        if let SignalSource::Topic { topic } = &signal.source {
            assert_eq!(topic, "factory/press/pressure");
        }
    }

    #[test]
    fn signal_source_serial_command_解析() {
        let yaml = r#"
id: serial_cmd
signal_type: analog_output
source:
  type: serial_command
  command: "READ_TEMP"
"#;
        let signal: SignalSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(signal.source, SignalSource::SerialCommand { .. }));
    }

    #[test]
    fn signal_source_ethercat_pdo_解析() {
        let yaml = r#"
id: status_word
signal_type: analog_input
source:
  type: ethercat_pdo
  slave_address: 1002
  pdo_index: 6656
  entry_index: 24641
  sub_index: 1
  bit_len: 16
  data_type: UINT16
  pdo_name: TxPDO
  entry_name: Status word
"#;
        let signal: SignalSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(signal.source, SignalSource::EthercatPdo { .. }));
        if let SignalSource::EthercatPdo {
            pdo_index,
            entry_index,
            sub_index,
            bit_len,
            data_type,
            slave_address,
            ..
        } = &signal.source
        {
            assert_eq!(*pdo_index, 0x1A00);
            assert_eq!(*entry_index, 0x6041);
            assert_eq!(*sub_index, 1);
            assert_eq!(*bit_len, 16);
            assert_eq!(*slave_address, Some(1002));
            assert_eq!(data_type.as_deref(), Some("UINT16"));
        }
    }

    #[test]
    fn signal_source_register_不含_bit_字段() {
        let yaml = r#"
id: temp
signal_type: analog_input
source:
  type: register
  register: 40001
  data_type: float32
"#;
        let signal: SignalSpec = serde_yaml::from_str(yaml).unwrap();
        if let SignalSource::Register { bit, access, .. } = &signal.source {
            assert_eq!(*bit, None);
            assert_eq!(*access, AccessMode::Read); // default
        } else {
            panic!("source 应为 Register");
        }
    }

    #[test]
    fn alarm_severity_三种变体() {
        for (yaml_str, expected) in [
            ("info", AlarmSeverity::Info),
            ("warning", AlarmSeverity::Warning),
            ("critical", AlarmSeverity::Critical),
        ] {
            let sev: AlarmSeverity = serde_yaml::from_str(yaml_str).unwrap();
            assert_eq!(sev, expected);
        }
    }

    #[test]
    fn device_spec_yaml_round_trip() {
        let yaml = r#"
id: test_device
type: test
connection:
  type: modbus-tcp
  id: conn1
"#;
        let spec: DeviceSpec = serde_yaml::from_str(yaml).unwrap();
        let re_yaml = serde_yaml::to_string(&spec).unwrap();
        let back: DeviceSpec = serde_yaml::from_str(&re_yaml).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn 缺少必填字段_id_解析失败() {
        let yaml = r#"
type: test
connection:
  type: mqtt
  id: conn1
"#;
        assert!(serde_yaml::from_str::<DeviceSpec>(yaml).is_err());
    }

    #[test]
    fn 缺少_connection_解析成功() {
        let yaml = r#"
id: test_device
type: test
"#;
        let spec: DeviceSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.id, "test_device");
        assert!(spec.connection.is_none());
    }

    // ── validate() 校验规则测试 ──

    fn make_valid_device() -> DeviceSpec {
        DeviceSpec {
            id: "test_dev".to_owned(),
            device_type: "sensor".to_owned(),
            manufacturer: None,
            model: None,
            connection: Some(ConnectionRef {
                connection_type: "modbus-tcp".to_owned(),
                id: "conn1".to_owned(),
                unit: None,
            }),
            network_group: None,
            ethercat_identity: None,
            signals: vec![SignalSpec {
                id: "temp".to_owned(),
                signal_type: SignalType::AnalogInput,
                unit: Some("℃".to_owned()),
                range: Some(Range {
                    min: -40.0,
                    max: 125.0,
                }),
                source: SignalSource::Register {
                    register: 40001,
                    access: AccessMode::Read,
                    data_type: DataType::Float32,
                    bit: None,
                },
                scale: None,
            }],
            alarms: vec![],
        }
    }

    #[test]
    fn validate_合法设备_无诊断() {
        let spec = make_valid_device();
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn validate_信号id重复_error() {
        let mut spec = make_valid_device();
        spec.signals.push(spec.signals[0].clone());
        let result = spec.validate();
        assert!(!result.is_valid());
        let errors: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.level == ValidationLevel::Error)
            .collect();
        assert!(errors.iter().any(|d| d.message.contains("重复信号 ID")));
    }

    #[test]
    fn validate_信号id非法字符_error() {
        let mut spec = make_valid_device();
        spec.signals[0].id = "1temp".to_owned();
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("不符合格式要求"))
        );
    }

    #[test]
    fn validate_信号id空_error() {
        let mut spec = make_valid_device();
        spec.signals[0].id = String::new();
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("不能为空"))
        );
    }

    #[test]
    fn validate_量程反转_error() {
        let mut spec = make_valid_device();
        spec.signals[0].range = Some(Range {
            min: 100.0,
            max: 50.0,
        });
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.path.contains("range") && d.message.contains("必须小于"))
        );
    }

    #[test]
    fn validate_模拟信号缺unit_warning() {
        let mut spec = make_valid_device();
        spec.signals[0].unit = None;
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("建议声明 unit"))
        );
    }

    #[test]
    fn validate_模拟信号缺range_warning() {
        let mut spec = make_valid_device();
        spec.signals[0].range = None;
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("建议声明 range"))
        );
    }

    #[test]
    fn validate_协议不一致_warning() {
        let mut spec = make_valid_device();
        spec.connection = Some(ConnectionRef {
            connection_type: "mqtt".to_owned(),
            id: "broker".to_owned(),
            unit: None,
        });
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("信号 source 类型为") && d.message.contains("mqtt"))
        );
    }

    #[test]
    fn validate_modbus_rtu与register一致_无warning() {
        let mut spec = make_valid_device();
        spec.connection = Some(ConnectionRef {
            connection_type: "modbus-rtu".to_owned(),
            id: "rtu1".to_owned(),
            unit: None,
        });
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn validate_scale括号不匹配_error() {
        let mut spec = make_valid_device();
        spec.signals[0].scale = Some("(raw + 1".to_owned());
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("scale 表达式括号不匹配"))
        );
    }

    #[test]
    fn validate_scale含除法_warning() {
        let mut spec = make_valid_device();
        spec.signals[0].scale = Some("raw / 100".to_owned());
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("除零风险"))
        );
    }

    #[test]
    fn validate_写信号无alarm_warning() {
        let mut spec = make_valid_device();
        spec.signals.push(SignalSpec {
            id: "setpoint".to_owned(),
            signal_type: SignalType::AnalogOutput,
            unit: Some("℃".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 100.0,
            }),
            source: SignalSource::Register {
                register: 40010,
                access: AccessMode::Write,
                data_type: DataType::Float32,
                bit: None,
            },
            scale: None,
        });
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("写信号但未声明任何告警"))
        );
    }

    #[test]
    fn validate_告警id重复_error() {
        let mut spec = make_valid_device();
        spec.alarms = vec![
            AlarmSpec {
                id: "alarm1".to_owned(),
                condition: "temp > 80".to_owned(),
                severity: AlarmSeverity::Warning,
                action: None,
            },
            AlarmSpec {
                id: "alarm1".to_owned(),
                condition: "temp > 90".to_owned(),
                severity: AlarmSeverity::Critical,
                action: None,
            },
        ];
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("重复告警 ID"))
        );
    }

    #[test]
    fn validate_告警id空_error() {
        let mut spec = make_valid_device();
        spec.alarms = vec![AlarmSpec {
            id: String::new(),
            condition: "temp > 80".to_owned(),
            severity: AlarmSeverity::Warning,
            action: None,
        }];
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.path.contains("alarms") && d.message.contains("不能为空"))
        );
    }

    #[test]
    fn validate_告警条件空_error() {
        let mut spec = make_valid_device();
        spec.alarms = vec![AlarmSpec {
            id: "alarm1".to_owned(),
            condition: "  ".to_owned(),
            severity: AlarmSeverity::Warning,
            action: None,
        }];
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("告警条件不能为空"))
        );
    }

    #[test]
    fn validate_告警条件括号不匹配_error() {
        let mut spec = make_valid_device();
        spec.alarms = vec![AlarmSpec {
            id: "alarm1".to_owned(),
            condition: "(temp > 80".to_owned(),
            severity: AlarmSeverity::Warning,
            action: None,
        }];
        let result = spec.validate();
        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("告警条件括号不匹配"))
        );
    }

    #[test]
    fn validate_数字信号无需unit_range() {
        let mut spec = make_valid_device();
        spec.signals[0] = SignalSpec {
            id: "switch".to_owned(),
            signal_type: SignalType::DigitalInput,
            unit: None,
            range: None,
            source: SignalSource::Register {
                register: 40100,
                access: AccessMode::Read,
                data_type: DataType::Bool,
                bit: Some(0),
            },
            scale: None,
        };
        let result = spec.validate();
        assert!(result.is_valid());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn parse_device_yaml_validated_语义校验失败() {
        let yaml = r#"
id: bad_dev
type: sensor
signals:
  - id: temp
    signal_type: analog_input
    source:
      type: register
      register: 40001
      data_type: float32
    scale: "(broken"
"#;
        let result = super::super::parser::parse_device_yaml_validated(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("语义校验失败"));
    }

    #[test]
    fn parse_device_yaml_validated_通过() {
        let yaml = r#"
id: good_dev
type: sensor
connection:
  type: modbus-tcp
  id: conn1
signals:
  - id: temp
    signal_type: analog_input
    unit: C
    range: [0, 100]
    source:
      type: register
      register: 40001
      data_type: float32
"#;
        let result = super::super::parser::parse_device_yaml_validated(yaml);
        assert!(result.is_ok());
    }

    // ── signal_schema_template 测试 ──

    #[test]
    fn schema_template_modbus_tcp() {
        let t = super::super::signal_schema_template("modbus-tcp").unwrap();
        assert_eq!(t.protocol, "modbus-tcp");
        assert_eq!(t.source_variants.len(), 1);
        assert_eq!(t.source_variants[0].source_type, "register");
        assert!(
            t.source_variants[0]
                .fields
                .iter()
                .any(|f| f.name == "register" && f.required)
        );
    }

    #[test]
    fn schema_template_mqtt() {
        let t = super::super::signal_schema_template("mqtt").unwrap();
        assert_eq!(t.source_variants[0].source_type, "topic");
    }

    #[test]
    fn schema_template_serial() {
        let t = super::super::signal_schema_template("serial").unwrap();
        assert_eq!(t.source_variants[0].source_type, "serial_command");
    }

    #[test]
    fn schema_template_can() {
        let t = super::super::signal_schema_template("can").unwrap();
        assert_eq!(t.source_variants[0].source_type, "can_frame");
        assert!(t.source_variants[0].fields.len() >= 5);
    }

    #[test]
    fn schema_template_ethercat() {
        let t = super::super::signal_schema_template("ethercat").unwrap();
        assert_eq!(t.source_variants[0].source_type, "ethercat_pdo");
    }

    #[test]
    fn schema_template_不支持的协议() {
        assert!(super::super::signal_schema_template("unknown").is_err());
    }
}
