//! Example 1 (proof.md §8.2): unchecked fee-split overflow.
//!
//! Bug: both protocol and LP shares multiply in `u32` with wrapping, so a large
//! fee overflows and the parts no longer sum to the fee collected.
//!
//! cargo kani --tests --harness ex01_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex01_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::math::mul_div_floor;
use kani_clawd::token::assert_split_conserves;

fn split_vulnerable(fee: u128, protocol_bps: u16) -> (u128, u128) {
    let fee32 = fee as u32;
    let protocol = fee32.wrapping_mul(protocol_bps as u32) / 10_000;
    let lp = fee32.wrapping_mul((10_000u16.saturating_sub(protocol_bps)) as u32) / 10_000;
    (protocol as u128, lp as u128)
}

fn split_fixed(fee: u128, protocol_bps: u16) -> (u128, u128) {
    let protocol = mul_div_floor(fee, protocol_bps as u128, 10_000).unwrap_or(0);
    (protocol, fee.saturating_sub(protocol))
}

fn check(f: fn(u128, u16) -> (u128, u128)) {
    let fee: u32 = kani::any();
    let bps: u16 = kani::any();
    kani::assume(bps <= 10_000);
    assert_split_conserves(fee as u128, |t| f(t, bps));
}

#[kani::proof]
fn ex01_vulnerable() { check(split_vulnerable); }

#[kani::proof]
fn ex01_fixed() { check(split_fixed); }
