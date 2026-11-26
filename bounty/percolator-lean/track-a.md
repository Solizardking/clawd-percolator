# Track A — percolator-lean Core Program Logic vs Its Lean Spec/Proofs

Audit target: `Percolator/Program.lean` (1466 lines), `Spec.lean`, `Proofs.lean`,
`TraceProofs.lean`, `Layout.lean`, `INVARIANTS.md`, plus the `LeanSVM.Math` /
`LeanSVM.Input` primitives the program is compiled from, and the TS client /
bounty scripts in `repo/` for intended semantics and live-market parameters.

Bounty market: DEX `6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH`, market
`76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq`, vault
`8MqHSrm6v6ccJVQunjnLqfBBWoAhVGrqUV4hdSH9F8i9`. Per
`repo/scripts/lean-mainnet-bounty.ts` (lines 29, 66–79): the pot was seeded with
a single shares-bearing `depositInsurance` of `PRIZE = 70n * POS_SCALE`
(70,000,000 micro-USDC) into a fresh market, so at seed time
`totalInsShares = insurance = 70_000_000`, held entirely by the funder's one
InsReceipt; market params: `tradeFeeBps = 0`, `maxInsDrawBps = 2500`,
`imBps = 1000`, `mmBps = 500`, `liqFeeBps = 100`, `maxMoveBps = 400`,
`minMm = 0`. The seeding script opens no positions.

Headline result: **one critical, fully-specified drain of the 70 USDC pot**
(Finding 1) that lives entirely in the gap between the Spec's typed account
model and the program's raw byte-layout accounts. The remaining findings are
lower-severity instances of the same root cause plus one spec-level economic
flaw. The arithmetic core itself (hunting ground 1) is genuinely sound — see
"Checked and held".

---

## Finding 1 — CRITICAL: Phantom insurance shares via cross-type account recycling (CloseAccount → DepositInsurance init never zeroes `shares`; `InsOff.shares` aliases the old portfolio owner's pubkey bytes)

**Severity:** Critical — drains the entire insurance pot (all 70 USDC) for the
cost of one dust deposit plus transaction fees. This is exactly the bounty's
primary win condition ("drain the insurance").

**Evidence.**

1. `handleCloseAccount` (`Program.lean:1403`) closes a Portfolio whose only
   state requirements are zero *accounting totals* — the owner's pubkey stays
   in the account data; only the magic word is erased before lamports are
   drained (`Program.lean:1429`):

```lean
  storeU32 data 0 0
  let accLam  ← AccountView.lamports acc
  ...
  AccountView.setLamports acc 0
```

   The Portfolio branch (`Program.lean:1410-1415`) requires
   `capital == 0 && legCount == 0 && totalMm == 0 && totalIm == 0 &&
   totalReservedMm == 0` — a fresh, never-used portfolio satisfies all of
   these while `PflOff.owner` (a full 32-byte pubkey) remains in the data.

2. The `DepositInsurance` receipt-initialization branch (`Program.lean:677-682`)
   fires on any program-owned account of ≥ 88 bytes whose magic word is 0 —
   exactly the state `CloseAccount` leaves behind — and writes only
   `magic`, `marketId`, `owner`, `market`. **It never writes `shares`:**

```lean
    if (← loadU32 rData InsOff.magic) == 0 then
      storeU32 rData InsOff.magic MAGIC_INSURANCE
      storeU64 rData InsOff.marketId mid
      pubkeyCopy (rData + InsOff.owner)  (AccountView.keyPtr lp)
      pubkeyCopy (rData + InsOff.market) (AccountView.keyPtr mkt)
```

   It then *adds* the freshly minted amount on top of whatever already sits in
   the `shares` field (`Program.lean:699`):

```lean
    addU64At rData InsOff.shares         minted
```

3. The byte layouts alias (`Layout.lean`): `PflOff.owner` is at byte 56
   (32 bytes, i.e. account bytes 56–88); `InsOff.shares` is at byte 80.
   Account bytes 80–88 are therefore **bytes 24–32 of the old portfolio
   owner's pubkey**, reinterpreted as a little-endian u64 share balance.
   Neither the `market` write (bytes 48–80) nor the `owner` write
   (bytes 16–48) in the init branch touches bytes 80–88.

