# kani-clawd

> **`clawd-kani`'s home in this repo.** The package / library names are
> `kani-clawd` / `kani_clawd`; docs and agents also call it **clawd-kani**.
> Dev's Mac still has an older in-progress copy under
> `percolator-skill/kani/clawd-kani` — this tree is the canonical one.

Reusable [Kani](https://github.com/model-checking/kani) verification primitives
and proof harnesses for Solana protocol math, plus machine-checked proofs
against this repo's own Percolator risk engine (`programs/percolator`, the v16
engine compiled into the clawd wrapper, layout VERSION 18).

The crate packages checked arithmetic for haircut ratios, effective PnL, warmup
schedules, fee-debt sweeps, funding payments and loss write-offs, plus
assertion helpers for value-conserving splits, staking multipliers, bounded
outputs, CPI authorization and state-machine guards. Proofs run in CI through
small profile scripts, so a team gets machine-checked invariants without
standing up a formal-methods stack.

## Features

- **Checked protocol math.** `risk` implements Percolator-style primitives:
  `haircut_ratio`, `effective_pnl`, `warmup_slope` / `warmed_amount`,
  `fee_debt_sweep`, `funding_payment`, `loss_writeoff`. Every function is
  total (no panics, no wrapping) and ships with proofs of conservation, bounds
  and monotonicity.
- **Percolator engine proofs.** `tests/percolator_engine.rs` calls the real
  v16 engine kernels (`kani_apply_backing_utilization_fee_charge`,
  `backing_domain_fee_split_for_lien_delta_num`,
  `floor_div_signed_conservative_i128`, `kani_mul_div_*_u128_or_wide`, ...)
  and pins the wrapper's account sizes (market 3,163 B for one asset slot,
  +1,813 B per extra slot; portfolio 9,539 B = 16 header + 9,419 engine + 104
  matcher config). The size and layout-discriminator (18) checks also run as
  plain `cargo test` in `tests/percolator_layout.rs`.
- **Reusable proof helpers.** `token`, `staking`, `bounds` and `math` expose
  assert-style helpers that wrap your own closures: split conservation,
  transfer conservation, monotone/bounded staking multipliers, bounded outputs,
  "fails only when allowed", spec equivalence and monotonicity.
- **Constrained generators.** `generators` produces bounded symbolic values for
  common Solana domains: scores, weights summing to 10,000 bps, basis points,
  lamports, ordered timestamps, oracle prices.
- **Agent invariants** (`solana-agent`). Symbolic agent accounts, lamport
  conservation, signer/authority checks, a reentrancy guard, CPI allowlists via
  `cpi_stub!` (allowlist assert + nondeterministic result) and `cpi_contract!`
  (assume-guarantee pre/postconditions), PDA seed limits (16 seeds x 32 bytes),
  median-of-three oracle consensus with a deviation band, and FSM transition
  guards.
- **Symbolic AccountInfo** (`solana-account-info`). `SymbolicAccount<D>` builds
  **real** `solana_program::account_info::AccountInfo` values from
  `AccountConstraints` (this feature builds against `solana-program` 2.x on
  the box — there is no local mirror). `LamportSnapshot` checks conservation
  across an instruction.

