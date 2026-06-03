#!/usr/bin/env bash
# ==========================================================================
#  install.sh -- 一键安装 jybot-rs（纯 Rust 版）
#    - 检查/安装 Rust 工具链
#    - cargo build --release
#    - 从 .env.example 生成 .env（若缺失）
#    - 校验配置
#  用法:  bash install.sh
# ==========================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT}"

echo "=========================================================================="
echo "  jybot-rs 安装器"
echo "  目录: ${ROOT}"
echo "=========================================================================="

# 1) Rust 工具链
if ! command -v cargo >/dev/null 2>&1; then
  echo "[1/4] 未检测到 Rust，正在安装 rustup ..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
else
  echo "[1/4] Rust 已安装: $(cargo --version)"
fi

# 2) 编译
echo "[2/4] 编译 (cargo build --release) ..."
cargo build --release

# 3) .env
if [ ! -f ".env" ]; then
  echo "[3/4] 生成 .env（请编辑填写）"
  cp .env.example .env
else
  echo "[3/4] .env 已存在（保留）"
fi

# 4) 校验
echo "[4/4] 校验配置 ..."
./target/release/jybot-rs check || true

echo "=========================================================================="
echo "  完成。下一步:"
echo "    1) 编辑  .env"
echo "    2) 测试  ./target/release/jybot-rs --test-mode"
echo "    3) 模拟  ./target/release/jybot-rs --simulation"
echo "    4) 实盘  .env 设 DRY_RUN=false + LIVE_TRADING=true，再 --live"
echo "=========================================================================="
