//! End-to-end flow 2: loss → insurance writeoff → haircut.
//!
//! A trader takes a mark-to-market loss larger than capital. Capital is drained
//! first (`loss_writeoff`), any remainder (`written_off`) is socialised onto the
//! insurance fund and then onto junior positive-PnL via the haircut. Senior
//! claims stay backed.
//!
//! cargo kani --tests --harness flow_loss_writeoff_haircut   # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::*;

#[kani::proof]
fn flow_loss_writeoff_haircut() {
    // Narrow domain so CBMC closes quickly.
    let capital: u8 = kani::any();
    let insurance: u8 = kani::any();
    let vault_extra: u8 = kani::any();
    let other_pnl: u8 = kani::any();
    let loss_mag: u8 = kani::any();
    kani::assume(loss_mag > 0);

    let capital0 = capital as u128;
    let mut insurance = insurance as u128;
    let other_pnl = other_pnl as u128;
    let loss = -(loss_mag as i128);

    // 1) loss writeoff against this account's capital (pnl forced to 0)
    let w = loss_writeoff(capital0, loss);
    kani::assert(w.pnl == 0, "writeoff clears the negative PnL");
    kani::assert(w.covered + w.written_off == loss_mag as u128, "covered+written_off = |loss|");
    kani::assert(w.capital + w.covered == capital0, "capital drained exactly by covered");

    // 2) insurance absorbs what capital could not
    let from_insurance = w.written_off.min(insurance);
    insurance -= from_insurance;
    let socialised = w.written_off - from_insurance;

    // 3) vault still covers seniors; residual backs junior PnL
    let capital = w.capital;
    let vault = (capital + insurance).saturating_add(vault_extra as u128);
    let (n, d) = haircut_ratio(vault, capital, insurance, other_pnl);
    kani::assert(d > 0 && n <= d, "haircut well-formed");
    let payout = effective_pnl(other_pnl as i128, n, d);
    kani::assert(payout <= other_pnl, "haircut never boosts junior claim");
    kani::assert(
        payout <= vault.saturating_sub(capital.saturating_add(insurance)),
        "junior payouts never exceed residual",
    );

    // conservation across the waterfall
    kani::assert(
        w.covered + from_insurance + socialised == loss_mag as u128,
        "loss fully accounted across capital, insurance, socialisation",
    );
    kani::assert(capital + insurance <= vault, "seniors backed after writeoff");
}
