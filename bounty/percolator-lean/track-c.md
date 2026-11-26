# Track C — lean-svm Compiler + Solana Runtime Boundary Audit

Scope: everything the Lean proofs structurally cannot cover — the lean-svm compiler
translation (LCNF → SbfIR → sBPF) and the Solana runtime/account boundary that the
verified model abstracts away.

Targets: bounty DEX `6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH`,
bounty market `76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq` (70 USDC insurance pot).
Method: read-only source audit of the local clones. No builds, no transactions.

Files audited in full: `svm/LeanSVM/Input.lean`, `Math.lean`, `CPI.lean`, `Types.lean`,
`Runtime.lean`, `Compiler/IR.lean`, `Compiler/Optimize.lean`, `Compiler/CodeGen.lean`,
`Compiler/RegAlloc.lean`, `Compiler/Driver.lean`, `Compiler/Lower.lean`,
`lean/Percolator/Layout.lean`, `lean/Percolator/Program.lean`,
`lean/Percolator/Spec.lean` (insurance transitions + arithmetic basis),
`repo/src/lean/constants.ts`, `repo/src/lean/parsers.ts`.

---

## C-1 — CRITICAL: Forged insurance shares via `closeAccount` + `depositInsurance` receipt-init (cross-layout type confusion at byte offset 80)

### Evidence

`handleDepositInsurance` initializes a receipt whenever its magic word is 0, and
writes every receipt field **except `shares`**:

`lean/Percolator/Program.lean:679-683`
```lean
    if (← loadU32 rData InsOff.magic) == 0 then
      storeU32 rData InsOff.magic MAGIC_INSURANCE
      storeU64 rData InsOff.marketId mid
      pubkeyCopy (rData + InsOff.owner)  (AccountView.keyPtr lp)
      pubkeyCopy (rData + InsOff.market) (AccountView.keyPtr mkt)
```
It then *adds* the minted amount to whatever already sits in the shares field:

`lean/Percolator/Program.lean:699`
```lean
    addU64At rData InsOff.shares         minted
```

The init path is reachable for any program-owned account with `dataLen >= 88` and
magic == 0 (`Program.lean:675-676`: owner == progId, `dataLen receipt >= InsOff.size`).
The only in-program way to obtain a program-owned, non-zero-filled account whose
magic is 0 is `handleCloseAccount`, which clears **only the 4 magic bytes** and
drains lamports, leaving all other data intact:

`lean/Percolator/Program.lean:1429-1433`
```lean
  storeU32 data 0 0
  let accLam  ← AccountView.lamports acc
  let destLam ← AccountView.lamports dest
  AccountView.setLamports dest (← checkedAdd64 destLam accLam)
  AccountView.setLamports acc 0
```

A fresh portfolio satisfies all close preconditions trivially (capital == 0,
legCount == 0, totalMm/totalIm/totalReservedMm == 0 — `Program.lean:1412-1417`),
and `handleInitPortfolio` is permissionless on any DEX (`Program.lean:577-588`).

The type confusion: in the portfolio layout, byte offset 80 is inside the owner
pubkey; in the receipt layout, offset 80 is the share balance:

`lean/Percolator/Layout.lean:170` — `PflOff.owner : UInt64 := 56   -- Pubkey (32)` (spans bytes 56..88)
`lean/Percolator/Layout.lean:199` — `InsOff.shares : UInt64 := 80  -- u64`

So a closed portfolio's bytes at offset 80 are **owner pubkey bytes 24..32** —
attacker-chosen key material, expected value ≈ 2^63, grindable arbitrarily high.

`handleWithdrawInsurance` then pays out against that balance
(`Program.lean:724-728`):
```lean
  let curSh ← loadU64 rData InsOff.shares
  SolanaM.require (curSh >= burnShares) PErr.insufficientFunds
  let totSh ← loadU64 mData MktOff.totalInsShares
  let ins   ← loadU64 mData MktOff.insurance
  let payout ← mulDiv64 burnShares ins totSh
```
With `burnShares = totSh`, `payout = ins` — the entire insurance balance. All
identity checks on the withdraw path (receipt owner/market/marketId,
`Program.lean:710-714`) pass because the init branch just wrote them consistently
from the attacker's own keys and the target market.

