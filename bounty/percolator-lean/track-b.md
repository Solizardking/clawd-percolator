# Track B — Oracle Adapter + Matcher audit (percolator-lean bug bounty)

Scope: `lean/Percolator/OracleAdapter.lean`, `lean/Percolator/Matcher.lean`, with
`lean/Percolator/Program.lean` (core) and `repo/src/lean/` (TS client) as supporting
evidence. Read-only audit; no transactions sent, nothing built or installed.

## Live state read on-chain (2026-11-25 evening, read-only RPC) — grounds the "live / not live" calls below

- Adapter cfg `DZhXmSK9PD9Jszt2WDTYWABAVrdkxp3WkuNYAHPEnuQH` (352 B, owner = oracle_adapter):
  feedType **1 (PythV2)**, quoteFeedType 0, **flags = 2 (FLAG_PIN_FID only — no quote leg,
  no hybrid/EWMA)**, maxFeedAge **600**, maxConfBps **500**, fundingBpsPerStep **0**,
  feedId pinned to the SOL/USD id `ef0d8b6f…c280b56d`, ewmaAlphaBps / ewmaPrice /
  ewmaMaxDevBps / lastEwmaSlot all **0**.
- Market `76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq`: status Active, markPrice
  **70,319,202 E6 (~$70.32)**, maxStaleness 9000, imBps 1000, mmBps 500, liqFeeBps 100,
  tradeFeeBps 0, **maxMoveBps 400**, maxInsDrawBps 2500, **insurance 70,000,000 (70 USDC)**,
  oiLong / oiShort / activeLegs all **0** (no open positions at read time).
- Pyth feed `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE`: verification-level byte = 1
  (Full), feedId matches the pin.

---

## F1 — Matcher `Fill` forwards the core Trade `mode` byte unchecked: hand-off modes 2/3 are reachable, and mode 2 quotes on the wrong side in BOTH orientations (taker extracts the full spread from the LP)

**Severity: High** (conditional on a matcher LP holding an open position on the target
market; repeatable per fill). This is the strongest live-relevant finding in this track.

**Evidence.**

Matcher.lean:187 — the mode byte is read from caller-controlled instruction data and
never range-checked anywhere in `handleFill` (Matcher.lean:178–246):

```lean
let mode        ← loadU8  ix.ptr 1
```

Matcher.lean:223 — it is forwarded verbatim into the core Trade CPI:

```lean
storeU8  data 1  mode
```

The documented/intended surface is only OPEN/CLOSE — repo/src/lean/instructions.ts:246:

```ts
/** matcher op 2. Single-signer taker fill against the LP portfolio. mode OPEN/CLOSE; limitPrice is the worst acceptable price. */
```

But the core implements two more modes — Program.lean:957:

```lean
else if mode == 2 || mode == 3 then
  -- Mode 2 (Long→Long) / Mode 3 (Short→Short): Position Hand-Off (`pflL` seller → `pflS` buyer).
```

In a hand-off, `pflL` is the **seller** and `pflS` is the **buyer**, and the core's own
accounting pins down which side pays the exec-price spread — Program.lean:981–984
(mode 2 calls `applyExecSpread` with the two portfolios swapped relative to mode 3) and
Program.lean:1011:

```lean
let sellerPaid := (mode == 2 && execPrice < markP) || (mode == 3 && execPrice > markP)
```

i.e. in mode 2 the seller is made whole at `execPrice`: below mark the seller pays the
buyer, above mark the buyer pays the seller. A correct mode-2 quote is therefore the
**bid** when the taker is the seller and the **ask** when the taker is the buyer.

The matcher instead picks the quote side — and the pflL/pflS assignment — from
`takerIsLong` alone (Matcher.lean:202–214):

```lean
let execPrice ← if takerIsLong != 0 then do
    let ask ← checkedAdd64 markP delta
    ...
  else do
    let bid := if markP > delta then markP - delta else 1
    ...
let pflL   := if takerIsLong != 0 then takerPfl   else lpPfl
let pflS   := if takerIsLong != 0 then lpPfl      else takerPfl
```

