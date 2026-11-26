# Track D — Round-2 patch diff + incomplete-fix audit (C-1)

**Scope:** read-only analysis of the round-2 core ELF + local PoC only. No live
transaction was sent or signed. All patch conclusions below are
**confirmed-by-disassembly** (hand-written sBPF disassembler over the full
8,113-instruction `.text`), **not** LiteSVM-PoC-confirmed — see §5 for why.

## 0. Artifacts

| Item | Value |
|---|---|
| Core program | `GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm` |
| Programdata | `2nXzb69CFjK2DYXU951YcgoNvY3m1JvV2eC4pEgKCtBK` |
| Round-2 ELF | 73,224 B, sha256 `5f9e564b0db3802033dbe3d2c945a4075eed33f3edf7839474c3e7ffd6996f3b` (re-fetched and re-verified this session via Helius `getAccountInfo`, 45-byte loader header stripped) |
| Round-1 ELF | 73,224 B, sha256 `c6dc8329…` — **unrecoverable**: the devnet twin program `GD43BhWwAjiZLs56juEkacSKkY3TVCTbD7cG7HHDYKdA` (programdata `AsDs4QULtw7AdxakRvFKh82spAJpWU4MbkiJUh3tWPa1`) was also upgraded and now holds a different 76,912 B ELF, sha256 `5d8d330e8e3393b4aca9ee0e705984f38551f696a6f319b281bf2db97192d034` (verified via public devnet RPC this session). No local dump of the round-1 ELF exists outside the security-flagged tree. **A literal byte-diff was therefore impossible**; the patch was characterized by auditing the new ELF directly against the round-1 findings (WRITEUP.md, reports/C-1-exploit-and-leads.md, track-c.md) and the WRITEUP §5 recommended fix. |

## 1. What the patch changed (vs. the round-1 behavior documented in the findings)

The dispatcher sits at pc 8019 (tag byte → thunk → handler). Handler entry
points in the round-2 ELF: CloseAccount 62, TombstoneMarket 379,
ClaimResolved 635, ResolveMarket 894, ForceClosePair 1636, StartWindDown 2212,
ClearDeadLeg 2304, Liquidate 2937, Trade 3943, RefreshLeg 5311,
WithdrawInsurance 5542, DepositInsurance 5851, WithdrawCollateral 6234,
DepositCollateral 6474, InitPortfolio 6692, UpdateOracle 6771,
UpdateMarketAuth 7044, CreateMarket 7173, UpdateCreateFee 7500, InitDex 7844.
(pc = instruction index in `.text`; ELF file offset = `0xb0 + 8·pc`.)

A shared zero-fill helper (memset-zero over N dwords, entry pc 53) and a shared
checked `addU64At` helper (entry ≈ pc 572) are called throughout. The patch is
a **systematic zero-on-init / zero-on-close fix**, not a one-line patch:

### 1a. CloseAccount — BOTH halves of the WRITEUP §5 fix, plus a tombstone discriminator
Round 1: `storeU32 data 0 0` (zeroed only the 4-byte magic), then drained lamports.
Round 2 (handler pc 62):
- Type dispatch on the data magic word: `PFLO` 1330398800 (dataLen must be 952),
  `INSR` 1381191241 (dataLen 88), `MKTP` 1347701581 (dataLen 352); anything else → error.
- Per-type authority checks, plus emptiness gates: a portfolio may close only if
  data fields at +184/+192/+200/+208 and the byte at +100 are all zero
  (pc 284–294); a receipt may close only if `shares` (data+80, read at pc 246
  via `[r6+176]`) is zero.
- pc 295–297: calls the zero-fill helper with r3 = 44 / 11 / 119 dwords by type
  → **the entire account data (352 / 88 / 952 bytes) is zeroed**.
- pc 298: then writes magic `1145389380` = **"DEAD"** at data+0 — a CLOSED
  discriminator on top of the zeroing.
- pc 299–307: lamports drained to the destination, account lamports set to 0.

### 1b. DepositInsurance — init branch now zeroes the whole receipt before writing fields
Round 1: init branch (magic == 0) wrote magic/marketId/owner/market but **not**
`shares`, then `addU64At(shares, minted)` accumulated onto stale bytes (C-1).
Round 2 (handler pc 5851):
- Receipt validation is inline at pc 6055–6065: dataLen must be exactly 88;
  magic == 0 → init branch; magic == `INSR` → owner/market validation;
  **any other magic (including "DEAD") → error 100**. A closed account can no
  longer be revived through DepositInsurance at all.
