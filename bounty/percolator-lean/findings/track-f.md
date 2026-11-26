# Track F — Round-2 re-validation of the round-1 non-C-1 leads (percolator-lean bounty)

Scope: re-check Track B F1 (matcher mode-byte) and Lead-001 / Track A F4
(RefreshLeg forceHaircut) against the round-2 deployment. READ-ONLY chain
reads + local static analysis only; no transaction sent or signed; no repo
code built or run (see "Method / constraints" — the LiteSVM step is held).

## Round-2 deployment (verified by read-only RPC, 2026-11-25 ~21:50–22:00 EST, slots 473,986,323–473,987,238)

- New market `BhYaG2JYRHGrLpWs85TroedmkcG7toX1Ly5YbG1FDZHH`, new vault
  `6k2H4GiRDGa7o9X8Ai99sGkPQGAvrAW7gVbKkAKuH2Fk` (70 USDC), insurance
  70,000,000, totalInsShares 70,000,000, OI 6,674,572 long / 6,674,572 short,
  activeLegs 2, realizedPool 0, totalPosPnl 0.
- Correction to the round-2 brief: the new market does **not** sit on the
  round-1 DEX `6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH`. Its stored
  `dex` field (MktOff.dex @16) is a fresh DEX account
  `CedQKVmmHuta5QcqRNkA96smV33LYtQBHo9wc1YcmFq3`, and both leg-holding
  portfolios are registered under that same DEX.
- Market parameters are identical to round 1: markPrice 71,165,607 E6
  (~$71.17), imBps 1000, mmBps 500, liqFeeBps 100, tradeFeeBps 0,
  maxMoveBps **400**, maxInsDrawBps **2500**, maxStaleness 9000,
  warmupSlots **0**.
- The single insurance receipt (`HRwLbjSzNMJipYrjafBUPgbtmx72tCF6R5UoGQyPAJZ9`)
  is held by the market admin `A3Mu2nQdjJXhJkuUDBbF2BdvgDs5KodNE9XsetXNMrCK`
  with all 70,000,000 shares — the house holds 100% of the insurance pool.
- All three programs were upgraded in place (same program ids, same ELF
  sizes, new hashes):

  | program | round 1 sha256 | round 2 sha256 | size |
  | --- | --- | --- | --- |
  | core `GVV4…KtYJsm` | `c6dc8329…a436d1` | `5f9e564b…96f3b` | 73,224 |
  | matcher `EFLg…3z3FDR` | `9d79d159…f54` | `ccb26bef…ff052` | 8,744 |
  | oracle `B4v9…JbCSN` | `4a49d558…a8d1a` | `7056ff17…434d` | 20,920 |

  (Round-1 hashes from `repo/lean-mainnet.json`; round-2 ELFs dumped from the
  live ProgramData accounts and hashed locally — they match the hashes in the
  tasking brief exactly.)

## What the round-2 patch actually changed (source diff)

The deployed round-2 source is the current `aeyakovenko/percolator-lean`
main (proven at the ELF level below). Diffing it against the round-1 source
tree (`~/workspace/percolator-cli-bounty/lean/Percolator/`, pin
`35cd13c`/`d591ab9`):

- `Program.lean` (+32 lines): a new `zeroWordsLoop` helper; **every Init
  handler now zeroes the full account** (DEX, market, portfolio, insurance
  receipt, freed legs) before writing fields; `checkOwnedMagic` /
  `checkFreshAccount` now require **exact** data length (`==` instead of
  `>=`); `WithdrawInsurance` gains a `curSh <= totSh` require;
  `handleCloseAccount` zeroes **all** bytes and writes a permanent
  `MAGIC_CLOSED` tombstone instead of only clearing the 4-byte magic.
  This is precisely the C-1 class: residual-byte / close-revive type
  confusion is closed at every init and close path.
- `Matcher.lean`: the same treatment inside `handleInitMatcher` only
  (new `zeroMatcherWords`, cfg zeroed on init, exact-length cfg checks).
- `Layout.lean`: one added constant, `MAGIC_CLOSED = 0x44454144`. No offset,
  opcode, or layout changes.
- **Neither `handleFill`, nor `handleRefreshLeg`, nor
  `settleAndRefreshLeg`, nor any haircut code appears anywhere in the diff.**

### ELF-level confirmation that the patched source is what is deployed

