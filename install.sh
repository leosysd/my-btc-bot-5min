#!/usr/bin/env bash
# ==========================================================================
#  install.sh -- 安装 jybot-rs（优先拉取 GitHub 预编译二进制，免本机编译）
#    1) 尝试从 Releases 下载预编译二进制（快，VPS 无需装 Rust）
#    2) 下载不到则回退：装 Rust + cargo build --release
#    3) 生成 .env、安装全局命令 jybot、校验配置
#  用法:  bash install.sh
# ==========================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT}"

REPO_DEFAULT="leosysd/my-btc-bot-5min"
# 从 git remote 推断仓库（失败用默认）
REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#.*github.com[:/]##; s#\.git$##')"
[ -z "${REPO}" ] && REPO="${REPO_DEFAULT}"

echo "=========================================================================="
echo "  jybot-rs 安装器   (repo: ${REPO})"
echo "  目录: ${ROOT}"
echo "=========================================================================="

# ── 选择平台资产名 ─────────────────────────────────────────────────────────
OS="$(uname -s 2>/dev/null || echo unknown)"
case "${OS}" in
  Linux*)  ASSET="jybot-rs-x86_64-linux"; BIN="target/release/jybot-rs" ;;
  Darwin*) ASSET="";                      BIN="target/release/jybot-rs" ;;  # mac 暂无预编译
  *)       ASSET="jybot-rs-x86_64-windows.exe"; BIN="target/release/jybot-rs.exe" ;;
esac

mkdir -p target/release
GOT_BINARY=0

# ── 1) 尝试下载预编译二进制 ────────────────────────────────────────────────
if [ -n "${ASSET}" ] && command -v curl >/dev/null 2>&1; then
  URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
  echo "[1/5] 下载预编译二进制: ${URL}"
  if curl -fL --retry 3 -o "${BIN}.new" "${URL}"; then
    mv "${BIN}.new" "${BIN}"
    chmod +x "${BIN}" 2>/dev/null || true
    GOT_BINARY=1
    echo "      OK：已获取预编译二进制（跳过本机编译）"
  else
    rm -f "${BIN}.new"
    echo "      未获取到预编译（可能还没发布 release）→ 回退源码编译"
  fi
else
  echo "[1/5] 跳过预编译下载（无 curl 或平台无预编译）→ 源码编译"
fi

# ── 2) 回退：装 Rust + 编译 ─────────────────────────────────────────────────
if [ "${GOT_BINARY}" -eq 0 ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "[2/5] 安装 Rust 工具链 ..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
  else
    echo "[2/5] Rust 已安装: $(cargo --version)"
  fi
  echo "      编译 (cargo build --release) ..."
  cargo build --release
else
  echo "[2/5] 已用预编译二进制，无需 Rust。"
fi

# ── 3) .env ────────────────────────────────────────────────────────────────
if [ ! -f ".env" ]; then
  echo "[3/5] 生成 .env（请编辑填写）"
  cp .env.example .env
else
  echo "[3/5] .env 已存在（保留）"
fi

# ── 4) 全局命令 jybot ──────────────────────────────────────────────────────
echo "[4/5] 安装全局命令 jybot ..."
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/jybot" <<EOF
#!/usr/bin/env bash
# jybot 启动器（由 install.sh 生成）—— 切到安装目录再运行，确保找到 .env / scripts
cd "${ROOT}" && exec "${ROOT}/${BIN}" "\$@"
EOF
chmod +x "$BIN_DIR/jybot"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "      已安装: jybot（$BIN_DIR 在 PATH 中）" ;;
  *) echo "      已安装: $BIN_DIR/jybot —— 请把它加入 PATH:"
     echo "        echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
esac

# ── 5) 校验 ────────────────────────────────────────────────────────────────
echo "[5/5] 校验配置 ..."
"${BIN}" check || true

echo "=========================================================================="
echo "  完成！"
echo "  打开管理面板:   jybot        （或 ${BIN}）"
echo "  面板里:  5 启动服务 · 6 停止 · 9 切换DRY_RUN · 12 更新程序(拉预编译)"
echo "  实盘前:  .env 设 DRY_RUN=false + LIVE_TRADING=true（面板第 9 项）"
echo "=========================================================================="