### Why the Lean proof does NOT cover it

The verified model has no byte layer and no account lifecycle. In `Spec.lean`,
share balances are a pure function in the state — `st.insShares mid lp`
(`stepWithdrawInsurance`, Spec.lean ~L300) — and can only come into existence via
`stepDepositInsurance`'s `+ minted`. There is no close/revive transition and no
representation of "the same account bytes previously held a different layout."
The init branch's soundness rests on an environmental assumption —
*magic == 0 ⟹ the account is freshly system-zeroed* — which is true for accounts
created by the system program but **false** for accounts recycled through the
program's own `closeAccount`, whose 4-byte magic wipe is precisely what makes the
closed account pass the `magic == 0` test while retaining attacker-controlled
bytes at the shares offset. That assumption lives entirely at the runtime
boundary the model abstracts away.

### Exploit sketch (against the bounty market)

1. Attacker generates a keypair `K` (optionally ground so `K[24..32]` is a huge
   u64; unground expected value ≈ 2^63 already dwarfs any realistic
   `totalInsShares`).
2. `initPortfolio` on the bounty DEX with owner `K` → portfolio account `P`
   (952 bytes, program-owned, owner field = `K` at bytes 56..88).
3. One transaction:
   a. `closeAccount(P, authority = K, dest = attacker)` — magic zeroed, lamports
      drained; `P`'s data otherwise intact (owner field still `K`).
   b. `depositInsurance(dex, bountyMarket, receipt = P, lp = K, lpTok, mint,
      vaultPda, tokProg, amount = 1)` — receipt checks pass (owner == program,
      952 ≥ 88, magic == 0) → init branch writes magic/marketId/owner/market,
      leaves `shares = K[24..32] ≈ 2^63`, then adds `minted = 1`.
      Attacker pays 1 unit (0.000001 USDC) into the vault.
   c. System-program transfer topping `P` back up to rent-exempt, so the
      0-lamport account is not purged at transaction end.
4. Second transaction: `withdrawInsurance(burnShares = market.totalInsShares)`.
   Preconditions: bounty market not tombstoned, `oiLong == oiShort ==
   rawBasisLong == rawBasisShort == 0` (`Program.lean:720-722`) — expected true
   for an idle bounty pot; **parent should confirm on-chain**. Payout =
   `mulDiv64(totSh, ins, totSh) = ins` = the full insurance balance, including
   the 70 USDC pot, transferred from the vault PDA to the attacker's token
   account (`Program.lean:731-739`).

Cost: 1 USDC unit + portfolio rent (recoverable) + tx fees. No admin, no oracle,
no trading required.

### Confidence

**Confirmed-by-reading.** Every step is pinned to the lines above; the only
runtime facts relied on are standard Solana semantics (account data persists
across instructions within a transaction; zero-lamport purge happens at
transaction end and is averted by the top-up). A devnet PoC would make it
end-to-end certain but is not needed to establish the code path.

---

## C-2 — HIGH: Insurance shares mint at par whenever `totalInsShares == 0` **or** `insurance == 0` (donation bootstrap + post-drawdown re-mint)

### Evidence

`lean/Percolator/Program.lean:690-691`
```lean
    let totSh ← loadU64 mData MktOff.totalInsShares
    let ins   ← loadU64 mData MktOff.insurance
    let minted ← if totSh == 0 || ins == 0 then pure amount else mulDiv64 amount totSh ins
```

Two exploitable states:

**(a) Donation bootstrap.** The permissionless donate path credits insurance
without minting any shares (`Program.lean:668-673`):
```lean
  if donate != 0 then
    -- Permissionless permanent insurance donation (0 LP shares minted!)
    tokenTransferChecked tokProg lpTok mint vaultPda lp amount dec
    addU64At dData DexOff.vaultBalance   amount
    addU64At dData DexOff.totalInsurance amount
    addU64At mData MktOff.insurance      amount
```
If a market's pot was seeded by donation — `ins > 0, totSh == 0` — the first
non-donate depositor mints **1:1 at par**: deposit 1 unit → 1 share → withdraw
with `burnShares = 1` pays `mulDiv64 1 ins 1 = ins`, the whole pot. If the
bounty market's 70 USDC was seeded this way, this is a 1-unit drain with no
account trickery at all. **Parent: read the live market account at
`MktOff.insurance` (offset 224) and `MktOff.totalInsShares` (offset 248) to
determine whether this variant is live.**

