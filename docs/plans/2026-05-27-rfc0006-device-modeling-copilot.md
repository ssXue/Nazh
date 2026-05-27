**Goal:** 实施 RFC-0006 设备建模 Copilot 接管——用 copilot 对话替换所有设备建模 UI 入口
**Architecture:** Rust 校验层 (`dsl-core`) + Copilot 工具调度 (`copilot_tools.rs`) + 前端工具定义 (`copilot-tools.ts`) + 结构化抽取管道 (`extraction-pipeline.ts`)
**Tech Stack:** Rust / serde / serde_yaml / TypeScript / Zod / ai-sdk (`generateObject`)

## Phase 1：`DeviceSpec::validate()` + 校验工具（~2 天）

- [ ] `crates/dsl-core/src/device.rs`：新增 `ValidationLevel` / `ValidationDiagnostic` / `ValidationResult` 类型
- [ ] `crates/dsl-core/src/device.rs`：实现 `DeviceSpec::validate()` 覆盖 RFC 表中全部规则
- [ ] `crates/dsl-core/src/parser.rs`：补实 `parse_device_yaml_validated` 调用 `validate()`
- [ ] `crates/dsl-core/src/lib.rs`：re-export 新增类型
- [ ] `src-tauri/src/commands/copilot_tools.rs`：新增 `tool_validate_device_yaml` + match arm
- [ ] `web/src/ai/copilot-tools.ts`：新增 `validate_device_yaml` 工具定义
- [ ] 单元测试覆盖每条校验规则
- [ ] 更新 `crates/dsl-core/AGENTS.md` 校验规则表

## Phase 2：schema template + capability inference 工具（~2 天）

- [ ] `crates/dsl-core/src/device.rs`：新增 `signal_schema_template()` 函数
- [ ] `src-tauri/src/commands/copilot_tools.rs`：新增 `tool_get_signal_schema_template` + match arm
- [ ] `web/src/ai/copilot-tools.ts`：新增 `get_signal_schema_template` 工具定义
- [ ] `crates/dsl-core/src/capability.rs`：新增 `CapabilityInference`，重构单信号级推断
- [ ] `src-tauri/src/commands/copilot_tools.rs`：新增 `tool_infer_capabilities` + match arm
- [ ] `web/src/ai/copilot-tools.ts`：新增 `infer_capabilities_from_signals` 工具定义
- [ ] 单元测试

## Phase 3：资产操作工具（~2 天）

- [ ] `src-tauri/src/commands/copilot_tools.rs`：新增 8 个资产操作 match arm
- [ ] `web/src/ai/copilot-tools.ts`：新增 8 个资产操作工具定义
- [ ] `save_device_asset` 工具内部先 validate 再保存
- [ ] IPC surface 契约测试更新

## Phase 4：系统提示 + 集成验证（~1 天）

- [ ] `web/src/ai/copilot.ts`：新增设备建模专家模式段落
- [ ] 手动测试四个典型流程
- [ ] 更新根 `AGENTS.md` IPC surface + `docs/rfcs/0006` 状态

## Phase 5：结构化抽取管道（~3 天）

- [ ] 新增 `extraction-schemas.ts`：Zod schema
- [ ] 新增 `extraction-pipeline.ts`：多阶段管道编排
- [ ] 重写 `device-extraction.ts`

## Phase 6：Copilot 附件 UI + 现有 UI 移除（~4 天）

- [ ] 新增 `CopilotAttachment.tsx`
- [ ] copilot 面板附件按钮 + 附件栏
- [ ] `copilot-stream.ts` 附件处理
- [ ] 移除 DeviceImportDrawer / DeviceModelingPanel 编辑 / CapabilitiesTab 生成按钮
