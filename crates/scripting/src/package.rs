use rhai::{
    EvalAltResult, def_package,
    plugin::{
        FuncRegistration, Module, NativeCallContext, PluginFunc, RhaiResult, TypeId,
        combine_with_exported_module, export_module, mem,
    },
};

#[allow(clippy::unnecessary_box_returns)]
fn to_package_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        message.into().into(),
        rhai::Position::NONE,
    ))
}

#[export_module]
mod nazh_script_helpers {
    use super::{EvalAltResult, to_package_error};
    use rhai::{
        Dynamic, ImmutableString,
        serde::{from_dynamic, to_dynamic},
    };
    use serde_json::Value;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    /// 这里收口所有"默认可用"的通用函数，避免散落在各处通过
    /// `register_fn` 临时注册，方便后续扩展、文档生成和 AI 提示词同步。
    pub NazhScriptPackage(module) {
        combine_with_exported_module!(module, "nazh-script-helpers", nazh_script_helpers);
    }
}

/// 生成 Rhai 脚本 API 参考文本，供 copilot `get_scripting_reference` 工具使用。
///
/// 输出与 copilot 系统提示词中的 Rhai 文档段落等价，但从此 crate 的实际注册函数
/// 自动生成——新增/修改函数后只需更新此处的条目即可。
pub fn generate_api_reference() -> String {
    let max_ops = crate::default_max_operations();
    format!(
        r#"### Rhai 脚本 API（code 节点、if/switch/loop 条件脚本）
脚本语言为 Rhai，运行在沙箱中（无 I/O、无网络、步数上限 {max_ops}）。脚本通过 `payload` 变量访问输入数据，修改后作为输出。

#### 内置函数
```
rand(min, max)          // 闭区间随机整数，如 rand(1, 100)
now_ms()                // 当前 Unix 时间戳（毫秒）
is_blank(text)          // 判断字符串是否为空或纯空白
from_json(json_str)     // JSON 字符串 → Rhai 值
to_json(value)          // Rhai 值 → JSON 字符串
```

#### 工作流变量（需工作流定义中声明 variables）
```
vars.get("name")        // 读取变量，不存在则报错
vars.set("name", value) // 写入变量（类型校验）
vars.cas("name", old, new) // 比较交换，返回 bool
```

#### 脚本示例
```rhai
// 数据变换
payload.temperature_c = (payload.temperature_f - 32) * 5 / 9;

// 计数器（需声明 workflow variable "counter"）
let c = vars.get("counter");
vars.set("counter", c + 1);

// JSON 处理
let config = from_json(payload.config_json);
config.enabled = true;
payload.new_config = to_json(config);

// 条件判断（if 节点）
payload.temperature > 100
```

#### 重要约束
- **不要使用** `global_get`、`global_set`、`get_state`、`set_state` 等函数——它们在 Nazh 中不存在。变量读写只能通过 `vars.get` / `vars.set`。
- `vars` 仅在工作流声明了 variables 时可用，否则运行时报错。
- 脚本不能执行 I/O 操作（HTTP、文件、串口等），这些由专门的 I/O 节点完成。
- if/switch 节点的条件脚本必须返回 bool，loop 节点返回循环次数或数组。"#,
        max_ops = max_ops,
    )
}