**(b) Par re-mint after insurance drawdown.** Liquidations draw on insurance
(`handleLiquidate`, `Program.lean:1024+`); if a draw takes `ins` to exactly 0
while `totSh > 0`, the `ins == 0` disjunct re-arms par minting: depositing
`amount = totSh` then captures 50% of all future insurance inflows (trade fees
credit the pot); `amount = 100 × totSh` captures ~99% — expropriating every
existing LP whose shares were priced against a funded pot.

### Why the Lean proof does NOT cover it

The flawed rule is *in* the specification. `Spec.lean` `stepDepositInsurance`
(~L289) contains the identical formula:
```lean
      let minted :=
        if m.totalInsShares = 0 ∨ m.insurance = 0 then amount
        else (amount * m.totalInsShares) / m.insurance
```
The proved invariant is stock conservation — `C_tot + I_tot + R_tot + S_tot = V`
(stated in `repo/src/lean/parsers.ts:185` and throughout `Spec.lean`) — which
**still holds** after this drain: the vault decreases by exactly the payout.
Conservation says nothing about per-LP entitlement; a share-pricing rule that
hands a donated or refilled pot to the next 1-unit depositor satisfies every
proved property while violating the economic intent. Verification of a spec
cannot flag that the spec itself misprices shares. (Partial mitigation already
present: `require (minted > 0)`, `Program.lean:692`, blunts the classic
first-depositor inflation attack against small victim deposits — it does
nothing against (a) or (b), where the attacker is the beneficiary.)

### Exploit sketch

Variant (a), if live on the bounty market: `depositInsurance(amount = 1,
donate = 0)` → receive 1 share → `withdrawInsurance(burnShares = 1)` → receive
the full `ins` (70 USDC). Variant (b) requires a market whose insurance was
drawn to 0 with shares outstanding — opportunistic, not directly forceable
against the bounty market.

### Confidence

**Confirmed-by-reading** as code/spec paths; variant (a)'s applicability to the
bounty market is **live-state-dependent** (needs the on-chain
`insurance`/`totalInsShares` read above).

---

## C-3 — MEDIUM: Compiler gives outlined functions no stack-frame isolation — callee stack writes overlap caller stack at identical `r10` offsets

### Evidence

Each function gets a private bump-allocated stack region anchored at the same
`r10`, with offsets assigned from 0:

`svm/LeanSVM/Compiler/Lower.lean:119-128` (`allocStack`)
```lean
  let aligned := ((size + 7) / 8) * 8
  let st ← get
  let newTotal := st.stackAllocBytes + aligned
  ...
  emit (.stackAddr r (Int16.ofInt (-(newTotal : Int))))
```

When a helper is outlined into a separate sBPF function, its stack state starts
over from zero:

`svm/LeanSVM/Compiler/Lower.lean:971`
```lean
                          stackAllocBytes := 0, callRetSlotOff? := none,
```

Spill slots are placed immediately above each function's *own* `stackAllocBytes`
(`Compiler/RegAlloc.lean`, `runLinearScan`: `nextSpillOff` initialized to
`((baseStackBytes + 7) / 8) * 8`, first spill at that +8). `CodeGen.lean`
addresses all stack slots as `r10 + off` (`emitStmt` `.stackAddr` case: `mov dr,
r10; add dr, stackOff`) and emits **no frame-pointer adjustment on call/return**
anywhere. The reported program stack usage is the **max**, not the call-chain
sum:

`svm/LeanSVM/Compiler/CodeGen.lean:443-451`
```lean
  let mut maxStack := entryAlloc.totalStackBytes
  ...
    maxStack := max maxStack hAlloc.totalStackBytes
```

