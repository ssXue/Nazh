# 项目约定

本文件记录 Nazh 的长期协作约定。根目录 `AGENTS.md` 是最高真值源；如果本文件与 `AGENTS.md` 冲突，以 `AGENTS.md` 为准，并修正文档漂移。

## 语言策略

- 文档、注释、ADR、RFC、Spec、Plan、提交信息、PR 描述和用户可见文案优先使用中文。
- 技术术语、协议名、标准名、产品名、代码标识符、环境变量和配置键保持英文或生态通用写法。
- 代码标识符遵循对应语言生态惯例，例如 Rust `snake_case`、TypeScript `camelCase` / `PascalCase`。

## 仓库结构

```text
AGENTS.md
CONTRIBUTING.md
README.md
SECURITY.md
.devcontainer/
.github/
crates/
src/
src-tauri/
web/
tests/
docs/
```

目录规则：

- 开发环境相关 Docker 文件统一放在 `.devcontainer/`。
- 根目录不放开发用 `Dockerfile`；只有发布生产容器镜像时才允许新增生产镜像构建文件，并在 `README.md` 说明。
- 大模块如果有独立架构边界、依赖约束或修改 checklist，应在模块根目录放局部 `AGENTS.md`。
- 第三方源码、固件、供应商 SDK 等若需要随仓库固定版本，应优先用 git submodule 或明确的下载脚本记录来源、版本、许可证和校验方式。

## 开发环境

默认开发环境是 Dev Container。

宿主机依赖保持最少：

- Git
- Docker / OrbStack / Docker Desktop
- 支持 Dev Container 的编辑器或 AI agent
- 需要原生 GUI、签名、公证或硬件访问时，按对应平台单独说明

容器内依赖由 `.devcontainer/Dockerfile` 管理，包括 Tauri Linux 依赖、Rust、Node、`cargo-deny` 和常用排障工具。项目级安装、编译、运行、测试命令写在 `README.md` 和 `.devcontainer/README.md`。

## 依赖与版本策略

- 原则上使用当前可用的稳定版本或 LTS 线。
- 版本锁定用于可复现，不代表永久冻结。
- 不能升级时，必须说明原因、影响范围、替代方案和重新评估条件。
- Dependabot 覆盖 Cargo、npm、GitHub Actions 和 `.devcontainer` Dockerfile；升级 PR 应跑对应验证命令。

当前基线（2026-04-30）：

- Rust：`rust-toolchain.toml` 的 stable，项目 MSRV 为 Rust 1.94
- Node：24 LTS
- Dev Container：Ubuntu 26.04
- cargo-deny：0.19.4

## 生成物

生成文件必须记录：

- 源输入：Rust 类型、schema、fixture、设计文件或脚本
- 生成器：命令、工具版本、运行位置
- 提交策略：生成物是否提交仓库
- 校验策略：diff 检查、编译检查或测试命令
- 真值源：生成物和源输入冲突时以哪个为准

Nazh 的 ts-rs 生成流程：

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
git diff -- web/src/generated/
```

## JSON 与 JSONC

- 严格 JSON 文件不写注释和尾随逗号，例如 `package.json`、`tauri.conf.json`、GitHub Actions 输入 JSON。
- `.devcontainer/devcontainer.json` 可按 Dev Container 规范使用 JSONC，但当前文件保持严格 JSON，方便通用工具校验。
- 测试 fixture 若采用 JSONC，读取逻辑必须显式剥离注释，并在测试或文档中说明。

## 安全与密钥

- 不提交密钥、私钥、token、证书、生产 `.env`、个人本机路径或客户数据。
- AI provider key、连接配置和本地工程库文件属于敏感边界；日志和错误消息不得输出完整凭据。
- 真实设备、生产数据、批量删除、force push、reset 等破坏性操作需要人工确认。
- 安全漏洞报告流程见 `SECURITY.md`。

## 本地工具状态

以下目录只保存本机或 agent 会话状态，不应进入 git：

- `.claude/`
- `.codex/`
- `.superpowers/`
- `.aider*`
- `.worktrees/`
