use rhai::{
    EvalAltResult, ImmutableString, def_package,
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

// ── 位操作 / 进制转换 ────────────────────────────────────────────────────────

#[export_module]
mod nazh_bit_helpers {
    use super::{EvalAltResult, ImmutableString, to_package_error};
    use rhai::Dynamic;

    /// 获取整数值指定位的布尔值（LSB 为第 0 位）。
    ///
    /// `bit_get(0b1010, 1)` 返回 `true`（第 1 位为 1）。
    #[rhai_fn(return_raw)]
    pub fn bit_get(value: i64, bit: i64) -> Result<bool, Box<EvalAltResult>> {
        if !(0..=63).contains(&bit) {
            return Err(to_package_error(format!(
                "bit_get(bit) 要求 0 <= bit <= 63，当前 bit={bit}"
            )));
        }
        Ok((value >> bit) & 1 == 1)
    }

    /// 设置整数值指定位为 1（置位）或 0（清零），返回新值。
    ///
    /// `bit_set(0b0000, 2, true)` 返回 `4`。
    #[rhai_fn(return_raw)]
    pub fn bit_set(value: i64, bit: i64, flag: bool) -> Result<i64, Box<EvalAltResult>> {
        if !(0..=63).contains(&bit) {
            return Err(to_package_error(format!(
                "bit_set(bit) 要求 0 <= bit <= 63，当前 bit={bit}"
            )));
        }
        if flag {
            Ok(value | (1i64 << bit))
        } else {
            Ok(value & !(1i64 << bit))
        }
    }

    /// 将整数转换为十六进制字符串（小写，无前缀）。
    ///
    /// `to_hex(255)` 返回 `"ff"`。
    #[must_use]
    pub fn to_hex(value: i64) -> ImmutableString {
        format!("{value:x}").into()
    }

    /// 将十六进制字符串解析为整数（支持可选的 `0x` / `0X` 前缀）。
    ///
    /// `from_hex("0xff")` 返回 `255`。
    #[rhai_fn(return_raw)]
    pub fn from_hex(text: &str) -> Result<i64, Box<EvalAltResult>> {
        let cleaned = text
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        i64::from_str_radix(cleaned, 16)
            .map_err(|e| to_package_error(format!("from_hex 解析失败: {e}")))
    }

    /// 将整数转换为二进制字符串（无前缀）。
    ///
    /// `to_bin(10)` 返回 `"1010"`。
    #[must_use]
    pub fn to_bin(value: i64) -> ImmutableString {
        format!("{value:b}").into()
    }
}

// ── 数值工具 ─────────────────────────────────────────────────────────────────

#[export_module]
mod nazh_math_helpers {
    use super::{EvalAltResult, to_package_error};
    use rhai::Dynamic;

    /// 将浮点数四舍五入到指定小数位数。
    ///
    /// `round(3.14159, 2)` 返回 `3.14`。
    #[rhai_fn(return_raw)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn round(value: f64, decimals: i64) -> Result<f64, Box<EvalAltResult>> {
        if !(0..=15).contains(&decimals) {
            return Err(to_package_error(format!(
                "round(decimals) 要求 0 <= decimals <= 15，当前 decimals={decimals}"
            )));
        }
        // SAFETY: decimals 已校验在 0..=15，as i32 不会截断。
        let factor = 10f64.powi(decimals as i32);
        Ok((value * factor).round() / factor)
    }

    /// 将数值限制在闭区间 [min, max] 内。
    ///
    /// `clamp(105.0, 0.0, 100.0)` 返回 `100.0`。
    #[rhai_fn(return_raw)]
    pub fn clamp(value: f64, min: f64, max: f64) -> Result<f64, Box<EvalAltResult>> {
        if min > max {
            return Err(to_package_error(format!(
                "clamp(min, max) 要求 min <= max，当前 min={min}, max={max}"
            )));
        }
        Ok(value.clamp(min, max))
    }

    /// 线性映射：将 value 从输入区间映射到输出区间。
    ///
    /// 典型场景：传感器原始值 → 工程量（如 4-20mA → 温度）。
    /// `scale(12.0, 4.0, 20.0, 0.0, 100.0)` 返回 `50.0`。
    #[rhai_fn(return_raw)]
    pub fn scale(
        value: f64,
        in_min: f64,
        in_max: f64,
        out_min: f64,
        out_max: f64,
    ) -> Result<f64, Box<EvalAltResult>> {
        if (in_max - in_min).abs() < f64::EPSILON {
            return Err(to_package_error(
                "scale(in_min, in_max) 的输入区间不能为零宽度",
            ));
        }
        Ok(out_min + (value - in_min) * (out_max - out_min) / (in_max - in_min))
    }
}