So when function A calls outlined helper H, H's `stackAlloc` buffers and spill
slots occupy the *same addresses* as A's own stack region. Any A-value held in a
spill slot (or in a `stackAlloc` buffer) that is live across the call can be
silently overwritten by H. The register allocator protects call-crossing values
only by *preferring* callee-saved `r6..r9` (4 registers,
`Compiler/RegAlloc.lean` `runLinearScan` pool logic); a call-crossing interval
that loses that race is spilled into the collision zone with no diagnostic.

### Why the Lean proof does NOT cover it

In the verified model, `stackAlloc` is a single global bump allocator over one
`SvmState`: `svm/LeanSVM/Runtime.lean:221-222` — `let nextSp := s.stackSp -
aligned; .ok nextSp { s with stackSp := nextSp }` — allocations monotonically
descend and are **never reused**, including across (model-level) function calls.
The compiled binary instead reuses one fixed region per function with no
interprocedural disjointness argument. The model's semantics and the binary's
stack discipline are different memory models; no proof artifact in the repo
bridges them (the 4 KB check in `allocStack`/`getOrAllocCallRetSlot` is
per-function, matching the max-not-sum reporting).

### Exploit sketch / impact

No concrete corrupting instance is confirmed in this program — the dangerous
pairs appear *accidentally* safe: (i) RegAlloc places a function's spills below
its full stackAlloc high-water mark, and the entry function's region is large
(CPI buffers), putting its spills far below the small outlined helpers'
(`mulDiv128Slow`, `div128Loop`, `isqrt64`) write ranges; (ii) fallible-helper
return slots and arg packs are written by the callee *last* / read by the callee
*first*, so self-overlap is temporally benign; (iii) `mulDiv64` keeps only two
values (`buf`, `denom`) live across its `mulDiv128Slow` call, fitting the
callee-saved pool. But this safety is an emergent property of one allocation
outcome, not a compiler guarantee: any source change that pushes a hot handler
or math helper past 4 call-crossing live values, or grows a helper's stack
region into a caller's spill range, silently corrupts state in a
formally-verified binary. Against the bounty market this is a latent
miscompilation class rather than a demonstrated drain.

### Confidence

Mechanism **confirmed-by-reading**; presence of a corrupting instance in the
deployed binary **needs-compiler-artifact** (the compiler supports IR/asm dumps
— `Compiler/Driver.lean` `CompileOptions.dumpIR/dumpAsm` — a spill-location dump
for the handlers vs. the outlined helpers settles it) or a devnet PoC.

---

## C-4 — LOW (latent): Shift semantics diverge between the Lean model and sBPF — amounts are masked (mod 64), not zero-filled

### Evidence

Lowering maps Lean shifts directly onto sBPF shifts with no emulation of Lean
semantics:

`svm/LeanSVM/Compiler/Lower.lean:504-505`
```lean
  | ``UInt64.shiftLeft  => some <$> lowerBinScalar .lsh64 none (vals.getD 0 .erased) (vals.getD 1 .erased)
  | ``UInt64.shiftRight => some <$> lowerBinScalar .rsh64 none (vals.getD 0 .erased) (vals.getD 1 .erased)
```

The compiler's own constant evaluator implements sBPF masking semantics:

`svm/LeanSVM/Compiler/IR.lean:109-111`
```lean
    | .lsh64  => some (a <<< (b &&& 63))
    | .rsh64  => some (a >>> (b &&& 63))
    | .arsh64 => some (a.toInt64 >>> (b &&& 63).toInt64).toUInt64
```

And the optimizer rewrites a constant shift by any multiple of 64 into an
identity move:

`svm/LeanSVM/Compiler/Optimize.lean:245-247`
```lean
          | .lsh64 | .rsh64 | .arsh64 =>
            if (cr &&& 63) == 0 then changed := true; aliases := aliases.insert d l; s1 := .mov d l
```

Lean's `UInt64.shiftLeft x n` is 0 for `n >= 64`; sBPF `lsh64` computes
`x << (n & 63)`. For `n = 64` the model says 0, the binary (and the optimizer's
rewrite) say `x`. Any present or future code path — including inlined Lean
library code — that shifts by a data-dependent amount, or by a constant ≥ 64,
executes differently from the verified model.

