# DRAFT — for dev only. Not filed, not posted, not sent.

# [High] Permissionless `RefreshLeg(forceHaircut=1)` burns an unsettled winner's backed profit; an insurance LP can capture it

## Summary
`RefreshLeg` is permissionless (no signer check) and accepts a caller-supplied `forceHaircut` flag. Because PnL settlement is lazy (a position's gain/loss is only booked when *that* position is refreshed), immediately after any oracle move a solvent market transiently has `realizedPool (R_m) < totalPosPnl (P⁺_m)` — the winners' gains are booked but the matching loser losses have not yet landed in `R_m`. Any third party can call `RefreshLeg(winner, forceHaircut=1)` in that window. The winner is paid `⌊posPnl · min(R_m,P⁺_m)/P⁺_m⌋ ≈ 0`, and the **entire unpaid remainder of their profit is zeroed and destroyed** (`posPnl := 0`, `totalPosPnl -= posPnl`). The loser funds then arrive in `R_m` with no claimant and are swept into the market's `insurance` at `ResolveMarket`. An attacker who holds that market's insurance shares withdraws the stranded value: in the PoC, 1.0 USDC of insurance deposit becomes a 41.0 USDC withdrawal while the victim loses a 40 USDC profit.

This directly violates the program's advertised Lean guarantees of "zero haircut" on a solvent counterparty and "No First-Mover Drain". The proofs hold only in the pure `Spec` model, where `R_m` is treated as already-final; the deployed program lets an unprivileged caller choose the settlement order, which the proofs never model.

## Affected program
- Repo/pin: `percolator-lean` @ `d591ab9` (deployed `35cd13c`); compiled by `lean-svm` @ `0bd9670`; cli `f0d81cd`.
- Deployed program id (mainnet): `GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm`
- Program sha256: `c6dc8329207c8a0f874b4c1ceb1c48a5529d031453b7c1e9ce6f5808bca436d1` (matches `lean-mainnet.json` and the first 73224 bytes of the devnet dump used in the PoC).
- Bounty DEX `6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH`, market `76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq` (70 USDC insurance). See "Reach to the bounty market" below.

## Root cause (file:line, `percolator-lean/Percolator/Program.lean`)
1. `handleRefreshLeg` has **no `requireSigner`** and reads the haircut flag from instruction data:
   - `:741-742` `def handleRefreshLeg … let (dex, mkt, pfl, ix, progId) ← parse3 input` — only `requireWritable` on the accounts, never a signature from the portfolio owner.
   - `:763` `let forceHaircut ← if ix.len >= 2 then pure ((← loadU8 ix.ptr 1) != 0) else pure false`
   - `:764` `let _ ← settleAndRefreshLeg dData mData fData slotIdx curSlot forceHaircut`
2. Lazy settlement books a winner's gain into `posPnl`/`P⁺_m` as soon as the winner is refreshed:
   - `:338-341` `if kSide > entryK then let gain ← mulDiv64 rawB (kSide-entryK) POS_SCALE; posPnl ← checkedAdd64 posPnl gain; addU64At mktData MktOff.totalPosPnl gain`
   - The matching loser loss only reaches `R_m` when the loser is later refreshed (`:348-360`, `addU64At mktData MktOff.realizedPool capDeb`). So `R_m < P⁺_m` right after a move.
3. The forced-haircut realization destroys the unpaid remainder:
   - `:367-380`:
     ```
     if posPnl > 0 then
       if warmup == 0 then
         let rPool ← loadU64 mktData MktOff.realizedPool     -- ≈ 0 in the attack window
         let totPnl ← loadU64 mktData MktOff.totalPosPnl
         if (rPool >= totPnl && totPnl > 0) || forceHaircut then
           let pay ← haircutPayoutU64 posPnl rPool totPnl    -- ⌊posPnl·min(R,P)/P⌋ ≈ 0
           if pay > 0 then … addU64At pflData PflOff.capital pay …
           subU64At mktData MktOff.totalPosPnl posPnl         -- removes the FULL posPnl
           posPnl := 0                                        -- remainder (posPnl − pay) destroyed
     ```
   The gap `posPnl − pay` is never retained as a claim; it is lost.
4. The stranded loser funds end up as insurance at resolution: `sweepResolvedDust` (`:97-118`) moves leftover `realizedPool` into `m.insurance` once `totalPosPnl == 0`, and `handleResolveMarket` (`:1330-1350`) calls it. Permissionless `StartWindDown` (stale oracle, `:1224-1242`), `ForceClosePair` (`:1244-1328`), and `ResolveMarket` let any caller drive the market to resolution.

## Lean claim / invariant it violates
- **README.md (percolator-lean), §"Unconditional Anti-Shielding":** "…Bob's `SOL/USD` payout suffers **zero shortfall, zero insurance draw, and zero haircut**…". In the PoC a fully solvent winner suffers a 100% haircut on their profit.
- **INVARIANTS.md §1, "No First-Mover Drain" (`haircutPayout_monotone_ratio`):** `(R_m − pay)·P⁺_m ≥ R_m·(P⁺_m − p)`. The theorem assumes `R_m` is the final loser pool for the epoch. On-chain, `R_m` is order-dependent and any unprivileged caller picks the order, so the "monotone ratio / no first-mover" property does not hold for the real contract.
- Root cause of the mismatch: `Percolator/Program.lean` carries **0 theorems**; all 66 theorems import only `Percolator/Spec.lean`, which has no oracle, no `K` index, and no lazy per-leg settlement (losses enter as a free `loss` parameter). There is no refinement lemma linking `Spec.step*` to `Program.handle*`, so laziness/ordering effects are entirely outside the proof surface.

## Proof of concept (LiteSVM 0.11, local, no network writes)
Harness: `/workspace/toly-bounty/poc-runner/src/main.rs` (copy saved at `reports/poc-runner-main.rs`), run against a read-only dump of the live program.

Exact commands:
```
cd /workspace/toly-bounty/poc-runner
cargo build --release
./target/release/poc-runner /workspace/toly-bounty/elf/percolator-devnet.so
```
(The ELF is `solana program dump -u devnet GD43BhWwAjiZLs56juEkacSKkY3TVCTbD7cG7HHDYKdA`, first 73224 bytes sha256 = `c6dc8329…a436d1`, byte-identical to the mainnet program.)

Scenario (PoC1 — profit wipe on a solvent market):
1. InitDex; create a market: price 100e6, IM 10%, MM 5%, liqFee 1%, tradeFee 0, maxMove 4%, warmup 0.
2. Victim (long 1u) vs honest counterparty (short 1u); both post 1000 collateral.
3. Advance one slot; `UpdateOracle` to 104e6 (+4%, inside the clamp). Now the long is up 4, the short down 4, market fully solvent.
4. Attack = a single **unsigned** `RefreshLeg(victim_pfl, forceHaircut=1)` before any loser is refreshed.
5. Then everyone is refreshed normally.

Output:
```
=== PoC1: permissionless forceHaircut burns unsettled winners' profit ===
  control : victim cap=1004.000000 attacker-long cap=1004.000000 stranded R_m=0.000000
    after forced refresh: victim posPnl=0 totalPosPnl=0 realizedPool=0
  attack  : victim cap=1000.000000 attacker-long cap=1004.000000 stranded R_m=4.000000
  RESULT  : victim lost 4.000000 of backed profit; 4.000000 left stranded in R_m
```
Control: the winner correctly ends at 1004. Attack: the winner is forced to 1000 (full 4.0 profit destroyed); 4.0 sits unclaimed in `R_m`.

Scenario (PoC1b — attacker captures the stranded value as an insurance LP):
1. Attacker deposits **1.0** USDC into the market's insurance (holds the shares).
2. Victim long 10u vs counterparty short 10u; +4% move → victim owed 40.
3. Attacker's single unsigned `RefreshLeg(victim, forceHaircut=1)` burns the 40.
4. Oracle goes stale → attacker calls permissionless `StartWindDown`, then `ForceClosePair`, then `ResolveMarket`; the stranded `R_m` is swept into `insurance`.
5. Attacker `WithdrawInsurance` of all shares.

Output:
```
  PoC1b control: victim cap=1040.000000 attacker insurance withdrawal=1.000000 (deposited 1.0)
  PoC1b attack : victim cap=1000.000000 attacker insurance withdrawal=41.000000 (deposited 1.0)
```
Attacker turns a 1.0 deposit into a 41.0 withdrawal; the victim loses a 40.0 profit. Net theft of the counterparty's realized loss via an unprivileged instruction.

## Impact
- **Theft of realized profit / loss of funds.** Any unprivileged account can destroy a solvent winner's PnL on any market, every time the oracle moves, and (as an insurance LP on that market) convert it to withdrawable insurance. It also enables griefing (burn profits with no gain) and front-running of honest keepers.
- Severity: **High.** No special role, no capital at risk beyond gas (and an optional tiny insurance deposit to capture), repeatable on every price update.

## Reach to the bounty market (`76oGDQ…`, 70 USDC)
This bug lets an attacker steal the *counterparty's* realized loss on whatever market the victim trades; it does not by itself drain the 70 USDC house insurance (that pool belongs to market id=1 and is isolated — see LEAD-004). It becomes a direct path to the prize only if the house opens/holds a position on the bounty market such that the attacker is the counterparty LP; against the current all-insurance, zero-OI state it is an attack on future traders of that market, not on the 70 USDC itself. Reported as a standalone loss-of-funds bug in the engine, independent of the prize.

## Suggested fix
Any one of:
1. Require the portfolio owner's signature for `forceHaircut=1` (keep permissionless refresh only for `forceHaircut=0`), so no third party can force a below-par realization.
2. Only allow `forceHaircut` realization once the market is `Resolved` (when `resolvedNum/resolvedDen` is final), never on a live/windDown market with unsettled losers.
3. Do not destroy the remainder: on a forced haircut, pay `pay` and **retain `posPnl − pay`** as a claim (leave `posPnl`/`totalPosPnl` reduced only by `pay`), so later-arriving `R_m` can still pay the winner.
Option 3 is the most robust; 1 or 2 are the smallest changes.

## Artifacts
- `reports/poc-runner-main.rs` — full LiteSVM harness.
- `reports/poc-output.txt` — complete run output.
- `reports/ENGINE-MATH-leads.md` — the broader engine-math slice.
- `reports/LEAD-004-isolation.md` — shared-vault isolation analysis.

---

## Live-state replay (mainnet bounty snapshot, strictly local)
Goal: run the exact lead-1 drain against a **read-only snapshot of the live mainnet bounty accounts** and report whether any of the real 70 USDC insurance can be extracted. No transaction was sent to any network; only read-only `solana account` dumps were taken, then replayed in LiteSVM.

**Setup (all local):**
- Program loaded at the real mainnet id `GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm`; verified `sha256(first 73224B) = c6dc8329…a436d1` (== `lean-mainnet.json`).
- Real account dumps loaded into LiteSVM at their real pubkeys: DEX `6nxbqVQ…`, market `76oGDQ…` (id=1, `maxInsDrawBps=2500`), vault `8MqHSrm6…` (70.000000 USDC), insurance receipt `4pxsDAv3…` (70,000,000 shares, the house).
- Derived vault PDA `find_program_address(["dvlt", dex])` = `8MqHSrm6…` — **matches the live vault**, so the program's own PDA signing works on the snapshot.
- Only two bytes-fields patched on the market: `oracleAuth` → a local test key (so price moves can be driven without the house's oracle key) and `lastOracleSlot` → current slot (so the snapshot isn't pre-stale). USDC mint substituted with a local 6-decimal mock to fund test actors. All balances, shares, config and the 70 USDC are the live values.
- Actors are fresh test keypairs: `victim` (long 10u, 1000 collateral), `shortk` (honest short, 1000), `att` (attacker).

**Commands:**
```
cd /workspace/toly-bounty/poc-runner
cargo build --release
./target/release/poc-runner /workspace/toly-bounty/elf/percolator-devnet.so live
```

**Output (`reports/live-replay-output.txt`):**
```
  LIVE START: vault(SPL)=70.000000 market.insurance=70.000000 totalInsShares=70000000 markPrice=70.319202
  attacker deposits 1 USDC insurance -> shares=1000000 of totalInsShares=71000000
  after forced refresh: victim posPnl=0 totalPosPnl=0 realizedPool=0
  stranded realizedPool R_m (stolen from victim) = 28.127680
  permissionless StartWindDown by attacker: Ok("ok")
  ResolveMarket: Ok("ok")  insurance now=99.127680 totalInsShares=71000000 R_m=0.000000
  WithdrawInsurance(attacker, 1000000 shares of 71000000): Ok("ok")
  ===== LIVE REPLAY RESULT =====
  vault(SPL):        70.000000 -> 2069.603836     (delta = test actors' own collateral, not the prize)
  market.insurance:  70.000000 -> 97.731516
  totalInsShares:    70000000 -> 70000000
  victim net:        1000.000000 (healthy would be 1028.127680; lost 28.127680)
  attacker net USDC: +0.396164
  of the ORIGINAL 70 insurance, attacker extracted: 0.396164 (deposit was 1.0)
```

**Interpretation — how much of the 70 is extractable: effectively none.**
- The forceHaircut wipe **does fire on the live market**: the victim's realized profit (28.127680 at the live mark) is destroyed, and the honest short's matching loss is stranded in `R_m`. This is a real loss-of-funds against the victim.
- But converting that into the attacker's pocket is throttled by the pool's share structure. At resolution the stranded 28.13 is swept into `market.insurance` (70 → 99.13). The attacker holds only **1,000,000 of 71,000,000 shares** (the house holds 70,000,000), so the attacker's withdrawal recovers only `1/71` of the swept amount: **net +0.396164** on a 1.0 deposit. The other ~27.7 of the stolen profit is **diluted to the house's own insurance stake** (insurance ends at 97.73 after the attacker exits).
- **The original 70 USDC principal is never reduced** — the insurance pool only grows. The attacker cannot withdraw the house's shares, and there is no path that credits the attacker from another holder's shares.
- General bound (algebraically, confirmed by the mechanics): if the attacker pre-funds insurance with `D`, their capture of a stolen amount `S` is `D·S/(70+D)`, which is **strictly less than `S`** and independent of the 70 principal. So the attacker can at most claw back a fraction of the profit *they caused to be stolen from the victim*, never the house's 70. `maxInsDrawBps=2500` is irrelevant here — the sweep-into-insurance path does not use the insurance-draw cap.

**Conclusion for the bounty prize:** lead-1 is a confirmed loss-of-funds bug against market participants (a solvent winner's profit can be destroyed by an unprivileged caller), but it is **not** a route to the 70 USDC prize. On the live market configuration the stolen value flows predominantly to the existing house insurance stake, and the attacker nets a negligible pro-rata cut. Reported as a High engine bug on its own merits; it does not, by itself, drain the bounty insurance.
