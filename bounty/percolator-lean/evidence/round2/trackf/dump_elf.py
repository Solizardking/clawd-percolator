import json, subprocess, sys, base64, hashlib, struct

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

for name, prog in [("core", "GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm"),
                   ("matcher", "EFLgSpuiyXxHJHFAWCZtmzdcTXeHguiMSUyRHN3z3FDR"),
                   ("oracle", "B4v96XfX4df9NtPbHjYsLTYkrjccGqWt4nsgyPpJbCSN")]:
    acct = rpc("getAccountInfo", [prog, {"encoding": "base64"}])["value"]
    raw = base64.b64decode(acct["data"][0])
    pd = b58encode(raw[4:36])
    pdacct = rpc("getAccountInfo", [pd, {"encoding": "base64"}])["value"]
    praw = base64.b64decode(pdacct["data"][0])
    slot = struct.unpack_from("<Q", praw, 0)[0]
    elf = praw[45:]
    path = f"/tmp/trackf/{name}.so"
    open(path, "wb").write(elf)
    print(name, "programdata:", pd, "deploy slot:", slot, "elf bytes:", len(elf),
          "sha256:", hashlib.sha256(elf).hexdigest())
