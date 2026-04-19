# AI 结果自动解析载入 Code Node 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `ai_complete()` 自动解析 JSON 返回值为结构化数据，并通过提示词优化引导 AI 输出规范格式。

**Architecture:** 修改 Rhai 沙箱的 `ai_complete()` 函数，使其返回 `Dynamic` 而非 `String`，自动检测并解析 JSON。同时优化节点级 AI 系统提示和前端 Copilot 生成提示词。

**Tech Stack:** Rust (rhai, serde_json), TypeScript (React)

---

### Task 1: 新增 `parse_ai_response()` 函数并修改返回类型

**Files:**
- Modify: `crates/scripting/src/lib.rs`

- [ ] **Step 1: 在 `crates/scripting/src/lib.rs` 中添加 `parse_ai_response()` 函数**

在 `to_rhai_error()` 函数（第 167-173 行）之后、`register_ai_complete()` 函数（第 175 行）之前，插入：

```rust
fn parse_ai_response(content: String) -> Dynamic {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Dynamic::UNIT;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => to_dynamic(value).unwrap_or_else(|_| Dynamic::from(content)),
        Err(_) => Dynamic::from(content),
    }
}
```

- [ ] **Step 2: 修改 `RhaiAiRuntime::complete()` 返回类型和逻辑**

第 141 行 `Ok(content)` → `Ok(parse_ai_response(content))`，返回类型改为 `Result<Dynamic, Box<EvalAltResult>>`。

- [ ] **Step 3: 修改 `RhaiAiBinding::complete()` 返回类型**

返回类型改为 `Result<Dynamic, Box<EvalAltResult>>`。

- [ ] **Step 4: 修改 `register_ai_complete()` 中的注册签名**

注册函数签名改为 `Result<Dynamic, Box<EvalAltResult>>`。

- [ ] **Step 5: 验证编译通过**

Run: `cargo check --manifest-path crates/scripting/Cargo.toml`

- [ ] **Step 6: 提交**

```
git commit -s -m "feat(scripting): ai_complete() 自动解析 JSON 返回值为结构化数据"
```

---

### Task 2: 修改节点级 AI 系统提示前缀

**Files:**
- Modify: `crates/scripting/src/lib.rs`

- [ ] **Step 1: 修改 `RhaiAiRuntime::build_request()` 方法**

在 system_prompt 拼接时追加 JSON 格式引导前缀。

- [ ] **Step 2: 验证编译通过**

- [ ] **Step 3: 提交**

```
git commit -s -m "feat(scripting): 节点级 AI 系统提示追加 JSON 格式引导前缀"
```

---

### Task 3: 添加 `JsonStubAiService` 测试辅助

**Files:**
- Modify: `tests/workflow.rs`

- [ ] **Step 1: 新增 `JsonStubAiService` 结构体（可配置响应内容）**
- [ ] **Step 2: 保留原 `StubAiService` 不变**
- [ ] **Step 3: 运行现有测试确认无回归**
- [ ] **Step 4: 提交**

---

### Task 4: 新增 `ai_complete` JSON 自动解析端到端测试

**Files:**
- Modify: `tests/workflow.rs`

- [ ] **Step 1: 添加 JSON 对象自动解析测试**
- [ ] **Step 2: 添加纯文本保持字符串测试**
- [ ] **Step 3: 添加 JSON 数组解析测试**
- [ ] **Step 4: 运行全量测试**
- [ ] **Step 5: 提交**

---

### Task 5: 更新前端 Copilot 脚本生成提示词

**Files:**
- Modify: `web/src/lib/script-generation.ts`
- Modify: `web/src/lib/__tests__/script-generation.test.ts`

- [ ] **Step 1: 更新 `SYSTEM_PROMPT` 中关于 `ai_complete` 的描述**
- [ ] **Step 2: 更新对应前端单元测试**
- [ ] **Step 3: 运行前端测试**
- [ ] **Step 4: 提交**

---

### Task 6: 全量验证与 lint

- [ ] **Step 1: Rust lint**
- [ ] **Step 2: Rust 全量测试**
- [ ] **Step 3: 前端全量测试**
