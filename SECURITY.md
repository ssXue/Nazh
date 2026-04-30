# Security Policy

## Supported Versions

当前 `main` 分支接受安全修复。正式版本发布后，本节应改为按 release line 或 tag 说明支持窗口。

## Reporting A Vulnerability

请不要在公开 Issue 中披露安全漏洞细节。

如果发现 Nazh 的安全问题，请通过私有渠道联系维护者，并在报告中尽量包含：

- 受影响的 commit、分支或版本
- 复现步骤和最小工作流示例
- 影响范围，例如本地文件访问、AI provider token、连接配置、IPC 权限或设备数据
- 已知缓解方式
- 你的联系方式

## Response

维护者收到报告后会先确认影响面，再决定是否临时下线相关功能、发布修复或更新文档。涉及密钥、生产配置、客户数据或设备安全的漏洞，应优先走私有修复和协调披露流程。