4. `handleWithdrawInsurance` trusts that field completely
   (`Program.lean:701` ff.): it checks the receipt's owner/market/marketId
   bindings (all freshly written by the init branch, so they pass) and
   `curSh >= burnShares`, then pays (`Program.lean:728`):

```lean
  let payout ← mulDiv64 burnShares ins totSh
```

**Why the Lean proof does not cover it.** The Spec's state is a typed record —
`markets : Nat → Option MarketState`, `portfolios : Nat → Option PortfolioState`,
`insShares : Nat → Nat → Nat` (`Spec.lean:143-145`) — in which an account has
exactly one type forever and an LP's share balance can only ever change through
`stepDepositInsurance` / `stepWithdrawInsurance`
(`Spec.lean:278-313`). There is no representation of "the same Solana account
first holds a Portfolio, is closed, and is then re-read as an InsReceipt with
residual bytes aliasing a different struct's field", so no theorem quantifies
over it. `INVARIANTS.md` §7 (line 85) explicitly claims the opposite safety
property — that `CloseAccount` "zeroes the magic word before draining lamports
to prevent same-tx revival" — but zeroing only the 4-byte magic prevents
revival *as the same type*; revival as a *different* type with field-offset
aliasing is precisely what succeeds. The already-tried attacks
(`scripts/lean-attack.ts`, `scripts/lean-bounty-validate.ts`) cover
"withdraw insurance w/o shares" (a genuine zero-share receipt) and
"non-owner withdraw" — not shares fabricated out of a recycled account's
residual bytes.

**Exploit sketch (against the live bounty market).**

1. Create a fresh Portfolio on the bounty DEX (any signer keypair; no deposit
   needed — `capital = 0`, `legCount = 0`, all totals 0). Its account is
   952 bytes (`PflOff.size`), satisfying the receipt's ≥ 88-byte requirement.
   The future phantom share balance is the u64 formed by bytes 24–32 of the
   attacker's own pubkey — a uniform random u64, ≥ the market's
   `totalInsShares` (70,000,000) with probability `1 − 70_000_000 / 2^64`
   (≈ 1 − 4×10⁻¹²); no vanity grinding is needed.
