//! Example 5 (proof.md §8.2): writeoff overrun.
//!
//! Bug: capital is subtracted by the full |loss| with wrapping, so a loss
//! larger than capital wraps the account to a huge balance.
//!
//! cargo kani --tests --harness ex05_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex05_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::loss_writeoff;

fn writeoff_vulnerable(capital: u128, pnl: i128) -> (u128, i128) {
    if pnl >= 0 {
        return (capital, pnl);
    }
    let loss = pnl.unsigned_abs();
    (capital.wrapping_sub(loss), 0) // BUG: wrapping underflow
}

fn check(f: fn(u128, i128) -> (u128, i128)) {
    let capital: u8 = kani::any();
    let loss_mag: u8 = kani::any();
    kani::assume(loss_mag > 0);
    let (c, p) = f(capital as u128, -(loss_mag as i128));
    kani::assert(c <= capital as u128, "writeoff never increases capital");
    kani::assert(p == 0, "negative PnL is cleared");
    kani::assert(capital as u128 - c <= loss_mag as u128, "covered amount bounded by |loss|");
}

#[kani::proof]
fn ex05_vulnerable() { check(writeoff_vulnerable); }

#[kani::proof]
fn ex05_fixed() {
    let capital: u8 = kani::any();
    let loss_mag: u8 = kani::any();
    kani::assume(loss_mag > 0);
    let w = loss_writeoff(capital as u128, -(loss_mag as i128));
    kani::assert(w.capital <= capital as u128, "writeoff never increases capital");
    kani::assert(w.pnl == 0, "negative PnL is cleared");
    kani::assert(w.covered + w.written_off == loss_mag as u128, "loss fully accounted");
    kani::assert(w.covered <= capital as u128, "covered bounded by capital");
}
