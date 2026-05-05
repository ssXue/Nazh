//! Device DSL 类型定义（RFC-0004 §7.1）。
//!
//! 描述设备实体、信号、协议连接和数据转换。

use serde::{Deserialize, Serialize};

use crate::workflow::Range;

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
    pub connection: ConnectionRef,
    #[serde(default)]
    pub signals: Vec<SignalSpec>,
    #[serde(default)]
    pub alarms: Vec<AlarmSpec>,
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
        assert_eq!(spec.connection.connection_type, "modbus-tcp");
        assert_eq!(spec.connection.id, "press_modbus");
        assert_eq!(spec.connection.unit, Some(1));
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
connection:
  type: mqtt
  id: broker_local
"#;
        let spec: DeviceSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.id, "sensor_1");
        assert!(spec.signals.is_empty());
        assert!(spec.alarms.is_empty());
        assert!(spec.manufacturer.is_none());
        assert!(spec.model.is_none());
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
            ..
        } = &signal.source
        {
            assert_eq!(*pdo_index, 0x1A00);
            assert_eq!(*entry_index, 0x6041);
            assert_eq!(*sub_index, 1);
            assert_eq!(*bit_len, 16);
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
    fn 缺少_connection_解析失败() {
        let yaml = r#"
id: test_device
type: test
"#;
        assert!(serde_yaml::from_str::<DeviceSpec>(yaml).is_err());
    }
}
