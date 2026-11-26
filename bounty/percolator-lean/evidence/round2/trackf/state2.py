import json, subprocess, base64, struct

HELPER = ["python3", "/home/hatch/workspace/solana-rpc/helius_mainnet_rpc.py"]
def rpc(method, params):
    out = subprocess.run(HELPER + ["--method", method, "--params", json.dumps(params)],
                         capture_output=True, text=True, timeout=120)
    d = json.loads(out.stdout)
    if "error" in d: raise RuntimeError(str(d["error"]))
    return d["result"]

def b58encode(b):
    alpha = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n = int.from_bytes(b, "big"); s = ""
    while n: n, r = divmod(n, 58); s = alpha[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + (s or "")
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]
def pk(b, o): return b58encode(b[o:o+32])

MKT = "BhYaG2JYRHGrLpWs85TroedmkcG7toX1Ly5YbG1FDZHH"
print("slot:", rpc("getSlot", []))
acct = rpc("getAccountInfo", [MKT, {"encoding": "base64"}])["value"]
b = base64.b64decode(acct["data"][0])
for off, n in [(4,"status"),(8,"marketId"),(112,"markPrice"),(120,"lastOracleSlot"),(144,"maxStaleness"),
               (152,"warmupSlots"),(160,"imBps"),(168,"mmBps"),(176,"liqFeeBps"),(184,"tradeFeeBps"),
               (192,"maxMoveBps"),(200,"maxInsDrawBps"),(216,"realizedPool"),(224,"insurance"),
               (232,"feeSurplus"),(240,"totalPosPnl"),(248,"totalInsShares"),(256,"oiLong"),(264,"oiShort"),
               (336,"activeLegs")]:
    print(f"  {n} @{off} = {b[off] if off==4 else u64(b, off)}")
print("  dex =", pk(b, 16)); print("  oracleAuth =", pk(b, 48)); print("  marketAdmin =", pk(b, 80))

res = rpc("getProgramAccounts", ["GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm",
        {"encoding": "base64", "filters": [{"dataSize": 88}, {"memcmp": {"offset": 48, "bytes": MKT}}]}])
for a in res or []:
    rb = base64.b64decode(a["account"]["data"][0])
    print("  receipt", a["pubkey"], "owner:", pk(rb, 16), "shares:", u64(rb, 80))

res = rpc("getProgramAccounts", ["EFLgSpuiyXxHJHFAWCZtmzdcTXeHguiMSUyRHN3z3FDR", {"encoding": "base64"}])
for a in res or []:
    mb = base64.b64decode(a["account"]["data"][0])
    if mb[:4] == b'MTCH':
        print("  matcher cfg", a["pubkey"], "paused:", mb[5], "owner:", pk(mb, 8), "percProg:", pk(mb, 40),
              "lpPortfolio:", pk(mb, 72), "halfSpreadBps:", u64(mb, 104), "impactBps:", u64(mb, 112), "maxFillQty:", u64(mb, 120))
        lp = pk(mb, 72)
        la = rpc("getAccountInfo", [lp, {"encoding": "base64"}])["value"]
        if la:
            pb = base64.b64decode(la["data"][0])
            print("    LP pfl owner:", pk(pb, 56), "capital:", u64(pb, 88), "legCount:", pb[4])
            for i in range(8):
                lo = 120 + i * 104
                if pb[lo] == 1:
                    print(f"    leg{i}: side={pb[lo+1]} marketId={u64(pb,lo+8)} market={pk(pb,lo+16)} rawBasis={u64(pb,lo+48)} posPnl={u64(pb,lo+64)}")