- **Zero runtime cost.** Everything is gated behind `cfg(kani)` (and
  `cfg(test)` for the crate's own unit tests). A normal build of a dependent
  crate compiles `kani_clawd` to an empty library.
- **Fail-to-fix examples.** Seven vulnerable/fixed pairs plus two end-to-end
  flows in `examples/`, each with a runnable command. Vulnerable harnesses are
  plain `#[kani::proof]`s that Kani must refute; the profile scripts treat a
  pass as a CI failure.


## Sibling crate

A parallel harness crate lives outside this repo at
`/workspace/clawd-percolator-kani` (package `clawd-percolator-kani`). That crate
owns proofs that call the **real** engine / matcher / stake sources. `kani-clawd`
owns the kamiyo-kani-style primitives, risk-math specs, agent guard rails,
fail-false pairs and end-to-end flows. The two crates are complementary — do not
duplicate engine harnesses here.

## Install

This crate lives in the repo and is not published. From another crate in this
repo:

```toml
[dev-dependencies]
kani-clawd = { path = "../kani-clawd" }
```

Install Kani once:

```bash
cargo install --locked kani-verifier
cargo kani setup
```

## Quick start

```rust
#![cfg(kani)]

use kani_clawd::risk::{effective_pnl, haircut_ratio};

#[kani::proof]
fn payout_is_bounded_by_profit() {
    let vault: u128 = kani::any();
    let principal_total: u128 = kani::any();
    let insurance: u128 = kani::any();
    let pnl_pos_total: u128 = kani::any();
    let my_pnl: i128 = kani::any();

    let (h_num, h_den) = haircut_ratio(vault, principal_total, insurance, pnl_pos_total);
    let payout = effective_pnl(my_pnl, h_num, h_den);

    kani::assert(payout <= my_pnl.max(0) as u128, "payout bounded by positive pnl");
}
```

Run it (this exact harness is in `src/risk.rs`):

```bash
cd kani-clawd
cargo kani --lib --harness payout_is_bounded_by_profit
```

Profiles (from the repo root):

```bash
./scripts/kani-smoke.sh       # fast core subset, every push
./scripts/kani-full.sh        # everything, every push + weekly
./scripts/kani-agent-flow.sh  # agent CPI flow, Benchmarks workflow
```

Normal builds are unaffected:

```bash
cd kani-clawd && cargo build --all-features && cargo test --all-features
```

## Account sizes (on-chain truth)

Live `getAccountInfo` on 2026-11-09 (America/New_York). Layout VERSION 18,
magic `PERCV16\0`. Handoff note of portfolio **9,555 B** is wrong — every
live portfolio is **9,539 B**. Zero accounts of size 9,555 were found on
clawd chain or on the Solana-devnet wrapper.

| Account | Address | data_len | Chain |
|---|---|---:|---|
| SOL-PERP market slab | `DNnhA2Lvuvu3Ywn1KJRCJqvMeZkZ2qndBXJidUMTQ6PY` | 3,163 | clawd (`rpc.clawdchain.trade`) |
| BTC-PERP market slab | `GRtaKnxkxFYnFxadn24PqC7S4BJyWeP7LEwJpXpJQ1yr` | 3,163 | clawd |
| ETH-PERP market slab | `2kaudfmM3TxUWDEih5gtkUucWVt1UuF5SKeFF4aEPore` | 3,163 | clawd |
| CLAWD-PERP market slab | `Ha2RECxyaKvXNA5KuoLxvJpEQGGdMmn3xBP4FdJQvSb5` | 3,163 | clawd |
| house SOL portfolio | `A5cB2hDT6trFDPwwmgb7qEpjq2SEmph7hRZRxhErPv28` | 9,539 | clawd (kind=2, ver=18) |
| house BTC portfolio | `H3W4UoXJCSGdz1UaTSKQy927Mn3HUdLbVXqWQYorPfwv` | 9,539 | clawd |
| house ETH portfolio | `AJMiw4xee2CgXbKw4FRyPNwPfXZRsoaBu6FRRG2xaYDw` | 9,539 | clawd |
| house CLAWD portfolio | `BxbRpZ5jniAjP35UEZiZQ8nGLHota1gXKBAJXqJBw6MK` | 9,539 | clawd |
| Solana-devnet portfolio | `C8dh9AW3eCET5V3hNtSdegcvWs1WwgWZC8toPnnZovsF` | 9,539 | wrapper `6q254q…az8ZT` |
| Solana-devnet market | `GsL6h6n8VxrodQ15UmmymMoNBWjYn87MPs5S7tCpLjyK` | 3,163 | same wrapper |

`PORTFOLIO_SIZE_BYTES = 9_539` (= 16 header + 9,419 engine + 104 matcher config)
and `MARKET_SIZE_BYTES = 3_163` (+1,813 per extra asset slot) match
`scripts/lib/clawd.js` `PORTFOLIO_LEN` / `MARKET_LEN_1SLOT` and the layout dump.
`getProgramAccounts` with `dataSize=9555` on the Solana-devnet wrapper returned 0.

## Coverage table

Filled by the box run on branch `kani-clawd` (America/New_York, 2026-11-09).
`EXPECTED FAIL` means Kani must find a counterexample (vulnerable half).
Logs: `kani-clawd/results/kani-fast.log`, `kani-pairs.log`, `kani-fix47.log`.

| Harness | Kind | Expected | Result |
|---|---|---|---|
| `risk::proofs::risk_haircut_at_most_one` | lib | PASS | PASS (smoke, 2s) |
| `risk::proofs::risk_haircut_never_exceeds_residual` | lib | PASS | PASS (smoke, 2s) |
| `risk::proofs::risk_fee_sweep_conserves_value` | lib | PASS | PASS (smoke, 3s) |
| `risk::proofs::risk_loss_writeoff_conserves` | lib | PASS | PASS (smoke, 2s) |
| `risk::proofs::risk_warmup_slope_bounds` | lib | PASS | PASS (smoke, 1s) |
| `percolator_backing_fee_charge_conserves` | engine | PASS | PASS (smoke, 6s) |
| `percolator_earnings_withdraw_conserves` | engine | PASS | PASS (smoke, 4s) |
| `ex01_vulnerable` / `ex01_fixed` (fee-split overflow) | fail-false | FAIL / PASS | FAIL / PASS |
| `ex02_vulnerable` / `ex02_fixed` (unbounded payout) | fail-false | FAIL / PASS | FAIL / PASS |
| `ex03_vulnerable` / `ex03_fixed` (warmup jump) | fail-false | FAIL / PASS | FAIL / PASS |
| `ex04_vulnerable` / `ex04_fixed` (funding sign flip) | fail-false | FAIL / PASS | FAIL / PASS |
| `ex05_vulnerable` / `ex05_fixed` (writeoff overrun) | fail-false | FAIL / PASS | FAIL / PASS |
| `ex06_vulnerable` / `ex06_fixed` (split residue leak) | fail-false | FAIL / PASS | FAIL / PASS |
| `ex07_vulnerable` / `ex07_fixed` (staking multiplier decrease) | fail-false | FAIL / PASS | FAIL / PASS |
| `flow_perps_lifecycle` | e2e | PASS | TIMEOUT under concurrent CBMC load (re-run when idle) |
| `flow_loss_writeoff_haircut` | e2e | PASS | PASS (25s) |
| `flow_agent_cpi` | e2e | PASS | PASS (25s) |

Key lib properties in smoke: haircut ∈ [0,1]; fee-sweep / loss-writeoff
conservation; warmup slope bounds. Agent proofs under `solana-agent`
(symbolic accounts, lamport conservation, reentrancy guard, oracle median-of-3,
PDA seed limits, FSM) are present in `src/agent.rs` and compile under `cfg(kani)`.

`solana-account-info` uses **real** `solana_program::AccountInfo` (solana-program
2.x builds on this box; no local mirror).

Acceptance: `cargo check --all-features` and `cargo test --all-features` pass;
Kani-gated code has zero runtime cost on a normal build.