- Init branch pc 6138–6154: **calls the zero-fill helper with r3 = 11 dwords
  (88 bytes) over the receipt data first**, then writes `INSR` magic (pc 6142),
  marketId (pc 6144), owner pubkey (pc 6145–6149), market pubkey (pc 6150–6154).
- The final `addU64At(receipt, 80, minted)` (pc 6208–6211) therefore always
  accumulates onto a zeroed (fresh init) or previously validated (live INSR)
  shares field. This is WRITEUP fix (1) in a stronger form (full zero instead
  of just `storeU64 shares 0`).

### 1c. Every other init path also zeroes first (sibling coverage)
Zero-fill helper call sites (exhaustive over the ELF):

| Handler | pc | Dwords | Bytes | Account |
|---|---|---|---|---|
| CloseAccount | 297 | 44/11/119 by type | full | all three types |
| DepositInsurance (init) | 6141 | 11 | 88 | insurance receipt |
| InitPortfolio | 6749 | 119 | 952 | portfolio |
| CreateMarket | 7421 | 44 | 352 | market |
| InitDex | 7971 | 25 | 200 | dex |
| ResolveMarket | 1627 | — | — | (zeroes a resolve-side buffer/account; not C-1 relevant) |

InitPortfolio detail (pc 6692): its pre-check helper (pc 6638) requires
dataLen == 952 **and magic == 0 exactly** (pc 6674–6678) — a "DEAD" account is
rejected — and the handler then zeroes all 952 bytes (pc 6745–6749) before
writing `PFLO`, legCount 0, and the owner pubkey at data+56 (pc 6759–6763).
CreateMarket zeroes 352 B before writing `MKTP` (pc 7417–7422). InitDex zeroes
200 B before writing the DEX magic (pc 7970–7972).

### 1d. What did NOT change
- **Par minting (Lead 2 / C-2) is still the rule**: DepositInsurance pc 6155–6170
  computes `minted = amount` when `totalInsShares == 0 || insurance == 0`, else
  `mulDiv(amount, totalInsShares, insurance)` — identical to round 1
  (Program.lean L688–691; also in Spec.lean, i.e. spec-level). Not armed on the
  round-2 market as seeded (insurance == totalInsShares == 70,000,000).
- The donate path (donate=1) still mints 0 shares and only adds to
  insurance/dex totals (pc 5984–6014).
- WithdrawInsurance (pc 5542) semantics unchanged: validates `INSR`, requires
  the market OI/rawBasis fields all zero (pc 5733–5740), burn ≤ shares
  (data+80, read at pc 5744) and ≤ totalInsShares, pays `mulDiv(burn, insurance,
  totalInsShares)`, then subtracts shares/totalInsShares/insurance.
- DepositInsurance remains the **only** writer of the `INSR` magic (exhaustive
  store scan) and the **only** `addU64At` site targeting receipt offset 80
  (pc 6211). There is no second insurance-init entry point.

## 2. Is C-1 fully dead?

**Yes — confirmed by disassembly, with three independent layers, any one of
which suffices:**

1. **CloseAccount zeroes everything** and stamps "DEAD" — no stale bytes
   survive a close anymore (round-1 step 1 of the exploit is gone).
2. **DepositInsurance's init branch zeroes the full 88-byte receipt** before
   the shares add — even a hypothetical stale-bytes account (e.g. one closed
   under round-1 code, magic left 0) is wiped at init (round-1 step 2 is gone).
3. **The "DEAD" discriminator**: an account closed under round-2 code has
   magic `0x44454144`, which DepositInsurance rejects (≠ 0, ≠ `INSR` → error
   100) and InitPortfolio rejects (requires magic == 0). Closed accounts are
   terminal; they cannot be re-initialized in place at all.

**Surviving-variant search (all negative):**
- *Revive a round-2-closed account*: blocked by layer 3 at every init entry
  point (DepositInsurance, InitPortfolio; CreateMarket/InitDex operate on
  fresh PDAs/accounts with their own checks and zero-first regardless).
- *Revive a round-1-closed account* (magic 0, stale bytes, e.g. closed before
  the upgrade): DepositInsurance init zeroes first (layer 2); InitPortfolio
  zeroes first. No alias survives.
