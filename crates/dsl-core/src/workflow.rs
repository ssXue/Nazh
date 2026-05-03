//! Workflow DSL 类型定义（RFC-0004 §7.3）。
//!
//! 描述业务状态机——状态、转移条件、触发动作、异常处理。
//!
//! 同时定义被 device / capability 模块共用的辅助类型：
//! [`Range`]（量程区间）、[`HumanDuration`]（人类可读时长）。

use std::collections::HashMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::error::DslError;

// ---- 共享辅助类型 ----

/// 量程区间 `[min, max]`。
///
/// YAML 表示为双元素数组 `[0, 35]`，内部存储为具名字段。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    /// 校验 `value` 是否在 [`min`](Self::min), [`max`](Self::max) 闭区间内。
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        value >= self.min && value <= self.max
    }
}

impl Serialize for Range {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeTuple;
        let mut tuple = serializer.serialize_tuple(2)?;
        tuple.serialize_element(&self.min)?;
        tuple.serialize_element(&self.max)?;
        tuple.end()
    }
}

impl<'de> Deserialize<'de> for Range {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RangeArr([f64; 2]);

        let arr = RangeArr::deserialize(deserializer)?;
        if arr.0[0] > arr.0[1] {
            return Err(D::Error::custom("range 的 min 不能大于 max"));
        }
        Ok(Self {
            min: arr.0[0],
            max: arr.0[1],
        })
    }
}

/// 人类可读时长（如 "30s"、"5m"、"1h"、"500ms"）。
///
/// YAML 中以字符串形式声明，解析为毫秒数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanDuration {
    pub millis: u64,
}

impl HumanDuration {
    /// 从字符串（如 "30s"）解析时长。
    ///
    /// 支持后缀：`ms`（毫秒）、`s`（秒）、`m`（分钟）、`h`（小时）。
    ///
    /// # Errors
    ///
    /// 无法识别的后缀或负数值时返回 [`DslError::YamlParse`]。
    pub fn parse(s: &str) -> Result<Self, DslError> {
        let s = s.trim();
        // 按长度降序匹配，确保 "ms" 优先于 "s"
        if let Some(num_str) = s.strip_suffix("ms") {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| DslError::YamlParse(format!("无法解析时长数值: {s}")))?;
            return Ok(Self { millis: num });
        }
        if let Some(num_str) = s.strip_suffix('h') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| DslError::YamlParse(format!("无法解析时长数值: {s}")))?;
            return Ok(Self {
                millis: num * 3_600_000,
            });
        }
        if let Some(num_str) = s.strip_suffix('m') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| DslError::YamlParse(format!("无法解析时长数值: {s}")))?;
            return Ok(Self {
                millis: num * 60_000,
            });
        }
        if let Some(num_str) = s.strip_suffix('s') {
            let num: u64 = num_str
                .trim()
                .parse()
                .map_err(|_| DslError::YamlParse(format!("无法解析时长数值: {s}")))?;
            return Ok(Self {
                millis: num * 1_000,
            });
        }
        Err(DslError::YamlParse(format!(
            "无法识别时长后缀，支持 ms/s/m/h: {s}"
        )))
    }
}

impl Serialize for HumanDuration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = if self.millis.is_multiple_of(3_600_000) {
            format!("{}h", self.millis / 3_600_000)
        } else if self.millis.is_multiple_of(60_000) {
            format!("{}m", self.millis / 60_000)
        } else if self.millis.is_multiple_of(1_000) {
            format!("{}s", self.millis / 1_000)
        } else {
            format!("{}ms", self.millis)
        };
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for HumanDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(D::Error::custom)
    }
}

// ---- Workflow 类型 ----

/// 工作流 DSL 定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub devices: Vec<String>,
    #[serde(default)]
    pub variables: HashMap<String, Value>,
    pub states: HashMap<String, StateSpec>,
    #[serde(default)]
    pub transitions: Vec<TransitionSpec>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub timeout: HashMap<String, HumanDuration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_timeout: Option<String>,
}

/// 状态定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSpec {
    #[serde(default)]
    pub entry: Vec<ActionSpec>,
    #[serde(default)]
    pub exit: Vec<ActionSpec>,
}

/// 状态转移规则。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionSpec {
    pub from: String,
    pub to: String,
    /// Rhai 条件表达式。
    pub when: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionSpec>,
}

/// 动作调用。
///
/// YAML 中通过 `capability` 或 `action` 键区分目标类型，
/// 使用 `#[serde(flatten)]` 将判别键内联。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    #[serde(flatten)]
    pub target: ActionTarget,
    #[serde(default)]
    pub args: HashMap<String, Value>,
}

