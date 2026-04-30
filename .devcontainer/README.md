# Dev Container

本目录定义 Nazh 的开发容器。容器负责 Linux/Tauri 编译依赖、Rust、Node、`cargo-deny` 和常用协作工具；宿主机只需要 Git、Docker/OrbStack/Docker Desktop，以及支持 Dev Container 的编辑器或 agent。

## 基础镜像与工具链

- 基础镜像：`ubuntu:26.04`
- Node：24 LTS
- Rust：stable，带 `rustfmt` 和 `clippy`
- 审计工具：`cargo-deny 0.19.4`
- Tauri Linux 依赖：`libwebkit2gtk-4.1-dev`、`libxdo-dev`、`libssl-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`
- 协作工具：`gh` 不在本镜像内默认安装；需要 GitHub CLI 时可在容器内追加安装或用宿主机凭据执行

## 打开方式

在支持 Dev Container 的编辑器中选择 “Reopen in Container”。首次创建后会执行：

```bash
git config --global --add safe.directory ${containerWorkspaceFolder}
npm --prefix web ci
cargo fetch --locked
```

容器会转发 Vite/Tauri dev server 使用的 `1420` 端口，并为 Cargo registry、Cargo git、`target/` 和 `web/node_modules/` 建立 Docker volume 缓存。

## 容器内常用命令

```bash
npm --prefix web ci
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm --prefix web run test
npm --prefix web run build
cargo deny check
```

## 验证环境

```bash
cat /etc/os-release
node --version
npm --version
rustc --version
cargo --version
cargo-deny --version
rg --version
fd --version
bat --version
mmdc --version
```
