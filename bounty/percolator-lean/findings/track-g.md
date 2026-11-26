# Track G — Round-2 insurance drawdown forensics (2026-11-26, ~10:23–10:36 EST)

Market: `BhYaG2JYRHGrLpWs85TroedmkcG7toX1Ly5YbG1FDZHH` (marketId 1, DEX `CedQKVmmHuta5QcqRNkA96smV33LYtQBHo9wc1YcmFq3`, core `GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm`). Vault: `6k2H4GiRDGa7o9X8Ai99sGkPQGAvrAW7gVbKkAKuH2Fk`. All reads via the local Helius helper, read-only. USDC micro-units throughout (70,000,000 = 70 USDC).

## VERDICT

The round-2 pot is being drained through the program's **designed liquidation bad-debt waterfall**, run as a loop against a genuinely insolvent house portfolio — **not** through an accounting exploit. No invariant breaks: `totalInsShares` never moved (70,000,000), the vault SPL balance never moved (342,580,391), and DEX conservation is exact at every read (`totalCapital + totalInsurance + totalResidual + totalSurplus == vaultBalance == 342,580,391`). Contrast round 1's C-1 (forged shares from nothing): here every micro drawn corresponds to a real settled loss on a real leg, paid into the realized pool and realized out to the winning legs. Whether it "claims" the bounty depends on toly's terms — the insurance is leaving through intended instructions, driven by the house's own flow.

## The draw in numbers

Insurance anchors (market account, u64 @224):

| Time (EST) | Insurance | Δ |
|---|---|---|
| 09:52 (pot watch) | 70,000,000 | — (intact; was intact since seeding 2026-11-25 20:55) |
| 10:01 | 66,885,713 | −3,114,287 |
| 10:05 | 61,041,634 | −5,844,079 |
| 10:24 (slot 474197314) | 49,293,127 | −11,748,507 |
| 10:30 (slot 474198483) | 48,809,112 | −484,015 |
| 10:35 (slot 474200262) | 47,686,896 | −1,122,216 |

**Total drawn: 22,313,104 micro = $22.31 = 31.9% of the 70 USDC pot in ~43 minutes.** Episode-average rate ≈ 519k/min (~$31/hour). Rate is price-path dependent: ~609k/min during the 10:05→10:24 down-trend; ~236k/min gross logged shortfall during a 10-minute window (10:23:06–10:32:54) in which the mark actually ROSE net ($70.24 → $70.48) — the drain runs on oscillation, not just trend (see ratchet below).

Projection (conditional): at the episode-average rate the remaining 47,686,896 reaches zero in ~90 minutes (≈12:07 EST); at the slower oscillation-only rate, ~3.5 hours. Draws pause whenever the mark rises sustainably above the victim leg's last-settled entry; they resume on the next down-tick. Nothing in the code stops the draw before dust (see "Can it reach zero").

Other state, latest reads: totalInsShares 70,000,000 (never changed); realizedPool ~1,400 (draws are paid out almost as fast as they land); totalPosPnl 0; feeSurplus 0; OI 16,674,437 / 16,674,437 (was 16,674,572 — the victim leg shrinks by only 1 unit per liquidation); markPrice $70.545126 @10:35. Market params: warmupSlots 0, maxInsDrawBps 2500, liqFeeBps 100, tradeFeeBps 0, mmBps 500, imBps 1000, minMm 0.

## Mechanism (code + tx evidence)

The losing leg: **portfolio PF5 `GE1MXQVcde2XtsEVPecrBCzCVQ7en6ckbZXZJ2WUdxqs` (id 5, owner `8wDjRt6Zro7jg4uF1hxmExXJ6NX2sEH757WspD7chkLN`) — LONG 9,999,867 units on this market with capital = 0 and reservedMm = 0.** It is a fully deficit leg: any settled loss is 100% shortfall.