2. In one transaction:
   a. `CloseAccount` on the portfolio (authority = attacker; destination =
      attacker's wallet). Magic zeroed, rent reclaimed. The account's data —
      including the owner pubkey at bytes 56–88 — is untouched.
   b. A small SystemProgram transfer re-funding the account to rent-exempt, so
      it survives the end of the transaction (lamport collection happens at
      tx end; the account remains addressable and writable mid-tx).
   c. `DepositInsurance` (donate = 0, amount = 1 micro-USDC) against the
      bounty market with `receipt` = the recycled account. The init branch
      binds it to the attacker and the bounty market; `minted = 1`; the
      receipt's share balance becomes `phantom_pubkey_u64 + 1`.
3. `WithdrawInsurance` with `burnShares = totalInsShares + 1` (70,000,001,
   well under the phantom balance). Precondition: market
   `oiLong == oiShort == rawBasisLong == rawBasisShort == 0` — the seeding
   script opened no positions, so this holds unless a third party currently
   has a live position (if it does, the durable phantom receipt from step 2
   simply waits for OI to return to zero).
   `payout = ⌊70_000_001 × 70_000_001 / 70_000_001⌋ = 70_000_001` micro-USDC —
   the entire 70 USDC pot plus the attacker's own dust back — is transferred
   from the vault to the attacker's token account. `V == SPL vault balance`
   is preserved *in the program's own accounting* while real funds leave
   against shares that were never minted, which is exactly the conservation
   break the bounty describes.

**Confidence:** confirmed-by-reading (offsets, missing store, and payout path
all verified line-by-line above). The only live-state precondition is OI == 0
at withdraw time, checkable on-chain at PoC time.

---

## Finding 2 — MEDIUM (latent, spec-level): First shares-depositor captures pre-existing shareless insurance 1:1

**Severity:** Medium as a design flaw; **not** exploitable against the bounty
market as seeded (see below).

**Evidence.** `Program.lean:690`:

```lean
    let minted ← if totSh == 0 || ins == 0 then pure amount else mulDiv64 amount totSh ins
```

When `totalInsShares == 0` but `insurance > 0`, a depositor mints shares 1:1
regardless of how large the pre-existing insurance balance is, and thereafter
owns `amount / (amount)` of all shares — i.e. on withdraw (OI == 0) they
receive the *entire* insurance balance, including value they never deposited.
Shareless insurance is reachable through two program paths: the permissionless
donation branch of `DepositInsurance` (`Program.lean:664-669`, adds to
`m.insurance` minting 0 shares) and trade fees in `handleTrade` mode 0, which
credit `fee2` to `m.insurance` / `totalInsurance` with no shares minted.
The mirror branch (`insurance == 0 ∧ totalInsShares > 0` → mint 1:1) likewise
lets a new depositor dilute legacy shareholders' claim on all future fee
inflows at no cost.

**Why the proof does not cover it.** It does — and that is the point: the Spec
contains the identical rule (`Spec.lean:289-291`,
`if m.totalInsShares = 0 ∨ m.insurance = 0 then amount`), so the proofs are
faithful to the spec; the flaw is in the spec's economics. No invariant in
`INVARIANTS.md` bounds an LP's withdrawal by their own deposits plus a
pro-rata share of *post-entry* accruals, so "withdraw more than deposited"
(the bounty's secondary condition) is achievable in any market where insurance
accrues before the first shares deposit.

**Bounty-market applicability.** None as seeded: `lean-mainnet-bounty.ts`
seeded the pot through the shares path itself (so `totSh = ins = 70_000_000`
from the start) and set `tradeFeeBps = 0`, so no shareless accrual exists to
capture and later deposits mint at a fair 1:1 rate. Recorded because any
future market on this program seeded by donation, or with fees flowing before
the first LP deposit, is immediately capturable by the first depositor.

**Confidence:** confirmed-by-reading.

---

## Finding 3 — LOW/MEDIUM: Initializers never zero account state (same root cause as Finding 1; markets and DEXes born violating Spec well-formedness)

**Severity:** Low for extraction (self-scoped to the creator's own new
market/DEX — no path to the bounty pot found), Medium as proof-gap evidence:
it shows Finding 1 is a class, not a one-off.

**Evidence.**

- `handleCreateMarket` (`Program.lean:440` ff.) writes magic, status, ids,
  keys, params, `adlLong/adlShort`, `kLong/kShort`, `minMm` — and never writes
  `windDownSlot` (off 128), `realizedPool` (216), `insurance` (224),
  `feeSurplus` (232), `totalPosPnl` (240), `totalInsShares` (248),
  `oiLong` (256), `oiShort` (264), `rawBasisLong` (272), `rawBasisShort`
  (280), `resolvedNum` (320), `resolvedDen` (328), or `activeLegs` (336).
  On a recycled closed-Portfolio account (952 B ≥ `MktOff.size` 352 B, magic
  zeroed by `CloseAccount`), those fields alias leg-slot bytes
  (`PflOff` legs begin at 120, stride 104): `totalPosPnl` @240 and
  `totalInsShares` @248 / `oiLong` @256 / `oiShort` @264 are fragments of the
  second leg's stored market pubkey, and `rawBasisShort` @280 is a freed
  leg's stale `entryK` — `freeLegIfEmpty` (`Program.lean:219`) clears only
  the `active` byte, leaving `entryK`, `cachedMm`, `cachedIm`,
  `lastRefreshSlot`, `reservedMm` in place. The market is born with
  `oiLong ≠ oiShort` and nonzero phantom balances, violating the Spec's
  `MarketState.oiBalanced` / well-formedness that `createMarket_sound`
  assumes of its zero-initialized `MarketState`.
- `handleInitDex` (`Program.lean:388` ff.) never writes `vaultBalance`,
  `totalCapital`, `totalInsurance`, `totalResidual`, `totalSurplus`
  (offsets 160–200), which on a recycled portfolio alias leg-0's `entryK` /
  `cachedMm` fragments — a DEX born with `stockExact` already false.
- `handleTombstoneMarket` does not require `totalInsShares == 0`, and
  `handleResolveMarket` leaves `resolvedNum/resolvedDen` set; a tombstoned
  market closed via `CloseAccount` and recycled through `CreateMarket`
  inherits its dead `totalInsShares`, so new depositors are diluted by
  unusable legacy shares (their receipts are bound to the old `marketId`).
- `handleInitPortfolio` (`Program.lean:586-594`) writes only magic, legCount,
  portfolioId, episode, dex, owner — it does not write `capital`, `totalMm`,
  `totalIm`, `totalReservedMm`, nor clear the legs region. This direction is
  currently *held* only by `CloseAccount`'s portfolio preconditions (totals
  must already be 0); it is the same missing-defense pattern one check away
  from a phantom-capital variant.

**Why the proof does not cover it.** Every `*_sound` theorem in
`Proofs.lean` / `TraceProofs.lean` starts transitions from the Spec's
zero-valued structures; "account data contains arbitrary residual bytes from
a previous life as a different account type" is outside the model (see
Finding 1). `checkFreshAccount` (`Program.lean:57-61`) checks only owner,
length, and `magic == 0` — it is the program's sole freshness gate, and it
cannot distinguish a genuinely fresh account from a closed-and-recycled one.

