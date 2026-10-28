#!/usr/bin/env bash
# build_linux.sh —— pygame-rs-dev Ubuntu/Debian 原生构建（一键产出桌面
# 解释器 python 可执行文件）。
#
# 用法（Ubuntu 20.04+ / Debian 系）：
#   chmod +x scripts/build_linux.sh
#   ./scripts/build_linux.sh
#
# 产物：target/release/python（pygame 全内置解释器——
#   ./target/release/python examples/camera_kit_demo.py 即跑脚本）
#
# 为什么必须在 Linux 上构建：Windows 交叉编译到 x86_64-unknown-linux-gnu
#   需要 Linux sysroot（pkg-config + Wayland/X11/CRT 头与库），得不偿失；
#   本机原生构建零额外配置。Web/Android 产物与宿主系统无关（各自独立
#   工具链），不受影响。
set -euo pipefail

cd "$(dirname "$0")/.."

# ── 1/3 系统依赖（winit=Wayland/X11 键盘布局、cpal=ALSA 音频、
#    gilrs=udev 手柄；缺一样链接期报错）────────────────────────────
if command -v apt-get >/dev/null 2>&1; then
    echo "=== 1/3 apt 依赖（sudo 需要时输密码）==="
    sudo apt-get update -qq
    sudo apt-get install -y -qq \
        build-essential pkg-config \
        libxkbcommon-dev libwayland-dev libxkbcommon-x11-dev \
        libx11-dev libxcb1-dev \
        libasound2-dev libudev-dev
else
    echo "=== 1/3 非 apt 系统：请自备 C 工具链 + pkg-config +"
    echo "    wayland/xkbcommon/alsa/udev 开发库（跳过安装）"
fi

# ── 2/3 Rust 工具链（已有则跳过）─────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
    echo "=== 2/3 未检测到 cargo——安装 rustup（默认 stable）==="
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "=== 2/3 cargo 已就绪：$(cargo --version)"
fi

# ── 3/3 构建（release——RP debug genesis 必败，铁律同 Android）────
echo "=== 3/3 cargo build --release --bin python ==="
cargo build --release --bin python

BIN=target/release/python
echo
echo "[OK] $BIN ($(du -h "$BIN" | cut -f1))"
echo "    试跑：./$BIN examples/camera_kit_demo.py"
echo "    脚本开发：./$BIN 你的脚本.py（多文件工程：modules/ 与脚本同级）"
