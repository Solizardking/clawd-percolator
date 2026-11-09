//! Example 4: funding that rounds in the trader's favour on both sides.
//!
//! Bug: payments use floor division, so the payer rounds down while the
//! receiver rounds up: each funding interval can mint one unit from nothing.
//!
//! cargo kani --tests --harness ex04_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex04_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::funding_payment;

fn funding_vulnerable(pos: i128, delta: i128, scale: u128) -> Option<i128> {
    Some((pos.checked_mul(delta)?).div_euclid(scale as i128))
}

fn check(f: fn(i128, i128, u128) -> Option<i128>) {
    let pos: i8 = kani::any();
    let delta: i8 = kani::any();
    let scale: u8 = kani::any();
    kani::assume(scale > 0);
    let long = f(pos as i128, delta as i128, scale as u128).unwrap();
    let short = f(-(pos as i128), delta as i128, scale as u128).unwrap();
    kani::assert(long + short >= 0, "funding never pays out more than it collects");
}

#[kani::proof]
fn ex04_vulnerable() {
    check(funding_vulnerable);
}

#[kani::proof]
fn ex04_fixed() {
    check(funding_payment);
}
