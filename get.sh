#!/usr/bin/env bash
# ==========================================================================
#  get.sh -- 一行远程安装 jybot-rs（无需 git / Rust / 编译）
#
#  用法（在 VPS 上直接跑）：
#    curl -fsSL https://raw.githubusercontent.com/leosysd/my-btc-bot-5min/main/get.sh | bash
#
#  它会：下载预编译二进制 + .env.example + place_order.py → 生成 .env
#        → 安装全局命令 jybot → 校验配置。
#  可选环境变量：
#    JYBOT_DIR=/opt/jybot-rs   自定义安装目录（默认 ~/jybot-rs）
#    JYBOT_REPO=owner/repo      自定义仓库（默认 leosysd/my-btc-bot-5min）
# ==========================================================================
set -euo pipefail

REPO="${JYBOT_REPO:-leosysd/my-btc-bot-5min}"
DIR="${JYBOT_DIR:-$HOME/jybot-rs}"
RAW="https://raw.githubusercontent.com/${REPO}/main"
REL="https://github.com/${REPO}/releases/latest/download"

echo "=========================================================================="
echo "  jybot-rs 远程安装   repo=${REPO}"
echo "  安装目录: ${DIR}"
echo "=========================================================================="

if ! command -v curl >/dev/null 2>&1; then
  echo "ERROR: 需要 curl。请先安装：sudo apt install -y curl" >&2
  exit 1
fi

# 平台 → 预编译资产名 / 二进制相对路径
OS="$(uname -s 2>/dev/null || echo unknown)"
case "${OS}" in
  Linux*)  ASSET="jybot-rs-x86_64-linux";        BIN="target/release/jybot-rs" ;;
  Darwin*) echo "macOS 暂无预编译，请用源码: git clone + bash install.sh" >&2; exit 1 ;;
  *)       ASSET="jybot-rs-x86_64-windows.exe";   BIN="target/release/jybot-rs.exe" ;;
esac

mkdir -p "${DIR}/target/release" "${DIR}/scripts"

echo "[1/5] 下载预编译二进制 (${ASSET}) ..."
if ! curl -fL --retry 3 -o "${DIR}/${BIN}" "${REL}/${ASSET}"; then
  echo "ERROR: 下载二进制失败（可能还没发布 release）。" >&2
  echo "       可改用源码安装：git clone https://github.com/${REPO}.git && cd <dir> && bash install.sh" >&2
  exit 1
fi
chmod +x "${DIR}/${BIN}" 2>/dev/null || true

echo "[2/5] 下载运行所需文件 (.env.example / place_order.py) ..."
curl -fsSL -o "${DIR}/.env.example" "${RAW}/.env.example"
curl -fsSL -o "${DIR}/scripts/place_order.py" "${RAW}/scripts/place_order.py" || true

echo "[3/5] 生成 .env ..."
if [ ! -f "${DIR}/.env" ]; then
  cp "${DIR}/.env.example" "${DIR}/.env"
  echo "      已生成 ${DIR}/.env（请编辑填写）"
else
  echo "      .env 已存在（保留）"
fi

echo "[4/5] 安装全局命令 jybot ..."
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/jybot" <<EOF
#!/usr/bin/env bash
cd "${DIR}" && exec "${DIR}/${BIN}" "\$@"
EOF
chmod +x "$BIN_DIR/jybot"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "      已安装: jybot（$BIN_DIR 在 PATH 中）" ;;
  *) echo "      已安装: $BIN_DIR/jybot —— 加入 PATH:"
     echo "        echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
esac

echo "[5/5] 校验配置 ..."
( cd "${DIR}" && "${DIR}/${BIN}" check ) || true

echo "=========================================================================="
echo "  完成！下一步："
echo "    1) 编辑配置:  nano ${DIR}/.env"
echo "    2) 打开面板:  jybot      （5 启动服务 · 9 切换DRY_RUN · 12 更新）"
echo "  以后更新：面板按 12（自动拉最新预编译二进制，免编译）。"
echo "=========================================================================="
