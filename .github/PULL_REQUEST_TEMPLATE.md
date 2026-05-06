## 摘要

请用中文说明本 PR 解决什么问题，以及用户可见行为是否变化。

## 变更内容

-

## 影响面检查

- [ ] 公开 API、trait、数据模型、存储结构、IPC 或事件通道无变化，或已在摘要/风险中说明
- [ ] 架构边界、crate/module 依赖方向无变化，或已补充 ADR/RFC/Spec/Plan
- [ ] 安全、认证、权限、密钥、隐私数据无变化，或已在摘要/风险中说明
- [ ] 平台支持、运行环境、CI/CD、Dev Container 或发布流程无变化，或已同步文档
- [ ] 第三方源码、供应商交付物、生成物无变化，或已说明来源、版本、许可证、验证和真值源
- [ ] 生成文件无变化，或已说明生成命令并提交 `web/src/generated/` diff
- [ ] 没有破坏性变化，或已使用 `!` / `BREAKING CHANGE` 标注迁移方式
- [ ] 每个 commit 都包含 DCO `Signed-off-by` trailer，或已说明例外原因

## 测试

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `npm --prefix web run test`
- [ ] `npm --prefix web run build`
- [ ] 其他：

## 文档

- [ ] 如果命令、架构、IPC、节点清单或约束变化，已更新 `AGENTS.md`
- [ ] 如果安装、运行、测试、发布方式变化，已更新 `README.md`
- [ ] 如果模块局部契约变化，已更新最近的 `AGENTS.md`
- [ ] 如果适用，已更新 ADR/RFC/Spec/Plan 或 `docs/README.md`

## 风险与回滚

请说明主要风险、未覆盖的验证，以及回滚方式。
