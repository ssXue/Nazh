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
  adr/
  rfcs/
  specs/
  plans/
  blueprints/
  templates/
```

目录规则：

- 开发环境相关 Docker 文件统一放在 `.devcontainer/`。
- 根目录不放开发用 `Dockerfile`；只有发布生产容器镜像时才允许新增生产镜像构建文件，并在 `README.md` 说明。
- 大模块如果有独立架构边界、依赖约束或修改 checklist，应在模块根目录放局部 `AGENTS.md`。
- 第三方源码、固件、供应商 SDK 等若需要随仓库固定版本，应优先用 git submodule 或明确的下载脚本记录来源、版本、许可证和校验方式。
- `docs/specs/` 保存功能或子系统设计，`docs/plans/` 保存可执行实施计划，`docs/blueprints/` 保存历史蓝图或评审基准。不要再新增 `docs/superpowers/` 路径。

## 代码组织与文件规模

手写源代码文件应保持职责单一、边界清晰、易于 review。一般情况下，单个文件以 100-200 行为较理想状态；超过 300 行时，应主动检查是否承担了多个职责、隐藏了过多状态，或混合了不该放在一起的层次。

原则上，单个手写源代码文件不应超过 500 行。新增或修改代码导致文件超过 500 行时，PR 应说明暂不拆分的理由，或同步给出拆分计划。拆分依据优先是职责、依赖方向、测试边界和变更频率，不为行数制造无意义 wrapper、过深目录或循环依赖。

生成代码、lockfile、fixture、snapshot、migration、schema/IDL、大型静态数据、第三方/vendor 文件，以及框架明确要求集中维护的入口文件可以例外；例外应能说明来源或原因。

## 开发环境

默认开发环境是常驻 Dev Container。宿主机只负责 Git、Docker/Dev Container 编排，以及支持 Dev Container 的编辑器或 AI agent；Rust、Node、Tauri 依赖、构建工具、测试工具、生成器和审计工具不作为宿主机项目运行环境。

宿主机依赖保持最少：

- Git
- Docker / OrbStack / Docker Desktop
- 支持 Dev Container 的编辑器或 AI agent
- 需要原生 GUI、签名、公证或硬件访问时，按对应平台单独说明

容器内依赖由 `.devcontainer/Dockerfile` 管理，包括 Tauri Linux 依赖、Rust、Node、`cargo-deny` 和常用排障工具。项目级安装、编译、运行、测试、生成、打包和发布命令写在 `README.md` 和 `.devcontainer/README.md`，并默认通过 `docker exec "$DEVCONTAINER_NAME" ...`、`devcontainer exec`、编辑器 Dev Container 终端或 CI 执行。

常驻容器命名规则：

- Dev Container 镜像名：`nazh-devcontainer:latest`。
- Dev Container 显示名：`Nazh Dev Container`。
- 常驻 Dev Container 容器名：`nazh-devcontainer-{username}-{branch}`。
- `username` 和 `branch` 必须归一化为 Docker 容器名允许的字符，分支名中的 `/`、空格和其他特殊字符替换为 `-`。
- `branch` 必须来自当前具名 Git 分支；detached HEAD 状态不得启动常驻 Dev Container。

Docker、Dev Container 和 CI 中的 bind mount 宿主机源路径必须是当前项目目录或其子目录，容器内目标路径必须落在项目工作区内。`target/`、`web/node_modules/` 等 Docker volume 仅作为开发缓存；需要发布、验收或回滚的产物必须写回当前项目目录下声明的宿主机可见目录，例如 `dist/`、`web/dist/` 或后续发布文档声明的目录。

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

## 运行时配置

环境相关、部署相关、现场差异相关、硬件拓扑相关、诊断流程、实时调度、连接参数、安全阈值和 AI provider 等运行时配置必须由受控配置源显式声明。缺少配置时应 fail fast，返回包含配置键、所属功能和安全上下文的错误，不得通过 `serde(default)`、`Default`、`unwrap_or(...)` 或等价 fallback 静默补值。

新增运行时配置项必须说明拥有者类型、校验规则、文档位置和测试覆盖。协议常量、领域不变量、数据帧布局、算法内部约束、测试 fixture、消息零值和临时缓冲初始值不属于运行时配置，不应为了“可配置”而外置。

## 第三方代码与供应商交付物

适用于不由 Cargo、npm 等语言生态包管理器直接管理、需要随仓库固定版本的第三方源码、固件、硬件 SDK、仿真模型、协议定义、ESI 文件或供应商交付物。

记录要求：

| 字段 | 要求 |
|------|------|
| 来源 | 上游仓库、供应商、下载地址或交付批次 |
| 集成方式 | git submodule、vendor copy、下载脚本、包管理器 |
| 版本 | tag、commit SHA、release 编号、供应商版本或校验和 |
| 许可证 | LICENSE、NOTICE、商业授权或内部使用限制 |
| 更新方式 | 更新命令、review 要点、回滚方式 |
| 验证方式 | 构建、测试、仿真、硬件诊断或验收命令 |
| 真值源 | 上游、供应商包、生成输入、仓库内源码中的哪一个 |

默认不直接修改第三方代码、固件、SDK、供应商交付物或安装目录中的依赖源码。需要修复或适配时，优先通过上游修复、版本升级、配置、adapter/wrapper 或项目自有代码隔离处理。确需本地补丁时，必须把补丁作为项目自有交付物管理，记录来源、变更原因、补丁文件或重放命令、验证方式、如何向上游回传，以及第三方版本升级时如何重新应用或移除。

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

上述命令默认在 Dev Container 或 CI 内执行。

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
