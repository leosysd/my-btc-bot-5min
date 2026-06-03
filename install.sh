#!/usr/bin/env bash
# ==========================================================================
#  install.sh -- 从源码目录安装 jybot-rs（git clone 后用）
#    1) 优先从 Releases 下载预编译二进制（免本机编译）
#    2) 下载不到则装 Rust + cargo build --release
#    3) 把二进制装进全局目录 $JYBOT_HOME/bin，安装全局命令 jybot
#
#  "系统级"模型：配置/状态/日志集中在 $JYBOT_HOME（默认 ~/.jybot），
#  全局命令 jybot 永远读写这同一份——面板里改的就是唯一设置。
#
#  可选: JYBOT_HOME=/opt/jybot bash install.sh
# ==========================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT}"

REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#.*github.com[:/]##; s#\.git$##')"
[ -z "${REPO}" ] && REPO="leosysd/my-btc-bot-5min"
HOME_DIR="${JYBOT_HOME:-$HOME/.jybot}"

echo "=========================================================================="
echo "  jybot-rs 安装器   repo=${REPO}"
echo "  源码目录: ${ROOT}"
echo "  全局数据目录 (系统级唯一配置): ${HOME_DIR}"
echo "=========================================================================="

OS="$(uname -s 2>/dev/null || echo unknown)"
case "${OS}" in
  Linux*)  ASSET="jybot-rs-x86_64-linux";       BINNAME="jybot-rs" ;;
  Darwin*) ASSET="";                            BINNAME="jybot-rs" ;;
  *)       ASSET="jybot-rs-x86_64-windows.exe";  BINNAME="jybot-rs.exe" ;;
esac

mkdir -p "${HOME_DIR}/bin" "${HOME_DIR}/scripts"
BUILT="target/release/${BINNAME}"
GOT=0

# 1) 预编译二进制
if [ -n "${ASSET}" ] && command -v curl >/dev/null 2>&1; then
  URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
  echo "[1/4] 下载预编译二进制: ${URL}"
  if curl -fL --retry 3 -o "${HOME_DIR}/bin/${BINNAME}" "${URL}"; then
    GOT=1; echo "      OK（跳过本机编译）"
  else
    echo "      未获取到预编译 → 回退源码编译"
  fi
fi

# 2) 回退编译
if [ "${GOT}" -eq 0 ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "[2/4] 安装 Rust ..."; curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
  else
    echo "[2/4] Rust 已安装: $(cargo --version)"
  fi
  echo "      cargo build --release ..."; cargo build --release
  cp "${BUILT}" "${HOME_DIR}/bin/${BINNAME}"
fi
chmod +x "${HOME_DIR}/bin/${BINNAME}" 2>/dev/null || true
cp -f scripts/place_order.py "${HOME_DIR}/scripts/place_order.py" 2>/dev/null || true

# 3) 全局命令 jybot
echo "[3/4] 安装全局命令 jybot ..."
BIN_DIR="$HOME/.local/bin"; mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/jybot" <<EOF
#!/usr/bin/env bash
export JYBOT_HOME="${HOME_DIR}"
exec "${HOME_DIR}/bin/${BINNAME}" "\$@"
EOF
chmod +x "$BIN_DIR/jybot"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "      已安装: jybot（$BIN_DIR 在 PATH 中）" ;;
  *) echo "      已安装: $BIN_DIR/jybot —— 加入 PATH:"
     echo "        echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
esac

# 4) 生成全局配置并校验
echo "[4/4] 生成全局配置并校验 ..."
JYBOT_HOME="${HOME_DIR}" "${HOME_DIR}/bin/${BINNAME}" check || true

echo "=========================================================================="
echo "  完成！全局唯一配置: ${HOME_DIR}/.env"
echo "  打开面板:  jybot      （5 启动服务 · 9 切DRY_RUN · 12 更新）"
echo "  以后更新:  面板按 12（拉预编译到 ${HOME_DIR}/bin）"
echo "=========================================================================="
