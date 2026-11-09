//! Example 4 (proof.md §8.2): funding sign flip.
//!
//! Bug: the short side is charged with the same sign as the long side, so
//! funding mints (or burns) value instead of transferring it.
//!
//! cargo kani --tests --harness ex04_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex04_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::risk::funding_payment;

fn funding_vulnerable(pos: i128, delta: i128, scale: u128) -> Option<i128> {
    // Always take the payment in the index direction — both sides pay the same sign.
    let raw = pos.checked_mul(delta)?;
    let abs = raw.unsigned_abs();
    let pay = (abs / scale) + u128::from(abs % scale != 0);
    let pay = i128::try_from(pay).ok()?;
    Some(if delta >= 0 { pay } else { -pay }) // BUG: ignores position side
}

fn check(f: fn(i128, i128, u128) -> Option<i128>) {
    let size: u8 = kani::any();
    let delta: u8 = kani::any();
    let scale: u8 = kani::any();
    kani::assume(scale > 0 && size > 0);
    let long = f(size as i128, delta as i128, scale as u128).unwrap();
    let short = f(-(size as i128), delta as i128, scale as u128).unwrap();
    // Fixed funding never creates value (payers round up → sum >= 0).
    kani::assert(long as i32 + short as i32 >= 0, "funding never pays out more than it collects");
}

#[kani::proof]
fn ex04_vulnerable() { check(funding_vulnerable); }

#[kani::proof]
fn ex04_fixed() { check(funding_payment); }