For modes 0/1 that mapping is right (takerIsLong ⇒ taker is the long side). For mode 2
it is inverted in both orientations:

- `takerIsLong = 1`, mode 2: pflL = taker = **seller** of a long, pflS = LP = buyer.
  Matcher quotes the **ask** (mark + δ). Seller receives ask instead of bid → the LP
  buyer pays δ per unit to the taker. The limit check is also inverted for a seller
  (`ask <= limitPrice`, a ceiling, where a seller needs a floor).
- `takerIsLong = 0`, mode 2: pflL = LP = **seller**, pflS = taker = buyer. Matcher
  quotes the **bid** (mark − δ). Buyer pays bid instead of ask → the LP seller is
  shorted δ per unit. Limit check inverted again (`bid >= limitPrice`, a floor, where
  a buyer needs a ceiling).

δ is the full matcher spread, clamped only by the market's maxMoveBps
(Matcher.lean:196–200: `effBps = min(halfSp + qty·impact/POS_SCALE, maxMove)`) — **400 bps
on the live bounty market**. Per-fill transfer is `qty · δ_price` via the core's
`applyExecSpread` (Program.lean:768–788, `mulDiv64Ceil qty diff POS_SCALE`), taken
directly out of LP portfolio capital.

Mode 3 note: with mode 3 the same `takerIsLong` mapping happens to land LP-favorable in
both orientations (the taker overpays by δ). That is a correctness bug against the
taker, not an LP drain — but it confirms the mode byte was never meant to reach the
matcher surface, and that quote semantics differ per mode in ways `handleFill` does
not model.

**Why the Lean proof does not cover it.** The matcher's only theorems are bounds on
the pure quote functions (Matcher.lean:57–84: `quoteAsk_ge_mark`, `quoteBid_le_mark`,
`quoteAsk_within_maxMove`, `quoteBid_within_maxMove`) — they state a quote stays
within maxMove of mark. Nothing proves `mode ∈ {0,1}`, and nothing relates the quote
side to the core's per-mode seller/buyer spread semantics. `handleFill` itself is
unproven imperative code; the hand-off modes live entirely outside the proven model.

**Exploit sketch (bounty market).** Precondition: some matcher LP holds an open long
or short leg on market `76oG…kfFSwq` (OI was 0 at read time, so this needs a victim LP
position to exist first — e.g. after any third party deploys a matcher on the bounty
DEX and takes flow). Attacker, as taker against that LP's cfg:
1. Mode 2, `takerIsLong = 0`: buy the LP's long hand-off at **bid** — attacker pays
   mark − δ, LP capital is debited the δ spread by `applyExecSpread`.
2. Mode 2, `takerIsLong = 1`: sell the same quantity back at **ask** — attacker
   receives mark + δ, again out of LP capital.
Each round trip moves ≈ 2·qty·δ from LP capital to the attacker, δ up to 4% of mark,
bounded per fill only by `maxFillQty`, the buyer's IM check, and the seller's
must-stay-above-totalMm check (Program.lean:1006–1013) — none of which stop repeated
fills as the LP's position is progressively handed over. Endgame: the LP portfolio is
bled of capital while still holding residual position; when it is liquidated
underwater, the shortfall draws market insurance (maxInsDrawBps 2500 → up to 17.5 USDC
of the 70 USDC pot per liquidation) — the route from this bug to the bounty pot.

**Confidence:** confirmed-by-reading for the mode bypass and the mode-2 quote
inversion; needs-devnet-poc for the full repeat-until-liquidation insurance path.

---

## F2 — Matcher config is not bound to any market (or to a DEX/market pair)

**Severity: Medium** (design-level; becomes an LP-capital exploit if a second market
exists — or can be created — on the same DEX).