- *Sibling-field aliasing in other layouts*: every account type's init path
  zeroes its full data length before any checked add; the only
  add-to-stale-field pattern in the program was the C-1 shares add, and its
  offset-80 add site is now always preceded by zeroing or INSR validation.
- *Alternate insurance-init entry points*: none — INSR magic has exactly one
  store site (pc 6142) and shares exactly one add site (pc 6211), both in
  DepositInsurance.
- *CloseAccount as a weapon* (closing someone else's live receipt/portfolio to
  grief): close requires the per-type authority/owner checks and, for
  receipts, shares == 0, and for portfolios all balance fields zero — a funded
  position cannot be closed by anyone. No change from round-1 intent.

Confidence: **high** for the code paths above (full-coverage disassembly;
dispatcher, all 20 handlers, all zero-fill and magic-store sites enumerated).
Residual uncertainty is the usual disassembly caveat: control flow was read,
not executed — see §5.

## 3. Best surviving lead

None against the round-2 pot via the C-1 family. The remaining round-1 leads
are unchanged by this patch and were already assessed in round 1 as **not**
routes to the seeded pot:
- Lead 2 (par minting on `ins == 0 || totSh == 0`) — spec-level rule, not
  armed at the round-2 seed state (70M/70M); would need the market's insurance
  drained to zero first, which is the thing it would be trying to achieve.
- Lead 1 / Lead-001 (permissionless RefreshLeg forceHaircut; duplicate-account
  alias guards) — griefing class, no pot route per the round-1 replay.
- Lead 3 (discarded shortfall returns outside Liquidate) — accounting
  observation, unchanged.
- Lead 4 (first-depositor donation inflation) — the donate path is intact;
  against a market already seeded 1:1 it has no leverage on the pot.

## 4. Round-2 live-state note

WithdrawInsurance is additionally OI-gated right now (round-2 market OI
6,674,572 long / 6,674,572 short ≠ 0), so even a hypothetical forged-share
balance could not pay out until the market goes flat. Defense in depth holds
at the state level as well as the code level.

## 5. PoC status — why this is disassembly-confirmed, not PoC-confirmed

A LiteSVM run of the C-1 sequence (InitPortfolio → CloseAccount → revive →
DepositInsurance(1) → read shares @80 → WithdrawInsurance) against the new ELF
was **not executed**, for two independent reasons:
1. The only existing harness (the round-1 poc-runner, Rust/litesvm) lives in
   `~/workspace/percolator-cli-bounty/`, which is under a **security flag**:
   per the binding instruction for this task, nothing in that tree may be run,
   built, imported, or copied out. Plain-text reading only. (The identical
   runner file also sits in the bounty repo's `poc/`; running it from there
   would route around the flag, so it was not run either.)
2. A from-scratch harness was attempted and is **blocked by the VM's network
   egress**: a Rust toolchain does exist (`cargo`/`rustc` 1.79.0 at
   `~/workspace/.cargo/bin`, not on PATH), but `cargo fetch` for `litesvm`
   stalled repeatedly on crates.io downloads (transfers <10 B/s, timed out),
   so the dependency tree cannot be obtained in this environment. The home
   volume is also in an active EDQUOT fault state, which makes a multi-GB
   local build a poor risk regardless.

If a PoC is wanted, the cheapest route is on a machine with the Solana/Rust
toolchain: reuse the round-1 poc-runner shape against
`core_new.so` (sha256 `5f9e564b…`) — expected result per this audit:
DepositInsurance on the closed account fails with error 100 (DEAD magic), and
on a round-1-style zero-magic stale account it succeeds but yields
shares == 1 (init zeroed), so WithdrawInsurance pays out ~1 unit, not the pot.

## 6. Method note

ELF fetched via `helius_mainnet_rpc.py getAccountInfo` on the programdata
account (base64, 45-byte loader header stripped, hash verified against the
assignment). Disassembly: hand-written Python sBPF decoder (fixed 8-byte
instructions, relative-call resolution, `.rel.dyn` syscall annotation:
sol_log_64_, sol_get_clock_sysvar, sol_invoke_signed_c ×4 sites). Zero
UNKNOWN opcodes over 8,113 instructions. Devnet twin checked via public
devnet RPC.
