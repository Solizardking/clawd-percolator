//! Example 2: haircut that forgets the insurance fund is senior.
//!
//! Bug: residual is computed as `vault - principal`, so positive PnL is paid
//! out of the insurance fund.
//!
//! cargo kani --tests --harness ex02_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex02_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::{effective_pnl, haircut_ratio, junior_residual};

fn haircut_vulnerable(vault: u128, principal: u128, _insurance: u128, pnl_pos: u128) -> (u128, u128) {
    if pnl_pos == 0 {
        return (1, 1);
    }
    let residual = vault.saturating_sub(principal);
    (residual.min(pnl_pos), pnl_pos)
}

fn check(h: fn(u128, u128, u128, u128) -> (u128, u128)) {
    let vault: u16 = kani::any();
    let principal: u16 = kani::any();
    let insurance: u16 = kani::any();
    let pnl: u16 = kani::any();
    let (v, p, i) = (vault as u128, principal as u128, insurance as u128);
    let (n, d) = h(v, p, i, pnl as u128);
    let payout = effective_pnl(pnl as i128, n, d);
    kani::assert(payout <= junior_residual(v, p, i), "positive PnL never paid from principal or insurance");
}

#[kani::proof]
fn ex02_vulnerable() {
    check(haircut_vulnerable);
}

#[kani::proof]
fn ex02_fixed() {
    check(haircut_ratio);
}
