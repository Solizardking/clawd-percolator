//! Example 3 (proof.md §8.2): warmup jump.
//!
//! Bug: warmed amount is computed as `slope * elapsed` with no cap, so a long
//! elapsed interval jumps past the PnL being warmed and over-pays capital.
//!
//! cargo kani --tests --harness ex03_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex03_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::{warmup_slope, warmed_amount};

fn warmed_vulnerable(slope: u128, elapsed: u64, _cap: u128) -> u128 {
    slope.saturating_mul(elapsed as u128) // BUG: no min(cap, ...)
}

fn check(w: fn(u128, u64, u128) -> u128) {
    let pnl: u8 = kani::any();
    let period: u8 = kani::any();
    let elapsed: u8 = kani::any();
    let slope = warmup_slope(pnl as u128, period as u64);
    let got = w(slope, elapsed as u64, pnl as u128);
    kani::assert(got <= pnl as u128, "warmup never exceeds the PnL being released");
}

#[kani::proof]
fn ex03_vulnerable() { check(warmed_vulnerable); }

#[kani::proof]
fn ex03_fixed() { check(warmed_amount); }
