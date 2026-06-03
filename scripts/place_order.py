"""
scripts/place_order.py -- LIVE 下单助手（被 Rust 核心在实盘模式下调用）。

Rust 负责全部行情/盘口/策略判断（WebSocket）；真正"签名并提交订单"这一步
复用官方 py-clob-client。本脚本自包含，不依赖任何项目内 Python 包。

  * 从 stdin 读一行 JSON: {token_id, side, size, price, order_type}
  * 用 .env 凭证连接 CLOB，下限价单
  * 向 stdout 打印一行 JSON 结果: {status, filled_size, avg_price, order_id, reason}

所有日志走 stderr，stdout 只输出结果 JSON（Rust 解析 stdout）。

安全：自带 .env 解析 + 安全锁，必须 DRY_RUN=false 且 LIVE_TRADING=true 才下单。
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def err(msg: str) -> None:
    print(msg, file=sys.stderr, flush=True)


def out(obj: dict) -> None:
    print(json.dumps(obj), flush=True)


def load_env() -> None:
    """极简 .env 加载（无需 python-dotenv）。优先 JY_ENV_PATH。"""
    path = os.environ.get("JY_ENV_PATH")
    candidates = [path] if path else []
    candidates += [str(ROOT / ".env"), ".env"]
    for cand in candidates:
        if cand and os.path.exists(cand):
            try:
                for raw in Path(cand).read_text(encoding="utf-8", errors="ignore").splitlines():
                    line = raw.strip()
                    if not line or line.startswith("#") or "=" not in line:
                        continue
                    if line.startswith("export "):
                        line = line[len("export "):].strip()
                    k, _, v = line.partition("=")
                    k, v = k.strip(), v.strip()
                    if v[:1] in ("'", '"'):
                        q = v[0]
                        e = v.find(q, 1)
                        v = v[1:e] if e != -1 else v[1:]
                    else:
                        h = v.find(" #")
                        if h != -1:
                            v = v[:h].strip()
                    os.environ.setdefault(k, v)
            except Exception as e:
                err(f"[place_order] .env 解析失败: {e}")
            return


def getenv(*names, default=None):
    for n in names:
        v = os.environ.get(n)
        if v and v.strip() and v.strip().lower() not in ("0x...", "...", ""):
            return v.strip()
    return default


def is_true(name: str, default: bool) -> bool:
    v = os.environ.get(name)
    if v is None:
        return default
    return v.strip().lower() in ("1", "true", "yes", "y", "on")


def main() -> int:
    try:
        req = json.loads(sys.stdin.read())
    except Exception as e:
        out({"status": "FAILED", "filled_size": 0, "avg_price": 0, "reason": f"bad input: {e}"})
        return 1

    load_env()

    # 安全锁
    if not (is_true("LIVE_TRADING", False) and not is_true("DRY_RUN", True)):
        out({"status": "REJECTED", "filled_size": 0, "avg_price": 0,
             "reason": "safety lock: need DRY_RUN=false AND LIVE_TRADING=true"})
        return 0

    private_key = getenv("POLYMARKET_PRIVATE_KEY", "POLYMARKET_PK")
    api_key = getenv("POLYMARKET_API_KEY")
    api_secret = getenv("POLYMARKET_API_SECRET")
    api_pass = getenv("POLYMARKET_API_PASSPHRASE", "POLYMARKET_PASSPHRASE")
    funder = getenv("PM_FUNDER_ADDRESS", "POLYMARKET_FUNDER")
    missing = [n for n, v in [
        ("POLYMARKET_PRIVATE_KEY", private_key), ("POLYMARKET_API_KEY", api_key),
        ("POLYMARKET_API_SECRET", api_secret), ("POLYMARKET_API_PASSPHRASE", api_pass),
        ("PM_FUNDER_ADDRESS", funder),
    ] if not v]
    if missing:
        out({"status": "REJECTED", "filled_size": 0, "avg_price": 0,
             "reason": f"missing credentials: {','.join(missing)}"})
        return 0

    sig_type = int(getenv("POLYMARKET_SIG_TYPE", default="2"))
    chain_id = int(getenv("CHAIN_ID", default="137"))
    clob_url = getenv("CLOB_API_URL", default="https://clob.polymarket.com").rstrip("/")

    token_id = str(req.get("token_id", ""))
    side = str(req.get("side", "")).upper()
    size = float(req.get("size", 0) or 0)
    price = float(req.get("price", 0) or 0)
    order_type = str(req.get("order_type", "FOK")).upper()

    try:
        from py_clob_client.client import ClobClient
        from py_clob_client.clob_types import OrderArgs, OrderType, ApiCreds
        from py_clob_client.order_builder.constants import BUY, SELL
    except Exception as e:
        out({"status": "FAILED", "filled_size": 0, "avg_price": 0,
             "reason": f"py-clob-client not installed: {e}"})
        return 1

    try:
        client = ClobClient(host=clob_url, key=private_key, chain_id=chain_id,
                            signature_type=sig_type, funder=funder or None)
        client.set_api_creds(creds=ApiCreds(
            api_key=api_key, api_secret=api_secret, api_passphrase=api_pass))

        order_args = OrderArgs(token_id=token_id, price=price, size=size,
                               side=BUY if side == "BUY" else SELL, fee_rate_bps=0)
        signed = client.create_order(order_args)
        ot_map = {"FOK": OrderType.FOK, "FAK": getattr(OrderType, "FAK", OrderType.GTC),
                  "GTC": OrderType.GTC}
        resp = client.post_order(signed, orderType=ot_map.get(order_type, OrderType.FOK))
        err(f"[place_order] resp={resp}")

        order_id = (resp or {}).get("orderID") or (resp or {}).get("orderId")
        matched = float((resp or {}).get("makingAmount", 0) or 0)
        success = bool(resp and (resp.get("success") or order_id))
        status = "FILLED" if success and matched else ("RESTING" if success else "FAILED")
        out({"status": status, "filled_size": size if status == "FILLED" else 0.0,
             "avg_price": price, "order_id": order_id, "reason": str(resp)[:200]})
        return 0
    except Exception as e:
        out({"status": "FAILED", "filled_size": 0, "avg_price": 0, "reason": str(e)[:200]})
        return 1


if __name__ == "__main__":
    sys.exit(main())
