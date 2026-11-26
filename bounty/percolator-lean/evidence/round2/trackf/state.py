import json, subprocess, base64, struct

HELPER = ["python3", "/home/hatch/workspace/solana-rpc/helius_mainnet_rpc.py"]
def rpc(method, params):
    out = subprocess.run(HELPER + ["--method", method, "--params", json.dumps(params)],
                         capture_output=True, text=True, timeout=180)
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
slot = rpc("getSlot", [])
print("current slot:", slot)
acct = rpc("getAccountInfo", [MKT, {"encoding": "base64"}])["value"]
b = base64.b64decode(acct["data"][0])
print("market bytes:", len(b), "magic:", b[:4])
names = [(4,"status","B"),(8,"marketId","Q"),(112,"markPrice","Q"),(120,"lastOracleSlot","Q"),
         (144,"maxStaleness","Q"),(152,"warmupSlots","Q"),(160,"imBps","Q"),(168,"mmBps","Q"),
         (176,"liqFeeBps","Q"),(184,"tradeFeeBps","Q"),(192,"maxMoveBps","Q"),(200,"maxInsDrawBps","Q"),
         (208,"maxOi","Q"),(216,"realizedPool","Q"),(224,"insurance","Q"),(232,"feeSurplus","Q"),
         (240,"totalPosPnl","Q"),(248,"totalInsShares","Q"),(256,"oiLong","Q"),(264,"oiShort","Q"),
         (336,"activeLegs","Q"),(344,"minMm","Q")]
for off, n, fmt in names:
    v = b[off] if fmt == "B" else u64(b, off)
    print(f"  {n} @{off} = {v}")
print("  dex =", pk(b, 16))
print("  oracleAuth =", pk(b, 48))
print("  marketAdmin =", pk(b, 80))

# insurance receipts for this market (InsOff: market @48, shares @80, owner @16)
res = rpc("getProgramAccounts", ["GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm",
        {"encoding": "base64", "filters": [{"dataSize": 88}, {"memcmp": {"offset": 48, "bytes": MKT}}]}])
print("insurance receipts:", len(res or []))
for a in res or []:
    rb = base64.b64decode(a["account"]["data"][0])
    print("  receipt", a["pubkey"], "owner:", pk(rb, 16), "shares:", u64(rb, 80), "marketId:", u64(rb, 8))

# portfolios with legs on this market
res = rpc("getProgramAccounts", ["GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm",
        {"encoding": "base64", "filters": [{"dataSize": 952}]}])
print("portfolios total:", len(res or []))
mid = u64(b, 8)
for a in res or []:
    pb = base64.b64decode(a["account"]["data"][0])
    for i in range(8):
        lo = 120 + i * 104
        if pb[lo] == 1 and u64(pb, lo + 8) == mid and pk(pb, lo + 16) == MKT:
            print(f"  pfl {a['pubkey']} owner {pk(pb,56)} capital {u64(pb,88)} leg{i}: side={pb[lo+1]} rawBasis={u64(pb,lo+48)} posPnl={u64(pb,lo+64)} lastRefresh={u64(pb,lo+88)}")

# matcher cfg accounts
res = rpc("getProgramAccounts", ["EFLgSpuiyXxHJHFAWCZtmzdcTXeHguiMSUyRHN3z3FDR", {"encoding": "base64"}])
print("matcher program accounts:", len(res or []))
for a in res or []:
    mb = base64.b64decode(a["account"]["data"][0])
    print("  acct", a["pubkey"], "space:", a["account"]["space"], "head:", mb[:4].hex())
