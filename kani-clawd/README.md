# kani-clawd

> Also referred to as **`clawd-kani`** in the Clawd Chain docs. The package and
> library names are `kani-clawd` / `kani_clawd`.

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
  real `solana_program::account_info::AccountInfo` values from
  `AccountConstraints`; `LamportSnapshot` checks conservation across an
  instruction.
- **Zero runtime cost.** Everything is gated behind `cfg(kani)` (and
  `cfg(test)` for the crate's own unit tests). A normal build of a dependent
  crate compiles `kani_clawd` to an empty library.
- **Fail-to-fix examples.** Seven vulnerable/fixed pairs plus two end-to-end
  flows in `examples/`, each with a runnable command. Vulnerable harnesses are
  plain `#[kani::proof]`s that Kani must refute; the profile scripts treat a
  pass as a CI failure.

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