**Evidence.** The cfg layout stores no market and no DEX — Matcher.lean:36–47
(`CfgOff`): only `owner`, `percProg`, `lpPortfolio`, `halfSpreadBps`, `impactBps`,
`maxFillQty`. `handleInitMatcher` (Matcher.lean:107–147) receives a `dex` account for
the InitPortfolio CPI but never stores it. `handleFill` checks only
`cfg.percProg` (Matcher.lean:184), `cfg.lpPortfolio` (Matcher.lean:185), and that the
supplied market account is owned by the percolator program with the right magic
(Matcher.lean:188–194). The quote is then computed from **the supplied market's**
`markPrice` and `maxMoveBps` (Matcher.lean:196–200).

The core backstop limits the blast radius but does not close it — Program.lean:804–806
requires `dex == market.dex == pflL.dex == pflS.dex`, so cross-DEX fills fail, but any
market **on the same DEX** is accepted.

**Why the Lean proof does not cover it.** The quote theorems are about a single
abstract `markP`; there is no theorem (and no state) tying a matcher cfg to the market
its LP signed up to quote. LP consent to market A's volatility/spread regime is
silently extended to every market on the DEX.

**Exploit sketch.** On the bounty DEX (`6nxb…Fm1CkH`) only marketId 1 is known today,
so this is not presently executable against the pot. If a second market exists or can
be created on that DEX (market-creation policy is core-side), an attacker fills the
victim LP against a thin/rigged market whose mark/maxMove the attacker influences:
the LP takes the other side at a quote derived from the rigged mark, and the loss
lands in the LP portfolio's shared cross-margin capital.

**Confidence:** confirmed-by-reading.

---

## F3 — Initialization front-running: neither the adapter cfg nor the matcher cfg requires its own account's signature at init; first caller becomes admin/owner

**Severity: High for the matcher leg** (theft of LP collateral after victim funding),
**Medium for the adapter leg** (oracle takeover at setup). Setup-window only — the
live bounty cfg is already initialized correctly, so this does **not** touch the
current pot retroactively.

**Evidence.** Adapter `handleInitAdapter` requires only that the *caller* signs, and
that the cfg account is program-owned, writable, long enough, and still zero-magic —
OracleAdapter.lean:509–513:

```lean
SolanaM.require (← AccountView.isSigner admin) Err.missingRequiredSignature
SolanaM.require (← AccountView.isWritable adp) Err.invalidAccountData
SolanaM.require (← pubkeyEq (AccountView.ownerPtr adp) progId) Err.illegalOwner
SolanaM.require ((← AccountView.dataLen adp) >= AdpOff.size) Err.accountDataTooSmall
SolanaM.require ((← loadU32 (AccountView.dataPtr adp) 0) == 0) Err.accountAlreadyInitialized
```

…then binds admin to that caller — OracleAdapter.lean:544:

```lean
pubkeyCopy (aData + AdpOff.admin)       (AccountView.keyPtr admin)
```

Matcher `handleInitMatcher` is identical in shape — Matcher.lean:110–114 (owner
signer; cfg writable/owned/`magic == 0`) and Matcher.lean:126:

```lean
pubkeyCopy (cData + CfgOff.owner)       (AccountView.keyPtr owner)
```

The cfg account itself never signs init: in the TS builders it is a plain writable,
non-signer account — instructions.ts:232 (`matcherInit` keys
`[w(a.cfg), r(a.owner, true), …]`) and instructions.ts:301 (`adapterInit` keys
`[w(a.cfg), r(a.admin, true), …]`).

The shipped flows create the account and initialize it in **separate confirmed
transactions**, leaving a public window: scripts/lean-mainnet-bounty.ts:31–35
(`newAcc` creates + confirms) with the cfg created at line 63 and `adapterInit` sent
separately at line 68; scripts/lean-mainnet-verify.ts:110–112 does the same for a
matcher cfg (`newAcc` × 2, then `matcherInit`).

**Why the Lean proof does not cover it.** All adapter/matcher theorems are pure
price/quote arithmetic. There is no state-machine theorem about initialization
authority, account provenance, or who is entitled to become admin/owner. Entitlement
is purely "called first on a zeroed program-owned account."

