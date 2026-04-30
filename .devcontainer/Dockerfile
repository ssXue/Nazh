FROM ubuntu:26.04

ARG NODE_MAJOR=24
ARG CARGO_DENY_VERSION=0.19.4
ARG CARGO_EDIT_VERSION=0.13.10

ENV DEBIAN_FRONTEND=noninteractive
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# 基础工具 + Tauri v2 Linux 依赖。
# 依赖清单对齐 https://v2.tauri.app/start/prerequisites/ 的 Debian/Ubuntu 段。
RUN apt-get update && apt-get upgrade -y && \
    apt-get install --no-install-recommends --fix-missing -y \
    ca-certificates \
    curl \
    wget \
    git \
    gnupg \
    build-essential \
    pkg-config \
    file \
    libwebkit2gtk-4.1-dev \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libgtk-3-dev \
    libjavascriptcoregtk-4.1-dev \
    libsoup-3.0-dev \
    zsh \
    vim \
    less \
    tree \
    htop \
    jq \
    ripgrep \
    fd-find \
    bat \
    unzip \
    sudo \
    locales \
    python3 \
    python3-pip \
    python-is-python3 \
    graphviz \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && ln -sf /usr/bin/batcat /usr/local/bin/bat \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# Node 24 是 2026-04 当前 LTS；用 NodeSource 避免 Ubuntu 仓库版本漂移。
RUN curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - && \
    apt-get install --no-install-recommends -y nodejs && \
    npm install -g @mermaid-js/mermaid-cli npm-check-updates && \
    apt-get clean -y && \
    rm -rf /var/lib/apt/lists/*

# Rust stable + 项目要求的 rustfmt/clippy。
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --component rustfmt --component clippy
ENV PATH="/root/.cargo/bin:${PATH}"

# Rust manifest 维护工具只装在容器内，避免污染宿主机 Cargo 环境。
RUN cargo install cargo-edit --version "${CARGO_EDIT_VERSION}" --locked && \
    cargo install --list | grep -F "cargo-edit v${CARGO_EDIT_VERSION}:"

# 使用官方 release 二进制，避免在容器构建时从源码编译 cargo-deny。
RUN arch="$(dpkg --print-architecture)" && \
    case "${arch}" in \
      amd64) target="x86_64-unknown-linux-musl" ;; \
      arm64) target="aarch64-unknown-linux-musl" ;; \
      *) echo "unsupported architecture: ${arch}" >&2; exit 1 ;; \
    esac && \
    curl -fsSL \
      "https://github.com/EmbarkStudios/cargo-deny/releases/download/${CARGO_DENY_VERSION}/cargo-deny-${CARGO_DENY_VERSION}-${target}.tar.gz" \
      -o /tmp/cargo-deny.tar.gz && \
    tar -xzf /tmp/cargo-deny.tar.gz -C /usr/local/bin --strip-components=1 \
      "cargo-deny-${CARGO_DENY_VERSION}-${target}/cargo-deny" && \
    rm /tmp/cargo-deny.tar.gz && \
    cargo-deny --version

ENV SHELL=/usr/bin/zsh \
    CARGO_TERM_COLOR=always \
    RUST_BACKTRACE=1

WORKDIR /workspace
