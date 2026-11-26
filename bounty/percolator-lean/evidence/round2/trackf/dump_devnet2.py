import json, subprocess, base64, hashlib

def rpc(url, method, params):
    body = json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params})
    out = subprocess.run(["curl","-s","-A","Mozilla/5.0","-X","POST",url,"-H","Content-Type: application/json","-d",body],
                         capture_output=True, text=True, timeout=120)
    d = json.loads(out.stdout)
    if "error" in d: raise RuntimeError(str(d["error"]))
    return d["result"]

def b58encode(b):
    alpha = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n = int.from_bytes(b, "big"); s = ""
    while n: n, r = divmod(n, 58); s = alpha[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + (s or "")

URL = "https://api.devnet.solana.com"
prog = "GD43BhWwAjiZLs56juEkacSKkY3TVCTbD7cG7HHDYKdA"
acct = rpc(URL, "getAccountInfo", [prog, {"encoding":"base64"}])["value"]
raw = base64.b64decode(acct["data"][0])
pd = b58encode(raw[4:36])
pdacct = rpc(URL, "getAccountInfo", [pd, {"encoding":"base64"}])["value"]
praw = base64.b64decode(pdacct["data"][0])
elf = praw[45:]
open("/tmp/trackf/core-devnet.so","wb").write(elf)
print("devnet core programdata:", pd, "elf bytes:", len(elf), "sha256:", hashlib.sha256(elf).hexdigest())
