//! Example 7 (proof.md §8.2): staking multiplier decrease.
//!
//! Bug: the multiplier is `max_bps - slope * duration`, so longer locks earn
//! *less* — the opposite of the staking invariant.
//!
//! cargo kani --tests --harness ex07_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex07_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::staking::{assert_multiplier_monotone, linear_multiplier_bps};

fn multiplier_vulnerable(duration: u64, max_lock: u64, max_bps: u16) -> u128 {
    let top = (max_bps as u32).max(10_000) as u128;
    if max_lock == 0 {
        return 10_000;
    }
    let step = (top - 10_000) * (duration.min(max_lock) as u128) / max_lock as u128;
    top.saturating_sub(step) // BUG: decreases with duration
}

fn check(m: fn(u64, u64, u16) -> u128) {
    // Concrete max_lock values keep the division circuit SAT-tractable.
    for max_lock in [1u8, 2, 4, 8, 16, 32, 64, 128, 255] {
        let max_bps: u16 = kani::any();
        kani::assume(max_bps >= 10_000 && max_bps <= 12_000);
        let d1: u8 = kani::any();
        let d2: u8 = kani::any();
        kani::assume(d1 <= d2);
        assert_multiplier_monotone(d1 as u64, d2 as u64, |d| m(d, max_lock as u64, max_bps));
    }
}

#[kani::proof]
#[kani::unwind(10)]
fn ex07_vulnerable() { check(multiplier_vulnerable); }

#[kani::proof]
#[kani::unwind(10)]
fn ex07_fixed() { check(linear_multiplier_bps); }