The flow (verified by tx anatomy; all sampled flow txs are signed by PF5's owner `8wDjRt6Z…` — 60/60 signatures in the 10:22–10:27 sweep):

- Oracle adapter pushes a price update ~every 6 s (101 UpdateOracle inner instructions in the 120-sig tally window; mark ranged $70.169–$70.537 in-window; funding fundE6 = 0 — K moves are pure price).
- Repeatedly: `RefreshLeg` on PF5's market-2 leg (required first — Liquidate's A1 fix demands all victim legs fresh in the same slot), paired `RefreshLeg`s on PF6's short leg (fh 0 and fh 1), `RefreshLeg` fh=1 on PF5's own market-1 leg, and `Liquidate` on PF5's market-1 leg, **mode 0, reqQty = 1**.
- `settleAndRefreshLeg` (Program.lean:327): loss = rawB × (entryK − kLong) / POS_SCALE (ceil). PF5 has no posPnl buffer and no capital above other legs' reserve (availCap = 0), so shortfall = the entire loss. Verified arithmetically: the 10:26:38 liquidation's logged shortfall 220,568 == ceil(9,999,868 × 22,057 / 1e6) for the K-gap at that settle.
- `handleLiquidate` (Program.lean:1102–1111): insDraw = min(shortfall, insurance × 2500/10000, insurance); insurance −= insDraw, realizedPool += insDraw. liqFee = 0 every time (victim's availForFee = 0, so no token ever leaves the vault — consistent with the vault balance never moving).
- Sampled Liquidate events (log fields are `(EvtTag=3, mid<<8|mode=0x100, rawClose, liqFee, shortfall)` per Program.lean:1179): shortfalls 220,568 / 263,511 / 419,315 / 553,949 / 561,283 / 630,662 / 710,581; rawClose = 1 and liqFee = 0 in every one. 120-sig tally window (588 s): 5 Liquidates, Σshortfall 2,316,812, ΣrawClose 5. No DepositInsurance / WithdrawInsurance / Trade / token transfer anywhere in the sampled windows.

### The ratchet (why a flat/rising market still drains)

1. Down-tick: PF5's long settles a loss → full shortfall → insurance draws into realizedPool.
2. Up-tick: PF5's long gain accrues as posPnl, but the flow refreshes it with **forceHaircut = 1**: realization pays pro-rata against the pool and *zeroes the claim, forgiving the unpaid remainder* — the gain never rebuilds a loss buffer. What is paid lands in PF5's shared cross-margin capital and is immediately exposed to PF5's market-2 SHORT leg (a mirror of PF6's 10M long, whose claim on market 2 grew to 534,660,000 against a 53,303,685 pool): the paired RefreshLeg(MKT2, PF5) debits that capital into market 2's realizedPool. In the S0→S1 window this is visible to the micro: market-1 pool inflows were 484,015 (draw) + 79,040 (PF6 short-loss debit) = 563,055, pool Δ was +23, so 563,032 was paid out — and market-2's realizedPool rose by exactly 563,032 in the same window.
3. Meanwhile PF6's winning SHORT on this market (10,000,000 units, owner `7QxxWn1BBZ17cgcNwBPCfLaXDPyoU1no7MuBorMJBXkW`, capital ~140.5M) is refreshed 14× in the tally window (fh 0 ×9, fh 1 ×5) and realizes its gains in full against the draw-fed pool (warmupSlots = 0 → instant realization whenever pool ≥ totalPosPnl).

Net: insurance absorbs the insolvent long's losses; the gains those draws back are collected by the house short (PF6) and, on up-ticks, by PF5 itself — where they are siphoned cross-market into market 2's pool. PF5's capital reads 0 at every snapshot, so every subsequent down-tick is again a full shortfall.

Other legs: PF3 (long 6,674,572, capital 50,000,000) has a latent settled loss of ~4,141,445 at current K but is never refreshed (lastRefreshSlot = creation era) — inert. PF4 (short 6,674,572, capital 50,000,000) has a latent gain of ~4,140,537, also never refreshed — it does not collect. PF1/PF2 are empty, capital 0.

## Does the 25% cap bind?

No. maxDraw at the latest read = 47,686,896 × 25% = 11,921,724 per liquidation; observed shortfalls are 0.22M–0.71M (2–6% of the cap). insDraw == shortfall in every sampled event. The cap only starts trimming when insurance < ~2.8M (25% < max observed shortfall ~0.71M).

## Can it proceed to zero? What stops it?

Yes — mechanically it can drain to dust: the draw path has no floor, no circuit breaker, and no per-period limit; the victim leg survives because each Liquidate closes exactly 1 unit (reqQty = 1, minMm = 0, so no dust-leg rejection) — 133 units closed in ~14 hours of market life, 9,999,867 remain. The LP cannot rescue the remainder: WithdrawInsurance requires oiLong == oiShort == rawBasisLong == rawBasisShort == 0 (Program.lean, handleWithdrawInsurance), and OI is 16.67M per side.

It stops only if: (a) the mark rises sustainably above PF5's settle entries (shortfalls cease; note the pot does not refill — realizedPool is ~1.4k, the money is already paid out); (b) someone fully closes PF5's leg — Liquidate is permissionless and reqQty = 0 closes it all, but that triggers one final draw capped at 25% of remaining insurance (~11.9M) and strands the rest of the accrued loss unpaid; (c) admin wind-down/resolve/tombstone; or (d) callers stop settling the leg (the deficit is public — any keeper can keep it going).

## Classification

- **By-design mechanism, aggressive parameters.** Every step is an intended instruction path: permissionless liquidation of a genuinely insolvent leg, capped insurance draw into the isolated realized pool, instant winner realization (warmupSlots = 0 was chosen at market creation), cross-margin shared capital. The enabling choices: warmup 0, maxInsDrawBps 2500 (non-binding at these shortfall sizes), minMm 0 + reqQty 1 partial closes (leg is effectively immortal), and a zero-capital cross-margined portfolio whose second leg sits on an uninsured market (market 2, insurance 0) pinning its capital at 0.
- **Not an accounting exploit.** No forged shares (totalInsShares constant, and WithdrawInsurance is OI-gated anyway), no conservation break (identity above holds exactly), no unauthorized state transition — the flow is signed by the victim portfolio's own owner, and the "victim" and the winning short are both round-2 house wallets. The loss falls on the insurance LP position (the 70 USDC seed; shares now redeem at 47,686,896/70,000,000 ≈ 0.681 USDC if the market ever winds down).
- Corrections to the parent brief: the flow signer in all sampled windows is `8wDjRt6Z…` (PF5's owner), not `7QxxWn1B…`; the Liquidate log's fields are `(tag, mid<<8|mode, rawClose, liqFee, shortfall)` — insDraw is not logged, it equals shortfall here because the cap never binds; live rawClose is 1, liqFee is 0.