**Exploit sketch.** Watch for fresh zeroed accounts owned by the matcher/adapter
programs (program-account subscription). The moment a victim's create-account tx
lands, race their init tx:
- *Matcher:* initialize their cfg with yourself as `owner`, their LP portfolio, and
  the PDA bump. The victim's own verify flow only checks the LP portfolio binding
  (lean-mainnet-verify.ts:113–114 checks `lpPortfolio`, not `owner`). Victim funds the
  LP portfolio — `DepositCollateral` (Program.lean:594–620) requires only the token
  source owner's signature, not the portfolio owner's. You then call `WithdrawLp`
  (Matcher.lean:255–307): the stored-owner check passes for you, and the core
  withdrawal is CPI-signed by the LP PDA — LP collateral lands in your token account.
- *Adapter:* initialize their cfg with yourself as `admin` and your choice of feed /
  pins / tolerances before the market creator's init lands; the market was created
  pointing at this cfg's oracle PDA, so you own its price feed from birth (plus
  `InitEwma`/`PushEwmaMark` rights).

**Confidence:** confirmed-by-reading for the missing authorization; needs-devnet-poc
for the race execution itself.

---

## F4 — Hybrid After-Hours fallback (if enabled) lets a permissionless cranker keep a dead market "fresh" forever, defeating the core's staleness wind-down

**Severity: Medium, conditional — NOT live on the bounty market** (live cfg flags = 2;
hybrid off, all EWMA fields 0). Reported because it is a designed-in adapter behavior
that negates a core safety mechanism on any market that enables it.

**Evidence.** With `FLAG_HYBRID_AH` set, the base feed is parsed with
`allowStale = isHybrid` (OracleAdapter.lean:578, 582–587), so a stale or wide-conf
feed returns `(0, 0)` instead of failing; the crank then falls back to the carried
EWMA price (OracleAdapter.lean:598–604):

```lean
else if isHybrid then do
  -- Hybrid After-Hours fallback: carry last EWMA base mark while underlying market is closed!
  ...
  pure prevEwma
```

It then CPIs `UpdateOracle` with that unchanged price. The core treats every such
crank as a genuine oracle update — Program.lean:544 requires only
`curSlot > lastSlot`, and Program.lean:574 stores `lastOracleSlot = curSlot`. So each
permissionless crank advances the market's oracle freshness clock with a carried
price, and the core `maxStaleness` check (9000 slots on the bounty market) that would
otherwise force wind-down on a dead feed never trips. `PushEwmaMark`
(OracleAdapter.lean:718ff) is admin-only, but plain `Crank` is permissionless and is
sufficient to keep the market alive indefinitely.

**Why the Lean proof does not cover it.** The EWMA theorems (OracleAdapter.lean:
`ewmaBlendNat_bounds`, `ewmaBlendNat_fixed_point`, `ewma_ratio_crank_within_clamp`)
prove the blend is convex, positive, and clamp-bounded. Nothing models freshness or
liveness semantics — i.e. that a carried price presented as a fresh oracle update
disables the core's staleness protection. The clamp proof is satisfied trivially
(diff = 0), which is exactly the problem.

**Exploit sketch (on a hybrid-enabled market, not this one).** Feed dies or its
underlying closes permanently; attacker cranks once per slot with the carried EWMA
mark. Positions that should have been wound down at staleness remain live and
tradeable/liquidatable against a frozen price while the real asset moves elsewhere.

**Confidence:** confirmed-by-reading.

---

## F5 — Composite ratio: one `maxFeedAge` shared across legs with different units, and no divisor floor beyond post-scale `> 0`

**Severity: Low–Medium, conditional — NOT live on the bounty market** (no quote leg;
flags = 2).

**Evidence.** `maxFeedAge` is a single cfg field whose meaning depends on each leg's
format — OracleAdapter.lean:82:

```lean
def maxFeedAge        : UInt64 := 136 -- u64 (slots for Custom/Chainlink/Switchboard; secs for Pyth)
```

