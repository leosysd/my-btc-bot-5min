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

# 4) 全局命令 jybot（自动 cd 到安装目录，任何路径都能进面板）
echo "[4/5] 安装全局命令 jybot ..."
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/jybot" <<EOF
#!/usr/bin/env bash
# jybot 启动器（由 install.sh 生成）—— 切到安装目录再运行，确保能找到 .env / scripts
cd "${ROOT}" && exec ./target/release/jybot-rs "\$@"
EOF
chmod +x "$BIN_DIR/jybot"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "      已安装: jybot（$BIN_DIR 在 PATH 中）" ;;
  *) echo "      已安装: $BIN_DIR/jybot —— 请把它加入 PATH:"
     echo "        echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
esac

# 5) 校验
echo "[5/5] 校验配置 ..."
./target/release/jybot-rs check || true

echo "=========================================================================="
echo "  完成！"
echo "  打开管理面板（推荐）:   jybot        （或 ./target/release/jybot-rs）"
echo "  面板里:  5 启动服务 · 6 停止 · 9 切换DRY_RUN · 12 更新程序"
echo "  以后更新:  面板选 12，或命令行  git pull && cargo build --release"
echo "  实盘前:  .env 设 DRY_RUN=false + LIVE_TRADING=true（面板第 9 项）"
echo "=========================================================================="