// ── 时间格式化 ───────────────────────────────────────────────────────────────

#[export_module]
mod nazh_time_helpers {
    use super::{EvalAltResult, ImmutableString, to_package_error};
    use rhai::Dynamic;

    /// 将 Unix 毫秒时间戳按 strftime 格式格式化（UTC 时区）。
    ///
    /// 格式占位符遵循 chrono strftime 规范：
    /// `format_ts(now_ms(), "%Y-%m-%d %H:%M:%S")` → `"2026-05-17 14:30:00"`
    #[rhai_fn(return_raw)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn format_ts(ms: i64, fmt: &str) -> Result<ImmutableString, Box<EvalAltResult>> {
        let secs = ms.div_euclid(1000);
        // SAFETY: rem_euclid(1000) 结果在 0..1000，as u32 不截断。
        let nanos = ms.rem_euclid(1000) as u32 * 1_000_000;
        chrono::DateTime::from_timestamp(secs, nanos)
            .ok_or_else(|| to_package_error(format!("format_ts: 时间戳 {ms} 无法转换为有效日期")))
            .map(|dt| dt.format(fmt).to_string().into())
    }

    /// 计算两个毫秒时间戳之间的毫秒差（end - start）。
    ///
    /// 如果 end < start，返回负数。
    #[must_use]
    pub fn duration_ms(start: i64, end: i64) -> i64 {
        end - start
    }
}

def_package! {
    /// Nazh 脚本节点的默认辅助函数包。
    ///
    /// 这里收口所有"默认可用"的通用函数，避免散落在各处通过
    /// `register_fn` 临时注册，方便后续扩展、文档生成和 AI 提示词同步。
    pub NazhScriptPackage(module) {
        combine_with_exported_module!(module, "nazh-script-helpers", nazh_script_helpers);
        combine_with_exported_module!(module, "nazh-bit-helpers", nazh_bit_helpers);
        combine_with_exported_module!(module, "nazh-math-helpers", nazh_math_helpers);
        combine_with_exported_module!(module, "nazh-time-helpers", nazh_time_helpers);
    }
}

/// 生成 Rhai 脚本 API 参考文本，供 copilot `get_scripting_reference` 工具使用。
///
/// 输出与 copilot 系统提示词中的 Rhai 文档段落等价，但从此 crate 的实际注册函数
/// 自动生成——新增/修改函数后只需更新此处的条目即可。
#[allow(clippy::uninlined_format_args)]
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

// 位操作 / 进制转换
bit_get(value, bit)     // 获取第 bit 位的值（LSB=0），返回 bool
bit_set(value, bit, flag) // 设置第 bit 位，返回新值
to_hex(value)           // 整数 → 十六进制字符串（小写）
from_hex(text)          // 十六进制字符串 → 整数（支持 0x 前缀）
to_bin(value)           // 整数 → 二进制字符串

// 数值工具
round(value, decimals)  // 四舍五入到指定小数位（0-15）
clamp(value, min, max)  // 限制在 [min, max] 闭区间内
scale(value, in_min, in_max, out_min, out_max) // 线性映射（如 4-20mA → 工程量）

// 时间格式化
format_ts(ms, fmt)      // Unix 毫秒 → 格式化字符串（UTC，strftime 语法）
duration_ms(start, end) // 两个毫秒时间戳的差值
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

// Modbus 寄存器位操作
let reg = payload.register_value;
let alarm = bit_get(reg, 3);          // 读第 3 位
payload.alarm_active = alarm;
payload.reg_hex = to_hex(reg);        // 转十六进制显示

// 传感器线性映射（4-20mA → 0-100°C）
payload.temperature = scale(payload.current_ma, 4.0, 20.0, 0.0, 100.0);

// 数值裁剪与精度
payload.value = round(clamp(payload.raw, 0.0, 100.0), 2);

// 时间格式化
payload.timestamp_str = format_ts(now_ms(), "%Y-%m-%d %H:%M:%S");

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