(TS side documents the same: instructions.ts:279–280.) In the crank, both legs are
parsed with that same value (base: OracleAdapter.lean:582–584; quote:
OracleAdapter.lean:612–615). A Pyth leg therefore gets the budget in **seconds** while
a Chainlink/Switchboard/Custom leg gets it in **slots** (~0.4 s each) — a mixed-format
composite silently runs legs with mismatched real staleness windows (e.g. maxFeedAge
600 = 600 s for a Pyth base but ~240 s for a Chainlink quote; or, if the admin
intended slots, a Pyth quote leg is allowed 600 slots' worth ≈ 2.5× longer in wall
time than intended — inverted for other pairings).

The ratio itself (OracleAdapter.lean:617–618):

```lean
let ratioP ← mulDiv64 baseP POS_SCALE qP
SolanaM.require (ratioP > 0 && ratioP <= MAX_PRICE) PErr.invalidParams
```

`mulDiv64` truncates toward zero, and the only divisor protection is that the quote
price scaled to a positive E6 value. A quote leg sitting at a tiny positive E6 value
inflates the ratio by orders of magnitude; the per-slot clamp (maxMoveBps) bounds the
speed, not the destination — the mark ratchets the wrong way every slot for as long
as the divisor stays tiny/stale-within-its-window.

**Why the Lean proof does not cover it.** `ratio_crank_within_clamp`
(OracleAdapter.lean:209ff) proves the *clamp bound* for an arbitrary target price —
it assumes the target is meaningful. Nothing proves anything about leg freshness
units or divisor magnitude; the pure model takes `quoteE6` as an opaque Nat.

**Confidence:** confirmed-by-reading (unit sharing + truncation); real-world impact
depends on admin configuration, hence Low–Medium.

---

## F6 — Chainlink legs bypass the confidence gate entirely; Switchboard accepts a single sample

**Severity: Low, conditional — neither format is the live bounty feed** (live feed is
Pyth, whose conf is checked).

**Evidence.** The Chainlink branch returns zero confidence — OracleAdapter.lean:445:

```lean
return (pE6, 0)
```

so the crank's confidence gates (base: OracleAdapter.lean:585–588; quote leg:
OracleAdapter.lean:613–616, `qC <= maxQC`) are vacuous for any Chainlink leg — there
is no on-chain dispersion signal in the OCR2 `Transmissions` entry the parser reads,
and none is synthesized (e.g. from round cadence). The Switchboard branch requires
only OracleAdapter.lean:457:

```lean
SolanaM.require (samples >= 1 && resSlot > 0) OErr.badFeedFormat
```

A `num_samples == 1` result passes; its `std_dev` is then used as the confidence
value, which for a single-sample result carries no meaningful dispersion information.

**Why the Lean proof does not cover it.** The proofs cover scaling monotonicity and
ring-buffer bounds; the confidence gate is an imperative check whose input for these
formats is structurally 0 / degenerate. No theorem states "every accepted price
passed a non-trivial confidence check."

**Confidence:** confirmed-by-reading.

---

## F7 — Adapter funding is computed only when it (almost) cannot be paid: the price step consumes the entire clamp budget first

**Severity: Low / economic.** Live bounty cfg has fundingBpsPerStep = 0, so nothing
is at stake on this market today; included because the "on-chain premium funding"
feature is effectively dead in the common case.

**Evidence.** OracleAdapter.lean:642–651:

```lean
let remBudget := if maxAllowed >= step then maxAllowed - step else 0
...
let (fundSign, fundE6) ← if feedPrice != newP && remBudget > 0 && fundBps > 0 then do
```

Funding is paid only out of the clamp budget **remaining after the price step**.
Whenever the feed has moved by ≥ maxMoveBps — precisely when funding matters most —
the step saturates the budget (`step == maxAllowed`), `remBudget = 0`, and
`fundE6 = 0`. Funding is also zero once the mark has converged (`feedPrice == newP`).
It can only ever be nonzero on partial steps. The pure spec has the same shape
(`clampedFundingNat`, OracleAdapter.lean:153–156).

**Why the Lean proof does not cover it.** `crank_step_and_funding_within_clamp`
proves `diff + fundE6 ≤ maxAllowed` — an upper bound. It does not and cannot prove
funding is ever nonzero; the implementation satisfies the theorem trivially by
paying 0 in the saturated case.

