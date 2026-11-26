import json, subprocess, base64, hashlib, struct

HELPER = ["python3", "/home/hatch/workspace/custom-connectors/custom.legacy-helius/bin/helius_rpc.py", "--network", "devnet"]

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

for name, prog in [("core-devnet", "GD43BhWwAjiZLs56juEkacSKkY3TVCTbD7cG7HHDYKdA")]:
    acct = rpc("getAccountInfo", [prog, {"encoding": "base64"}])["value"]
    raw = base64.b64decode(acct["data"][0])
    pd = b58encode(raw[4:36])
    pdacct = rpc("getAccountInfo", [pd, {"encoding": "base64"}])["value"]
    praw = base64.b64decode(pdacct["data"][0])
    elf = praw[45:]
    open(f"/tmp/trackf/{name}.so", "wb").write(elf)
    print(name, "programdata:", pd, "elf bytes:", len(elf), "sha256:", hashlib.sha256(elf).hexdigest())
