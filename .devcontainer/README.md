# Dev Container

本目录定义 Nazh 的开发容器。容器负责 Linux/Tauri 编译依赖、Rust、Node、`cargo-deny` 和常用协作工具；宿主机只需要 Git、Docker/OrbStack/Docker Desktop，以及支持 Dev Container 的编辑器或 agent。

## 基础镜像与工具链

- 基础镜像：`ubuntu:26.04`
- Node：24 LTS
- Rust：stable，带 `rustfmt` 和 `clippy`
- 审计工具：`cargo-deny 0.19.4`
- 依赖维护工具：`cargo-edit 0.13.10`（提供 `cargo upgrade` / `cargo add` 等）、`npm-check-updates`
- Tauri Linux 依赖：`libwebkit2gtk-4.1-dev`、`libxdo-dev`、`libssl-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`
- 协作工具：`gh` 不在本镜像内默认安装；需要 GitHub CLI 时可在容器内追加安装或用宿主机凭据执行

## 打开方式

先在宿主机 shell 中设置稳定容器名，再从同一个 shell 启动支持 Dev Container 的编辑器或 Dev Containers CLI：

```bash
DEVCONTAINER_USER="$(id -un | tr -cs '[:alnum:]_.-' '-' | sed 's/^-//;s/-$//')"
DEVCONTAINER_BRANCH="$(git branch --show-current | tr -cs '[:alnum:]_.-' '-' | sed 's/^-//;s/-$//')"
test -n "$DEVCONTAINER_BRANCH"
export DEVCONTAINER_NAME="nazh-devcontainer-${DEVCONTAINER_USER}-${DEVCONTAINER_BRANCH}"
```

容器命名约定：

- Dev Container 镜像名：`nazh-devcontainer:latest`
- Dev Container 显示名：`Nazh Dev Container`
- 常驻 Dev Container 容器名：`nazh-devcontainer-{username}-{branch}`

在支持 Dev Container 的编辑器中选择 “Reopen in Container”。首次创建后会执行：

```bash
git config --global --add safe.directory ${containerWorkspaceFolder}
npm --prefix web ci
cargo fetch --locked
```

容器会转发 Vite/Tauri dev server 使用的 `1420` 端口，并为 Cargo registry、Cargo git、`target/` 和 `web/node_modules/` 建立 Docker volume 缓存。缓存 volume 不是发布产物真值源；需要保留、发布、验收或回滚的产物必须写回宿主机可见的项目目录，例如 `dist/`、`web/dist/` 或发布文档声明的目录。

没有编辑器集成时，可以手动创建同名常驻容器：

```bash
docker build -f .devcontainer/Dockerfile -t nazh-devcontainer:latest .
docker inspect "$DEVCONTAINER_NAME" >/dev/null 2>&1 || docker run -d \
  --name "$DEVCONTAINER_NAME" \
  --mount "type=bind,src=$PWD,dst=/workspace/Nazh" \
  --mount "type=volume,src=nazh-cargo-registry,dst=/root/.cargo/registry" \
  --mount "type=volume,src=nazh-cargo-git,dst=/root/.cargo/git" \
  --mount "type=volume,src=nazh-target,dst=/workspace/Nazh/target" \
  --mount "type=volume,src=nazh-web-node-modules,dst=/workspace/Nazh/web/node_modules" \
  -w /workspace/Nazh \
  -p 1420:1420 \
  nazh-devcontainer:latest sleep infinity
if [ "$(docker inspect -f '{{.State.Running}}' "$DEVCONTAINER_NAME")" != "true" ]; then
  docker start "$DEVCONTAINER_NAME" >/dev/null
fi
docker exec "$DEVCONTAINER_NAME" bash -lc 'git config --global --add safe.directory /workspace/Nazh && npm --prefix web ci && cargo fetch --locked'
```

## 容器内常用命令

以下命令在已启动的 Dev Container 内执行；宿主机侧使用 `docker exec "$DEVCONTAINER_NAME" ...`、`devcontainer exec` 或编辑器容器终端进入。

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
cargo install --list | grep 'cargo-edit v0.13.10'
ncu --version
rg --version
fd --version
bat --version
mmdc --version
```