**Confidence:** confirmed-by-reading for the missing writes and offset
aliasing; extraction impact beyond the creator's own market/DEX not
established (needs-devnet-poc to probe second-order effects, e.g. phantom
`activeLegs` blocking `ResolveMarket`).

---

## Finding 4 — LOW: Permissionless `forceHaircut` lets anyone permanently realize a victim's unrealized PnL at the worst ratio

**Severity:** Low — griefing / cross-user value transfer; conservation is
preserved, so it is not a bounty drain by itself.

**Evidence.** `handleRefreshLeg` (`Program.lean:741`) requires no signer at
all, and takes the haircut trigger from caller-supplied instruction data
(`Program.lean:763`):

```lean
  let forceHaircut ← if ix.len >= 2 then do pure ((← loadU8 ix.ptr 1) != 0) else pure false
  let _ ← settleAndRefreshLeg dData mData fData slotIdx curSlot forceHaircut
```

Inside `settleAndRefreshLeg`, the realization block (`Program.lean:367-385`)
pays out a leg's full `posPnl` at the *current* ratio
`haircutPayoutU64 posPnl rPool totPnl` and zeroes the claim whenever
`forceHaircut` is set, even when `realizedPool < totalPosPnl`. A third party
can therefore force any winner to crystallize an underbacked claim at the
moment backing is lowest; the un-realized remainder of their face claim is
destroyed, and the backing ratio of every remaining claimant improves at the
victim's expense (the transfer direction guaranteed by
`haircutPayout_monotone_ratio`).

**Why the proof does not cover it.** The Spec models realization
(`stepRealizeLegPnl`) as a transition that preserves `stockExact` — which it
does. No theorem speaks to *who may trigger* realization on another user's
leg or to the fairness of forced early realization; authorization and
trigger-rights are outside the proven invariant set.

**Confidence:** confirmed-by-reading.

---

## Checked and held

Surfaces audited and found sound, with the reason each attack fails:

- **Fixed-width arithmetic (hunting ground 1).** `mulDiv64` is an exact
  128-bit mul-div: 32-bit-limb schoolbook product plus a 64-step restoring
  long division (`svm/LeanSVM/Math.lean:33-72`), failing closed with
  `Err.arithmeticOverflow` when the quotient cannot fit in u64
  (`hi < denom` requirement). The `mulDiv64Ceil` large-operand exactness
  shortcut (`q * denom == a * b` mod 2⁶⁴) is sound because the true remainder
  is `< denom < 2⁶⁴`. `checkedAdd64` / `checkedSub64` genuinely check
  (`svm/LeanSVM/Input.lean:327-335`). Every raw `+`/`-` in `Program.lean` is
  either one of these checked helpers or is dominated by an explicit guard
  (e.g. `curSh - burnShares` after `curSh >= burnShares`; `oiL - qty` after
  `qty <= oiL`; `cap - liqFee` after `liqFee ≤ availForFee ≤ cap`). The one
  theoretical edge — a leg whose accrued `rawB × Δk` exceeds u64 in
  `settleAndRefreshLeg`'s gain computation — reverts the settle (fail-closed
  liveness edge at extreme price paths), it cannot wrap or mint value.
