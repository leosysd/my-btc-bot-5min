#!/usr/bin/env bash
# ==========================================================================
#  get.sh -- 一行远程安装 jybot-rs（无需 git / Rust / 编译）
#
#  用法：
#    curl -fsSL https://raw.githubusercontent.com/leosysd/my-btc-bot-5min/main/get.sh | bash
#
#  "系统级"模型：所有配置/状态/日志都集中在一个全局目录（默认 ~/.jybot，
#  可用 JYBOT_HOME 覆盖）。二进制装到 $JYBOT_HOME/bin，全局命令 jybot 永远
#  读写这同一份配置——你在面板里改的就是整台机器人的唯一设置。
#
#  可选环境变量：
#    JYBOT_HOME=/opt/jybot     全局数据目录（默认 ~/.jybot）
#    JYBOT_REPO=owner/repo      仓库（默认 leosysd/my-btc-bot-5min）
# ==========================================================================
set -euo pipefail

REPO="${JYBOT_REPO:-leosysd/my-btc-bot-5min}"
HOME_DIR="${JYBOT_HOME:-$HOME/.jybot}"
RAW="https://raw.githubusercontent.com/${REPO}/main"
REL="https://github.com/${REPO}/releases/latest/download"

echo "=========================================================================="
echo "  jybot-rs 远程安装   repo=${REPO}"
echo "  全局数据目录 (系统级唯一配置): ${HOME_DIR}"
echo "=========================================================================="

command -v curl >/dev/null 2>&1 || { echo "ERROR: 需要 curl（sudo apt install -y curl）" >&2; exit 1; }

# 平台 → 预编译资产名 / 二进制文件名
OS="$(uname -s 2>/dev/null || echo unknown)"
case "${OS}" in
  Linux*)  ASSET="jybot-rs-x86_64-linux";       BINNAME="jybot-rs" ;;
  Darwin*) echo "macOS 暂无预编译，请用源码: git clone + bash install.sh" >&2; exit 1 ;;
  *)       ASSET="jybot-rs-x86_64-windows.exe";  BINNAME="jybot-rs.exe" ;;
esac

mkdir -p "${HOME_DIR}/bin" "${HOME_DIR}/scripts"

echo "[1/4] 下载预编译二进制 (${ASSET}) → ${HOME_DIR}/bin ..."
if ! curl -fL --retry 3 -o "${HOME_DIR}/bin/${BINNAME}" "${REL}/${ASSET}"; then
  echo "ERROR: 下载二进制失败（可能还没发布 release）。改用源码: git clone + bash install.sh" >&2
  exit 1
fi
chmod +x "${HOME_DIR}/bin/${BINNAME}" 2>/dev/null || true

echo "[2/4] 下载实盘下单助手 place_order.py ..."
curl -fsSL -o "${HOME_DIR}/scripts/place_order.py" "${RAW}/scripts/place_order.py" || true

echo "[3/4] 安装全局命令 jybot ..."
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/jybot" <<EOF
#!/usr/bin/env bash
# jybot 启动器：锁定全局数据目录，保证读写唯一配置
export JYBOT_HOME="${HOME_DIR}"
exec "${HOME_DIR}/bin/${BINNAME}" "\$@"
EOF
chmod +x "$BIN_DIR/jybot"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "      已安装: jybot（$BIN_DIR 在 PATH 中）" ;;
  *) echo "      已安装: $BIN_DIR/jybot —— 加入 PATH:"
     echo "        echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
esac

echo "[4/4] 生成全局配置并校验 ..."
JYBOT_HOME="${HOME_DIR}" "${HOME_DIR}/bin/${BINNAME}" check || true

echo "=========================================================================="
echo "  完成！全局唯一配置: ${HOME_DIR}/.env"
echo "    1) 编辑配置:  nano ${HOME_DIR}/.env   （或在面板里 1/10 项改）"
echo "    2) 打开面板:  jybot     （5 启动服务 · 9 切DRY_RUN · 12 更新）"
echo "  以后更新：面板按 12（拉最新预编译二进制到 ${HOME_DIR}/bin，免编译）。"
echo "=========================================================================="
