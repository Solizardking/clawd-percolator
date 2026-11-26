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
for label, addr, kind in [("LP portfolio 3XsXCzHZ", "3XsXCzHZwbt4cKGwsudNK85Wo4NkJumvjfFgTJdStxeu", "pfl"),
                          ("other market 8ghSDBqr", "8ghSDBqrPMj9u1x8F9jZiULqnAsx9ZhQhWDWRskWyc7x", "mkt"),
                          ("bounty long pfl HamaLhsC", "HamaLhsCpnS6RA3zGESdU1uCk9xuwRML32ko4HC5sr9a", "pfl"),
                          ("bounty short pfl BTD2y4ho", "BTD2y4hoWsbt6ZSkf2QAm3hsVzZu9jom8Wwzvq8Dje96", "pfl")]:
    a = rpc("getAccountInfo", [addr, {"encoding": "base64"}])["value"]
    b = base64.b64decode(a["data"][0])
    if kind == "pfl":
        print(label, "| dex:", pk(b, 24), "| owner:", pk(b, 56), "| capital:", u64(b, 88))
    else:
        print(label, "| dex:", pk(b, 16), "| marketId:", u64(b, 8), "| insurance:", u64(b, 224), "| oiL/oiS:", u64(b, 256), u64(b, 264))
