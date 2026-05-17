//! 信号解码共享类型。

use serde::{Deserialize, Serialize};

/// 信号数据类型快照——镜像 `dsl-core::DataType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeSnapshot {
    Bool,
    U16,
    I16,
    U32,
    I32,
    Float32,
    Float64,
    String,
}

impl DataTypeSnapshot {
    /// Modbus 寄存器数量（每寄存器 2 字节）。
    pub fn modbus_register_count(self) -> u16 {
        match self {
            Self::U32 | Self::I32 | Self::Float32 => 2,
            Self::Float64 => 4,
            Self::Bool | Self::U16 | Self::I16 | Self::String => 1,
        }
    }

    /// 所需最小字节数。
    pub fn byte_count(self) -> usize {
        match self {
            Self::U32 | Self::I32 | Self::Float32 => 4,
            Self::Float64 => 8,
            Self::Bool | Self::U16 | Self::I16 => 2,
            Self::String => 0,
        }
    }
}

/// 字节序快照——镜像 `dsl-core::ByteOrder`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrderSnapshot {
    #[default]
    BigEndian,
    LittleEndian,
}

/// 信号源快照——编译期从 `SignalSpec.source` 复制。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalSourceSnapshot {
    Register {
        register: u16,
        data_type: DataTypeSnapshot,
        #[serde(default)]
        bit: Option<u8>,
    },
    CanFrame {
        can_id: u32,
        #[serde(default)]
        is_extended: bool,
        byte_offset: u8,
        byte_length: u8,
        data_type: DataTypeSnapshot,
        #[serde(default)]
        byte_order: ByteOrderSnapshot,
    },
    Topic {
        topic: String,
    },
    SerialCommand {
        command: String,
    },
    EthercatPdo {
        #[serde(default)]
        slave_address: Option<u16>,
        pdo_index: u16,
        entry_index: u16,
        sub_index: u8,
        bit_len: u16,
    },
}

impl SignalSourceSnapshot {
    /// 返回 serde tag 值（如 "register"、"topic"）。
    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::Register { .. } => "register",
            Self::CanFrame { .. } => "can_frame",
            Self::Topic { .. } => "topic",
            Self::SerialCommand { .. } => "serial_command",
            Self::EthercatPdo { .. } => "ethercat_pdo",
        }
    }
}

// ---- 错误类型 ----

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("buffer too short: need {needed} bytes, got {actual}")]
    BufferTooShort { needed: usize, actual: usize },
    #[error("unsupported data type for bit extraction: {0:?}")]
    BitExtractionUnsupported(DataTypeSnapshot),
    #[error("bit index {index} out of range for {width}-bit value")]
    BitOutOfRange { index: u8, width: u8 },
    #[error("invalid UTF-8 in string decode: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ScaleError {
    #[error("Rhai scale expression compile failed: {0}")]
    CompileFailed(String),
    #[error("Rhai scale expression evaluation failed: {0}")]
    EvalFailed(String),
    #[error("scale expression returned non-numeric type")]
    NonNumericResult,
}