/// 动作目标——调用设备能力或系统动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionTarget {
    Capability(String),
    Action(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    // ---- Range 测试 ----

    #[test]
    fn range_从_yaml_数组解析() {
        let yaml = "[0, 35]";
        let range: Range = serde_yaml::from_str(yaml).unwrap();
        assert!((range.min - 0.0).abs() < f64::EPSILON);
        assert!((range.max - 35.0).abs() < f64::EPSILON);
    }

    #[test]
    fn range_序列化为数组() {
        let range = Range {
            min: 0.0,
            max: 150.0,
        };
        let yaml = serde_yaml::to_string(&range).unwrap();
        assert!(yaml.contains("0.0"));
        assert!(yaml.contains("150.0"));
    }

    #[test]
    fn range_min_大于_max_解析失败() {
        let yaml = "[35, 0]";
        let result = serde_yaml::from_str::<Range>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn range_contains_边界值() {
        let range = Range {
            min: 0.0,
            max: 35.0,
        };
        assert!(range.contains(0.0));
        assert!(range.contains(35.0));
        assert!(range.contains(17.5));
        assert!(!range.contains(-0.1));
        assert!(!range.contains(35.1));
    }

    #[test]
    fn range_round_trip() {
        let range = Range {
            min: 1.5,
            max: 99.9,
        };
        let yaml = serde_yaml::to_string(&range).unwrap();
        let back: Range = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(range, back);
    }

    // ---- HumanDuration 测试 ----

    #[test]
    fn human_duration_解析秒() {
        let d = HumanDuration::parse("30s").unwrap();
        assert_eq!(d.millis, 30_000);
    }

    #[test]
    fn human_duration_解析分钟() {
        let d = HumanDuration::parse("5m").unwrap();
        assert_eq!(d.millis, 300_000);
    }

    #[test]
    fn human_duration_解析小时() {
        let d = HumanDuration::parse("1h").unwrap();
        assert_eq!(d.millis, 3_600_000);
    }

    #[test]
    fn human_duration_解析毫秒() {
        let d = HumanDuration::parse("500ms").unwrap();
        assert_eq!(d.millis, 500);
    }

    #[test]
    fn human_duration_非法后缀() {
        assert!(HumanDuration::parse("30x").is_err());
    }

    #[test]
    fn human_duration_空字符串() {
        assert!(HumanDuration::parse("").is_err());
    }

    #[test]
    fn human_duration_round_trip() {
        let d = HumanDuration { millis: 30_000 };
        let yaml = serde_yaml::to_string(&d).unwrap();
        let back: HumanDuration = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn human_duration_带空格() {
        let d = HumanDuration::parse(" 30s ").unwrap();
        assert_eq!(d.millis, 30_000);
    }

    // ---- ActionTarget 通过 ActionSpec 测试（flatten 上下文） ----

    #[test]
    fn action_spec_capability_目标() {
        let yaml = r#"
capability: hydraulic_axis.move_to
args:
  position: "${approach_position}"
"#;
        let action: ActionSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            action.target,
            ActionTarget::Capability("hydraulic_axis.move_to".to_owned())
        );
        assert_eq!(action.args["position"], "${approach_position}");
    }

    #[test]
    fn action_spec_action_目标() {
        let yaml = r#"
action: alarm.raise
args:
  message: "压装循环异常停机"
"#;
        let action: ActionSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            action.target,
            ActionTarget::Action("alarm.raise".to_owned())
        );
    }

    #[test]
    fn action_spec_无参数() {
        let yaml = "capability: hydraulic_axis.stop";
        let action: ActionSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            action.target,
            ActionTarget::Capability("hydraulic_axis.stop".to_owned())
        );
        assert!(action.args.is_empty());
    }

    // ---- WorkflowSpec 测试 ----

    #[test]
    fn 完整的_workflow_spec_从_yaml_解析成功() {
        let yaml = r#"
id: auto_pressing_cycle
description: "自动压装循环"
version: "1.0.0"
devices:
  - hydraulic_press_1
variables:
  target_pressure: 25.0
  hold_time: 5.0
  approach_position: 100.0
states:
  idle:
    entry: []
    exit: []
  approaching:
    entry:
      - capability: hydraulic_axis.move_to
        args:
          position: "${approach_position}"
  pressing:
    entry:
      - capability: hydraulic_axis.apply_pressure
        args:
          target: "${target_pressure}"
  fault:
    entry:
      - capability: hydraulic_axis.stop
      - action: alarm.raise
        args:
          message: "压装循环异常停机"
transitions:
  - from: idle
    to: approaching
    when: "start_button == true"
  - from: approaching
    to: pressing
    when: "position >= approach_position"
  - from: "*"
    to: fault
    when: "pressure > 34"
    priority: 100
timeout:
  pressing: 60s
  holding: 30s
on_timeout: fault
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.id, "auto_pressing_cycle");
        assert_eq!(spec.devices, vec!["hydraulic_press_1"]);
        assert_eq!(spec.states.len(), 4);
        assert!(spec.states.contains_key("idle"));
        assert!(spec.states.contains_key("approaching"));
        assert_eq!(spec.transitions.len(), 3);
        // wildcard from
        assert_eq!(spec.transitions[2].from, "*");
        assert_eq!(spec.transitions[2].priority, Some(100));
        // timeout
        assert_eq!(spec.timeout.get("pressing").map(|d| d.millis), Some(60_000));
        assert_eq!(spec.on_timeout, Some("fault".to_owned()));
        // fault state has two entry actions
        let fault = &spec.states["fault"];
        assert_eq!(fault.entry.len(), 2);
        assert_eq!(
            fault.entry[0].target,
            ActionTarget::Capability("hydraulic_axis.stop".to_owned())
        );
        assert_eq!(
            fault.entry[1].target,
            ActionTarget::Action("alarm.raise".to_owned())
        );
    }

    #[test]
    fn workflow_spec_yaml_round_trip() {
        let yaml = r#"
id: test_wf
version: "0.1.0"
states:
  idle:
    entry: []
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let re_yaml = serde_yaml::to_string(&spec).unwrap();
        let back: WorkflowSpec = serde_yaml::from_str(&re_yaml).unwrap();
        assert_eq!(spec.id, back.id);
        assert_eq!(spec.version, back.version);
        assert_eq!(spec.states.len(), back.states.len());
    }

    #[test]
    fn 最小_workflow_spec_解析成功() {
        let yaml = r#"
id: minimal
version: "1.0.0"
states:
  idle:
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.id, "minimal");
        assert!(spec.devices.is_empty());
        assert!(spec.transitions.is_empty());
    }
}