**Confidence:** confirmed-by-reading.

---

## Checked and held

Pyth PriceUpdateV2 (the live bounty feed):
- Discriminator checked; verification level must be exactly 1 (Full) — partial
  verification rejected (OracleAdapter.lean, Pyth branch of `parseOracleFeed`).
- Feed account key **and** owner pinned against cfg; optional feed-id pin enforced
  when FLAG_PIN_FID set — live cfg has it set and the on-chain feedId matches.
- Negative/zero i64 price rejected (sign-bit + `> 0` checks); confidence scaled and
  gated at maxConfBps (500 live).
- Freshness is `publish_time` vs `Clock.unix_timestamp` within maxFeedAge (seconds;
  600 live) **plus** `postedSlot <= curSlot`. No EMA/spot confusion — the spot fields
  are the ones read.
- Exponent handling: negative exponents bounded (`negExp <= 18`), positive-exponent
  path is overflow-checked (`pE6 / mult == rawPrice`) and capped at +6; result must
  land in `(0, MAX_PRICE]`.

Core UpdateOracle consumption (Program.lean:532–578):
- Caller must sign and must equal the market's stored `oracleAuth` — adapter-PDA
  substitution fails (PDA derived from the cfg key + stored bump; core compares the
  key). Direct UpdateOracle by a non-authority is rejected (also in the already-tried
  list).
- STATUS_ACTIVE required; strictly one step per slot (`curSlot > lastSlot`);
  `diff + fundE6 <= maxAllowed` enforced — the adapter cannot exceed the market's
  maxMoveBps in one crank.

Chainlink OCR2:
- Discriminator, `version == 2`, `roundId > 0`, `liveLen > 0` all required; ring
  index `(cursor + len − 1) % len` is proven in-bounds and the entry read is
  length-checked; negative i128 answers rejected; freshness in slots against the
  entry's slot field (layout matches the repo's own SVM harness fixtures);
  decimals bounded by `scaleU128ToE6` (`dec <= 18`).

Switchboard On-Demand:
- Discriminator, sign checks on value and std_dev, zero value rejected, slot
  freshness enforced, 128-bit → E6 division is overflow-checked
  (`divU128By64` requires nonzero denom and `hi < denom`).

Custom FEED:
- Magic checked, price must be in `(0, MAX_PRICE]`, publishSlot freshness in slots.

Adapter crank / config (post-init):
- Cfg magic/owner verified; supplied percolator program and market keys must equal
  the cfg's stored bindings; feed key + owner pinning re-enforced on **every** crank;
  quote leg (when present) is parsed with `allowStale = false` — strictly live.
- `InitEwma` is admin-only and strictly one-shot; `PushEwmaMark` is admin-only,
  once per slot, deviation-bounded by `ewmaMaxDevBps`. No post-init instruction can
  mutate feed pins, admin, or tolerances (there is no Update op).

Matcher (modes 0/1 surface):
- `UpdateMatcher` and `WithdrawLp` require the stored cfg owner as signer.
- Fill requires: not paused, percProg key match, LP portfolio key match, market
  program-owned with market magic, `0 < qty <= maxFillQty`, and a limit-price check
  that is side-correct for modes 0/1.
- `effBps` is clamped to the market's `maxMoveBps` regardless of cfg params;
  `impactBps` is unbounded at init/update but only the cfg owner can set it and the
  clamp neutralizes it at fill time (owner self-harm at worst).
- Quote rounding: `delta` uses floor division (`mulDiv64`), systematically
  taker-favorable by < 1 E6 price unit per fill — negligible, and the pure spec
  mirrors the implementation exactly.

Core Trade backstops relevant to matcher abuse:
- Aliased portfolios rejected (`pflL != pflS`) — no literal self-fill; both
  portfolios' stored owner fields are verified against the CPI signers
  (Program.lean:804–808), and dex consistency is enforced across market + both
  portfolios, which is what confines F2 to same-DEX markets.
- Same-owner two-portfolio wash trades are net-zero minus fees, and fees flow
  **into** insurance — wash trading cannot manufacture PnL or drain the pot.