### Why the Lean proof does NOT cover it

The proofs are about Lean-level `UInt64` operations. The compiler's semantic
contract silently substitutes the target ISA's masking behavior at three
independent sites (lowering, constant folding, optimizer rewrite). Nothing in
the toolchain warns when a shift amount can reach 64.

### Exploit sketch / impact

None against the current bounty binary: every shift in the program sources
(`Percolator/*.lean`, `Math.lean`, `Runtime.lean`, `Input.lean`, `CPI.lean`) has
a small constant right-hand side (verified by grep — all `<<<`/`>>>` occurrences
take numeric literals < 64). Recorded as a compiler-contract gap: the first
variable-shift that reaches this toolchain in a value path is a miscompilation
the proofs cannot see.

### Confidence

**Confirmed-by-reading** (mechanism); not exploitable in this program as
written.

---

## C-5 — LOW / informational: Input-layer accessors perform raw unchecked memory access; all bounds safety is call-site discipline

### Evidence

`svm/LeanSVM/Input.lean:54-76`: `readDataU8/U32/U64` and `writeDataU8/U32/U64`
are raw loads/stores at `acc + 88 + off` with **no check against
`AccountView.dataLen`** (defined at `Input.lean:47` but unused by the
accessors). The verified model cannot express an out-of-bounds access at all —
`SvmState.mem` is a total function `UInt64 → UInt8` (`Runtime.lean`), so the
model never faults where the binary reads/writes neighboring serialized
accounts or input-buffer padding.

In this program the gap is held by discipline: every state-account read/write
goes through handlers that first run `checkOwnedMagic` (owner + `dataLen >=
layout size` + magic) or the explicit receipt length check
(`Program.lean:676`), and all offsets are layout constants within the checked
size (leg indexing is capped by `MAX_LEGS` before use). Extended accounts
(`dataLen` greater than the layout) pass the `>=` checks and their tail bytes
are ignored — with the single exception of C-1, which is about *content*, not
length.

### Confidence

**Confirmed-by-reading**; no exploitable instance found in this program beyond
C-1. Recorded because the safety invariant is per-call-site convention, not a
property of the `Input` layer: one future handler that reads before checking
length is an out-of-bounds read/write the model cannot represent.

---

## Checked and held

- **Math.lean 128-bit arithmetic** — `mulDiv64` / `mulDiv128Slow` / `div128Loop`
  read line-by-line: schoolbook 128-bit product and restoring division are
  correct; the `hi < denom` precondition is enforced before the fast path.
  `mulDiv64Ceil`'s slow-path remainder argument (comparing `q*denom == a*b`
  mod 2^64) is sound because the true remainder is `< denom < 2^64`. Held.
- **Signed-division rounding class (hunting ground 3)** — does not exist here.
  Program, Math, and Spec use only unsigned arithmetic; PnL is signed-via-
  biased-K (`K_ZERO = 2^62`) in `UInt64`. Grep-verified: no `Int`, no
  `Int64`, no `ediv` anywhere in `Program.lean`/`Spec.lean`. The Spec's
  arithmetic basis is unbounded `Nat` with floor division (e.g. `Spec.lean:25-43`),
  matching the program's floor-oriented `mulDiv64` / ceil-oriented
  `mulDiv64Ceil` split. Held.
- **Oracle/K rounding drift** — `updateOracle` and `settleAndRefreshLeg` round
  the gaining side's K increment down (floor) and the losing side's decrement up
  (ceil): a ≤1-K-unit-per-round-trip drift that accrues to the pool, is
  uniformly protocol-favorable, and is not attacker-extractable. Held.
- **CPI token-program pinning** — the CPI layer takes the token program id from
  the passed account (`CPI.lean` `cpiExecTransferChecked`), but every value path
  in `Program.lean` (deposit/withdraw collateral, deposit/withdraw insurance)
  first runs `checkDexVaultAndMint`, and `checkTokenProg` pins the account's key
  against the canonical SPL Token program words in `Layout.lean`. Token-2022
  substitution fails the pin. Held (the already-known "A0 class" is excluded
  per instructions; this confirms the remaining paths).
