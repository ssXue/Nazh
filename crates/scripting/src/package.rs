use std::time::{SystemTime, UNIX_EPOCH};

use rhai::{
    Dynamic, EvalAltResult, ImmutableString, def_package,
    plugin::*,
    serde::{from_dynamic, to_dynamic},
};
use serde_json::Value;

fn to_package_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        message.into().into(),
        rhai::Position::NONE,
    ))
}

#[export_module]
mod nazh_script_helpers {
    use super::*;

    /// 生成闭区间随机整数。
    ///
    /// `rand(3, 7)` 可能返回 3、4、5、6、7 中任意一个值。
    #[rhai_fn(return_raw)]
    pub fn rand(min: i64, max: i64) -> Result<i64, Box<EvalAltResult>> {
        if min > max {
            return Err(to_package_error(format!(
                "rand(min, max) 要求 min <= max，当前收到 min={min}, max={max}"
            )));
        }

        Ok(fastrand::i64(min..=max))
    }

    /// 返回当前 Unix 时间戳，单位毫秒。
    #[must_use]
    pub fn now_ms() -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis().try_into().unwrap_or(i64::MAX),
            Err(_) => 0,
        }
    }

    /// 判断字符串在去掉首尾空白后是否为空。
    #[must_use]
    pub fn is_blank(text: &str) -> bool {
        text.trim().is_empty()
    }

    /// 将 JSON 字符串解析成 Rhai 值。
    #[rhai_fn(return_raw)]
    pub fn from_json(text: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        let value: Value = serde_json::from_str(text)
            .map_err(|error| to_package_error(format!("from_json 解析失败: {error}")))?;
        to_dynamic(value).map_err(|error| to_package_error(format!("from_json 转换失败: {error}")))
    }

    /// 将 Rhai 值序列化成 JSON 字符串。
    #[rhai_fn(return_raw)]
    pub fn to_json(value: Dynamic) -> Result<ImmutableString, Box<EvalAltResult>> {
        let json_value = from_dynamic::<Value>(&value)
            .map_err(|error| to_package_error(format!("to_json 转换失败: {error}")))?;
        serde_json::to_string(&json_value)
            .map(Into::into)
            .map_err(|error| to_package_error(format!("to_json 序列化失败: {error}")))
    }
}

def_package! {
    /// Nazh 脚本节点的默认辅助函数包。
    ///
    /// 这里收口所有“默认可用”的通用函数，避免散落在各处通过
    /// `register_fn` 临时注册，方便后续扩展、文档生成和 AI 提示词同步。
    pub NazhScriptPackage(module) {
        combine_with_exported_module!(module, "nazh-script-helpers", nazh_script_helpers);
    }
}
