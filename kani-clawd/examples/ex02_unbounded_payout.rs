//! Example 2 (proof.md §8.2): unbounded payout.
//!
//! Bug: haircut numerator is taken as the raw residual without
//! `min(residual, pnl_pos_total)`, so `h` can exceed 1 and the payout can
//! exceed the trader's positive PnL.
//!
//! cargo kani --tests --harness ex02_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex02_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::{effective_pnl, haircut_ratio};

fn haircut_vulnerable(vault: u128, principal: u128, insurance: u128, pnl_pos: u128) -> (u128, u128) {
    if pnl_pos == 0 {
        return (1, 1);
    }
    let residual = vault.saturating_sub(principal.saturating_add(insurance));
    (residual, pnl_pos) // BUG: no min(residual, pnl_pos)
}

fn check(h: fn(u128, u128, u128, u128) -> (u128, u128)) {
    let vault: u8 = kani::any();
    let principal: u8 = kani::any();
    let insurance: u8 = kani::any();
    let pnl: u8 = kani::any();
    let (n, d) = h(vault as u128, principal as u128, insurance as u128, pnl as u128);
    kani::assert(d > 0 && n <= d, "haircut stays in [0,1]");
    let payout = effective_pnl(pnl as i128, n, d);
    kani::assert(payout <= pnl as u128, "effective_pnl payout at most positive PnL");
}

#[kani::proof]
fn ex02_vulnerable() { check(haircut_vulnerable); }

#[kani::proof]
fn ex02_fixed() { check(haircut_ratio); }