- Core: the `MAGIC_CLOSED` immediate `0x44454144` is present in the round-2
  core ELF (instruction immediate field at file offset 0xa04) — the
  tombstone write shipped.
- Matcher: the Fill handler's instruction stream matches the patched-source
  shape (exact-length/magic checks present) and its CPI build is identical
  in structure to round 1 (details under F1 below).

## Lead 1 — Track B F1 (matcher `Fill` forwards the Trade `mode` byte unchecked)

**Verdict: STILL PRESENT in round 2 — code unchanged. But NOT currently a
route to the round-2 pot: the only live matcher LP sits on a different DEX
than the bounty market, and the core's dex-consistency check blocks every
cross-DEX fill. It becomes a pot route only if a matcher LP is deployed on
the bounty market's DEX and takes a position.**

Source evidence (round-2 tree, `Percolator/Matcher.lean`):

- Line 204: `let mode ← loadU8 ix.ptr 1` — still read from caller-controlled
  instruction data.
- Line 240: `storeU8 data 1 mode` — still forwarded verbatim into the core
  Trade CPI.
- No range check on `mode` exists anywhere in `handleFill`; the only
  requires are paused / prog / LP-portfolio / market-magic / qty bounds /
  limit-price. The quote side and the pflL/pflS assignment are still chosen
  from `takerIsLong` alone, so the mode-2 inversion (taker extracts δ per
  fill in both orientations, δ clamped only by the market's maxMoveBps =
  400 on this market) is intact.

ELF evidence (round-2 matcher, sha256 `ccb26bef…`, sBPF disassembly):

- `0x0fc0: ldxb r0, [r7+9]` — mode byte loaded from instruction data (ix
  byte 1); `0x0fc8: stxdw [r10-584], r0` — spilled.
- The next loads are takerIsLong (`0x0fd0: ldxb r0, [r7+10]`) and qty
  (`0x0fe0: ldxdw r0, [r7+11]`); the only validation that follows is on qty
  (`0x1000: jle r0, 0 → err`, `0x1008: jgt r0, maxFillQty → err`). **The
  mode value is never compared against anything.**
- CPI build: `0x1270: stb [r10-104], 11` (data byte 0 = IxOp.trade),
  `0x1278/0x1280: ldxdw r0, [r10-584]; stxb [r10-103], r0` (data byte 1 =
  the unchecked mode), `0x1290: stxdw [r10-102], qty`, `0x1298: stxdw
  [r10-94], execPrice`. Exactly the round-1 data flow.

