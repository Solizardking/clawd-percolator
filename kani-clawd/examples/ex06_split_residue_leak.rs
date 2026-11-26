//! Example 6 (proof.md §8.2): split residue leak.
//!
//! Bug: a three-way bps split floors every leg and drops the remainder, so
//! `a + b + c < total` and the residue leaks out of the system.
//!
//! cargo kani --tests --harness ex06_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex06_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::math::mul_div_floor;
use kani_clawd::token::assert_split_conserves_n;

fn split_vulnerable(total: u128, a_bps: u16, b_bps: u16) -> [u128; 3] {
    let a = mul_div_floor(total, a_bps as u128, 10_000).unwrap_or(0);
    let b = mul_div_floor(total, b_bps as u128, 10_000).unwrap_or(0);
    let c = mul_div_floor(total, (10_000 - a_bps - b_bps) as u128, 10_000).unwrap_or(0);
    [a, b, c] // BUG: remainder discarded
}

fn split_fixed(total: u128, a_bps: u16, b_bps: u16) -> [u128; 3] {
    let a = mul_div_floor(total, a_bps as u128, 10_000).unwrap_or(0);
    let b = mul_div_floor(total, b_bps as u128, 10_000).unwrap_or(0);
    let c = total.saturating_sub(a).saturating_sub(b); // remainder stays in-system
    [a, b, c]
}

fn check(f: fn(u128, u16, u16) -> [u128; 3]) {
    let total: u8 = kani::any();
    let a_bps: u16 = kani::any();
    let b_bps: u16 = kani::any();
    kani::assume((a_bps as u32) + (b_bps as u32) <= 10_000);
    assert_split_conserves_n(total as u128, |t| f(t, a_bps, b_bps));
}

#[kani::proof]
fn ex06_vulnerable() { check(split_vulnerable); }

#[kani::proof]
fn ex06_fixed() { check(split_fixed); }