- **Leg-refresh freshness (ground 3).** Enforced, not trusted:
  `checkAllLegsFreshLoop` (`Program.lean:170`) requires
  `lastRefreshSlot == curSlot` for every non-flat leg, and is invoked in
  `WithdrawCollateral`, `Trade` modes 0 and 2/3 (buyer), and `Liquidate`
  (after the A1 fix comment at the call site). Refresh itself settles against
  the market's *live stored* `markPrice`, so there is no stale-mark path;
  `UpdateOracle` is the only writer of `markPrice`, is gated to `oracleAuth`,
  is rate-limited to one step per slot (`curSlot > lastSlot`), and is clamped
  by `maxMoveBps` with funding charged against the same budget.
- **Oracle adapter account validation.** `OracleAdapter.lean` pins the feed
  account's key and owner to the configured values and validates feed-id,
  discriminator, version, sign, exponent and price bounds
  (`OracleAdapter.lean:367-418`) — the "any PriceUpdateV2 at any address"
  pattern does not apply.
- **Cross-margin containment (ground 4).** Loss settlement debits capital
  only down to `otherRes = totalReservedMm − leg.reservedMm`
  (`settleAndRefreshLeg`), `syncLegMargins` caps each leg's reservation by
  `capital − otherRes`, and withdrawals are gated on post-withdrawal
  `capital ≥ totalIm ≥ totalReservedMm` per-leg ordering
  (`mmBps < imBps` enforced at `CreateMarket`, same `minMm` floor in both
  margin computations). No path found to withdraw one leg's collateral out
  from under another leg's reservation.
- **Rounding directions (ground 6).** Systematically protocol-favorable:
  winner PnL gain floor / loser loss ceil in oracle K updates and in
  `settleAndRefreshLeg`; `haircutPayoutU64`, resolved claims
  (`handleClaimResolved`), and insurance mint/payout all floor in the pool's
  favor; margin requirements, trade fees, exec-spread, `postAdlRawAddU64`,
  and `proportionalRawRedU64` all ceil against the user. No user-favorable
  per-op rounding that compounds was found (the exceptions are Findings 1–2,
  which are share-accounting flaws, not rounding).
- **Liquidation (ground 5).** Fee is deficit-scaled, capped by
  `min(targetFee, capital − otherRes)`, and paid only when
  `shortfall > 0 ∨ equity < totalMm` after a same-slot all-leg refresh;
  mode-1 takeover preserves OI/rawBasis/ADL bit-for-bit and requires the
  taker to satisfy full IM; mode-0 reduces both sides' OI symmetrically with
  the derived-ADL recompute. Insurance draws are capped per call by
  `maxInsDrawBps` and flow into `realizedPool` (backing winners), not to the
  liquidator. No self-supplied price path exists (mark comes only from the
  oracleAuth-gated `UpdateOracle`). Note: the draw cap is per-call with no
  per-episode aggregate cap, but drawing requires a genuine fresh shortfall
  each time, so extraction remains bounded by real counterparty losses — as
  designed.
- **Account lifecycle, same-type (ground 7).** Live accounts cannot be
  re-initialized (`checkFreshAccount` requires magic == 0; all live types
  have nonzero magic); double-close fails (`CloseAccount` falls through to
  `invalidMagic` once magic is zeroed); closing an InsReceipt requires
  `shares == 0` and does not corrupt `totalInsShares`; closing a Portfolio
  requires zero capital/legs/totals; markets hold no references to
  portfolios, so a closed portfolio cannot strand a market's accounting
  (OI lives in portfolio legs, and a closable portfolio has none). The
  failure is specifically the *cross-type* reuse in Findings 1 and 3.
- **Direct SPL donation to the vault.** A raw token transfer into the vault
  PDA changes only the SPL balance, not `vaultBalance`/`insurance`
  accounting; no payout path keys off the raw SPL balance, so donated funds
  are stranded, not extractable, and cannot skew share price (share pricing
  reads `m.insurance`, not the token account).
- **CPI / authority pinning.** All token CPIs pin the canonical SPL Token
  program id word-by-word (`checkTokenProg`); vault payouts are signed only
  by the `["dvlt", dex, bump]` PDA; deposit paths cannot be redirected by
  passing the vault as the source token account (SPL authority checks reject
  it); `Trade` requires *both* counterparties' owner signatures in all
  modes, and portfolio/market/dex aliasing between roles is blocked by the
  distinct magic checks plus explicit `requireKeyNe` where two portfolios
  appear.