Round-2 wiring (why it cannot reach today's pot):

- Exactly one matcher cfg exists on mainnet: `3vgX57bVnFGNV3ytcrm3ex1zHZnkUzh7tGdNCM2eh8rz`
  (paused = 0, owner = the house admin `A3Mu2nQd…`, halfSpreadBps 50,
  impactBps 20, maxFillQty 10,000,000). Its LP portfolio
  `3XsXCzHZwbt4cKGwsudNK85Wo4NkJumvjfFgTJdStxeu` holds ~500.49 USDC of
  capital (500,492,897) — but the portfolio is registered under DEX
  `3dgsgDDBdjxYrAANiE5j6Ui8KN2pD8LwgvLbJq9TK5cd`, and its only leg is on
  market `8ghSDBqrPMj9u1x8F9jZiULqnAsx9ZhQhWDWRskWyc7x` (that same DEX;
  insurance 0, OI 3,000,000/3,000,000).
- The bounty market and both of its portfolios are on DEX
  `CedQKVmmHuta5QcqRNkA96smV33LYtQBHo9wc1YcmFq3`. Core Trade requires
  `dex == market.dex == pflL.dex == pflS.dex` (Track B, Program.lean
  804–806), so a Fill naming the bounty market with this LP fails before
  any quote is computed. There is no matcher LP on the bounty DEX at all.
- The bug itself is live and exercisable today where the cfg does sit
  (market `8ghSDBqr…`) — but that market carries no insurance, so there is
  nothing to win there.

Pot-route assessment (if the wiring changes): unchanged from round 1.
Extracted spread comes directly out of **LP portfolio capital**, not the
vault. Insurance is reachable only at the endgame — bleed an LP below
maintenance and force a liquidation whose shortfall draws market
insurance, capped per liquidation by maxInsDrawBps 2500 → up to 17.5 USDC
of the 70 USDC pot. Watch item: the house owns the matcher cfg and the
bounty market alike; if a cfg + LP portfolio appears under DEX
`CedQK…` with a leg on `BhYaG2…`, F1 becomes a live pot route.

## Lead 2 — Lead-001 / Track A F4 (permissionless `RefreshLeg(forceHaircut=1)`)

**Verdict: STILL PRESENT in round 2 — the patch did not touch RefreshLeg or
the haircut logic. Still griefing / value transfer, still NOT a route to
the pot; round-2's live OI makes the grief immediately exercisable against
the two leg-holders, but creates no insurance-paying variant.**

Source evidence (round-2 tree, `Percolator/Program.lean`):

- `handleRefreshLeg` (line 766) is identical to round 1 in every relevant
  respect: `parse3` + `requireWritable` on the three accounts only — **no
  signer requirement**; status must be Active/WindDown; and
  `let forceHaircut ← if ix.len >= 2 then do pure ((← loadU8 ix.ptr 1) != 0)
  else pure false`, passed straight into `settleAndRefreshLeg`.
- The `settleAndRefreshLeg` realization block is identical: realization
  fires when `(rPool >= totPnl && totPnl > 0) || forceHaircut`;
  `pay = haircutPayoutU64 posPnl rPool totPnl`; then the **full** `posPnl`
  is subtracted from `totalPosPnl` and `posPnl := 0` — the unpaid remainder
  is still destroyed, not retained as a claim. None of the draft's three
  suggested fixes (owner signature for forceHaircut=1, resolved-only
  force, retain-the-remainder) was adopted.
- The round-2 market has warmupSlots = 0, so the realization block is
  active (no warmup gate), exactly as in round 1.

Round-2 live-OI variant check (new this round: matched OI is live):

- The two legs are held by portfolios `HamaLhsCpnS6RA3zGESdU1uCk9xuwRML32ko4HC5sr9a`
  (long 6,674,572, owner `55wRrj9z6Bmr7Qw3tJ7KXcBqW5KfjyLKEQHRsej9N4Sw`,
  capital 50,000,000) and `BTD2y4hoWsbt6ZSkf2QAm3hsVzZu9jom8Wwzvq8Dje96`
  (short 6,674,572, owner `D5uRqfaP1iThVPRKgcDJEpKv5Ls1NE4AA7EnUeWPXPHD`,
  capital 50,000,000). Neither is held by the market-admin key, so the
  grief has real counterparties today: after any oracle move that leaves
  one side with booked-but-unsettled `posPnl` while `realizedPool <
  totalPosPnl` (both are 0 at read time — the window opens on the next
  move), any third party can force-realize the winner at the worst ratio
  with one unsigned RefreshLeg.
- No insurance-paying variant arises. The destroyed profit's backing (the
  loser leg's loss) still strands in `realizedPool` and sweeps **into**
  insurance at resolution — insurance only grows on this path; it never
  pays out. Capturing swept value still requires holding insurance shares,
  and the house holds all 70,000,000 shares; an attacker's capture of a
  stolen amount S with deposit D is `D·S/(70+D) < S` (round-1 live replay:
  1.0 USDC deposit → +0.396 USDC net on 28.13 stolen). The original 70 USDC
  principal is never reduced by this lead, and a forced haircut creates no
  liquidation shortfall — the victim leg is settled, not liquidated.

## Method / constraints

- Chain data: read-only RPC via the workspace Helius helper; ELFs dumped
  from ProgramData accounts and hash-verified against the tasking brief.
- Source: round-1 tree on disk vs current upstream main read through the
  GitHub connector (text only, nothing executed).
- ELF analysis: local sBPF disassembly of the dumped ELFs (data-only
  processing; nothing executed).
- **LiteSVM PoC: NOT run.** Harness/repo content in this workspace failed
  an execution-risk check during this task, so per protocol no repo code
  was built or run. Neither lead needed it for a verdict: F1 is confirmed
  at source + ELF level (and its current unreachability is an on-chain
  wiring fact), and Lead-001's pot question was already settled by round
  1's live-snapshot replay against identical code and identical market
  parameters. A round-2 LiteSVM replay of either lead needs the dev's
  fresh go-ahead on the harness code first.
- Devnet note: the devnet programs have also been upgraded to a different,
  larger build (core 76,912 B / `5d8d330e…`), so devnet no longer provides
  a round-1 byte reference; the round-1 mainnet ELFs are overwritten and
  not recoverable via RPC. The source diff + ELF spot checks above are the
  evidence chain instead.