- **Vault/mint validation on every path** — `checkDexVaultAndMint` (mint key ==
  `dex.quoteMint`, vault key == `dex.vaultPda`) is called on deposit, withdraw,
  deposit-insurance, and withdraw-insurance alike, not just deposit. PDA signer
  seeds are `[TAG_DEX_VAULT, dex key (32B), bump]` and the runtime enforces
  seed→key derivation on `invoke_signed`. Held.
- **Layout consistency (hunting ground 6)** — `Layout.lean` vs
  `repo/src/lean/constants.ts`: identical sizes (`DEX_SIZE 200`, `MARKET_SIZE
  352`, `PORTFOLIO_SIZE 952`, `INS_RECEIPT_SIZE 88`) and identical offsets for
  every `DexOff`/`MktOff`/`PflOff`/`LegOff`/`InsOff` field checked;
  `parsers.ts` reads through the same offset constants (e.g. receipt shares at
  `InsOff.shares`, `parsers.ts:137`). No parse-confusion mismatch. (Cosmetic:
  a Layout comment says "344 bytes" for the market while `MktOff.size := 352`
  and TS agree on 352 — comment only.) Held.
- **Duplicate-account `AccountView` forgery** — `nextCursor` (`Input.lean:81+`)
  strides 8 bytes over duplicate stubs, so a dup entry's key/owner/data
  pointers land inside the following account's serialized bytes. Forging a view
  that passes `checkOwnedMagic` would require 32 controlled bytes spanning the
  next account's key tail + owner head equal to the program id — infeasible;
  dup views passed as CPI accounts fail runtime account resolution. Held.
- **Collateral aliasing (vault as source)** — passing the vault PDA as the
  source token account in a deposit fails SPL Token's own authority check (the
  vault is not owned by the signer). Held by SPL.
- **Role-confusion aliasing sampled** — state accounts in swapped roles fail
  magic differentiation (each layout has a distinct magic); `closeAccount`
  additionally enforces `acc != dest` (`Program.lean:1406`). The one aliasing
  class that survives is C-1 (same account, *sequential* roles across two
  instructions), which is a lifecycle bug, not a role swap. Held otherwise.
- **Optimizer immediate conversions** — `fitsInt32` (`SBPF/Peephole.lean:12-20`)
  accepts only values that are the exact sign-extension of an `Int32`
  (`v <= 0x7FFFFFFF` or `v >= 0xFFFFFFFF80000000`); all `binImm`/`brCmpImm`
  conversions in `Optimize.lean` go through it, and the div/mod-by-power-of-2
  strength reductions are unsigned-correct (`rsh` for `div`, `and (2^k - 1)`
  for `mod`, `lsh` for `mul`). The one unsound rewrite found is the shift
  identity in C-4. Held otherwise.
- **Optimizer dataflow rewrites** — load CSE and store-to-load forwarding are
  invalidated on *any* store/call/syscall; SSA single-assignment makes the
  flow-insensitive constant/alias/add-offset maps sound; spill-slot reuse in
  `RegAlloc` is gated on the freeing interval's end position. No unsound
  instance found. Held.
- **Recursion / call depth** — tail recursion is compiled to in-place loops
  (`Compiler/Lower.lean` activeRecLoops path; `skipAccounts` included), and
  non-tail recursion is a compile-time error, not a runtime hazard. No sBPF
  call-depth exposure from per-account recursion. Held.
- **Nat-op lowering** — `Lower.lean` maps `Nat.add/sub/mul/div/mod` to 64-bit
  ops (Lean `Nat` is unbounded and `Nat.sub` saturates; `sub64` wraps). No
  runtime `Nat` arithmetic on attacker-influenced values was found in the
  program (counts/lengths are `UInt64` end-to-end), so the divergence class is
  latent only. Held for this program.
- **Close of a tombstoned market by a third party** — `handleCloseAccount`'s
  market branch skips the admin check when the stored admin is the zero key;
  impact is limited to reclaiming the market account's rent lamports to an
  arbitrary destination (~0.003 SOL), by-design rent cleanup, not the pot.
  Noted, not a finding.
