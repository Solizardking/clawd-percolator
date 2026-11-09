//! Example 3: fee split where both legs round up.
//!
//! Bug: protocol and LP shares are each computed with ceiling division, so the
//! two legs can sum to more than the fee actually collected.
//!
//! cargo kani --tests --harness ex03_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex03_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::math::{mul_div_ceil, mul_div_floor};
use kani_clawd::token::assert_split_conserves;

fn split_vulnerable(fee: u128, protocol_bps: u16) -> (u128, u128) {
    let protocol = mul_div_ceil(fee, protocol_bps as u128, 10_000).unwrap();
    let lp = mul_div_ceil(fee, 10_000 - protocol_bps as u128, 10_000).unwrap();
    (protocol, lp)
}

fn split_fixed(fee: u128, protocol_bps: u16) -> (u128, u128) {
    let protocol = mul_div_floor(fee, protocol_bps as u128, 10_000).unwrap();
    (protocol, fee - protocol)
}

fn check(f: fn(u128, u16) -> (u128, u128)) {
    let fee: u16 = kani::any();
    let bps: u16 = kani::any();
    kani::assume(bps <= 10_000);
    assert_split_conserves(fee as u128, |t| f(t, bps));
}

#[kani::proof]
fn ex03_vulnerable() {
    check(split_vulnerable);
}

#[kani::proof]
fn ex03_fixed() {
    check(split_fixed);
}
