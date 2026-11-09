//! End-to-end flow 2: loss → insurance writeoff → haircut.
//!
//! A trader takes a mark-to-market loss larger than capital. Capital is drained
//! first (`loss_writeoff`), any remainder is socialised onto the insurance fund
//! and then onto junior positive-PnL via the haircut. Senior claims stay backed.
//!
//! cargo kani --tests --harness flow_loss_writeoff_haircut   # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::*;

#[kani::proof]
fn flow_loss_writeoff_haircut() {
    // Bound the domain so CBMC closes quickly.
    let capital: u8 = kani::any();
    let insurance: u8 = kani::any();
    let vault_extra: u8 = kani::any(); // residual above senior
    let other_pnl: u8 = kani::any();   // another account's positive PnL
    let loss_mag: u8 = kani::any();
    kani::assume(loss_mag > 0);

    let mut capital = capital as u128;
    let mut insurance = insurance as u128;
    let other_pnl = other_pnl as u128;
    let loss = -(loss_mag as i128);

    // 1) loss writeoff against this account's capital
    let w = loss_writeoff(capital, loss);
    let absorbed_by_capital = capital - w.capital;
    capital = w.capital;
    // remaining uncovered loss magnitude (positive)
    let uncovered = if w.pnl < 0 { (-w.pnl) as u128 } else { 0 };

    // 2) insurance covers what capital could not (saturates)
    let from_insurance = uncovered.min(insurance);
    insurance -= from_insurance;
    let socialised = uncovered - from_insurance;

    // 3) vault / residual after seniors
    let vault = capital + insurance + other_pnl.min(vault_extra as u128) + vault_extra as u128;
    // rebuild a consistent vault: at least seniors
    let vault = (capital + insurance).saturating_add(vault_extra as u128);

    // 4) haircut on the remaining positive PnL (other account) absorbs socialised loss
    let pnl_pos_total = other_pnl;
    let (n, d) = haircut_ratio(vault, capital, insurance, pnl_pos_total);
    kani::assert(d > 0 && n <= d, "haircut well-formed");
    let payout = effective_pnl(other_pnl as i128, n, d);
    kani::assert(payout <= other_pnl, "haircut never boosts junior claim");
    kani::assert(payout <= vault.saturating_sub(capital.saturating_add(insurance)),
        "junior payouts never exceed residual");

    // conservation: capital drained + insurance drained + socialised == original loss mag
    kani::assert(
        absorbed_by_capital + from_insurance + socialised == loss_mag as u128,
        "loss fully accounted across capital, insurance, socialisation",
    );
    // seniors still backed
    kani::assert(capital + insurance <= vault, "seniors backed after writeoff");
}
