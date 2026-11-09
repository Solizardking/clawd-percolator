//! Constrained symbolic generators for common Solana domains (Kani only).
//!
//! Each generator returns a `kani::any()` value already narrowed with
//! `kani::assume`, so harnesses state their domain in one call.

/// Lamports per SOL.
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
/// Total SOL supply bound used to keep lamport sums from overflowing `u64` in practice.
pub const MAX_SUPPLY_LAMPORTS: u64 = 600_000_000 * LAMPORTS_PER_SOL;
/// Basis-point denominator.
pub const BPS_DENOM: u16 = 10_000;

/// Any `u128` in `0..=max`.
pub fn u128_up_to(max: u128) -> u128 {
    let v: u128 = kani::any();
    kani::assume(v <= max);
    v
}

/// Any `u64` in `0..=max`.
pub fn u64_up_to(max: u64) -> u64 {
    let v: u64 = kani::any();
    kani::assume(v <= max);
    v
}

/// Any `i128` in `-bound..=bound`.
pub fn i128_within(bound: u128) -> i128 {
    let v: i128 = kani::any();
    kani::assume(v.unsigned_abs() <= bound);
    v
}

/// Basis points in `0..=10_000`.
pub fn bps() -> u16 {
    let v: u16 = kani::any();
    kani::assume(v <= BPS_DENOM);
    v
}

/// A normalized score in `0..=max_score` (e.g. reputation, health scores).
pub fn score(max_score: u32) -> u32 {
    let v: u32 = kani::any();
    kani::assume(v <= max_score);
    v
}

/// `N` weights, each in `0..=10_000`, summing to exactly 10_000 bps.
pub fn weights_bps<const N: usize>() -> [u16; N] {
    let mut w = [0u16; N];
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < N {
        w[i] = bps();
        sum += w[i] as u32;
        i += 1;
    }
    kani::assume(sum == BPS_DENOM as u32);
    w
}

/// Lamports in `0..=max` (pass [`MAX_SUPPLY_LAMPORTS`] for realistic balances).
pub fn lamports(max: u64) -> u64 {
    u64_up_to(max)
}

/// Strictly positive lamports in `1..=max`.
pub fn nonzero_lamports(max: u64) -> u64 {
    let v = lamports(max);
    kani::assume(v > 0);
    v
}

/// Two slot/unix timestamps with `t0 <= t1` and `t1 - t0 <= max_gap`.
pub fn ordered_timestamps(max_gap: u64) -> (u64, u64) {
    let t0: u64 = kani::any();
    let gap = u64_up_to(max_gap);
    kani::assume(t0.checked_add(gap).is_some());
    (t0, t0 + gap)
}

/// Three ordered timestamps `t0 <= t1 <= t2`.
pub fn ordered_timestamps3() -> (u64, u64, u64) {
    let t0: u64 = kani::any();
    let t1: u64 = kani::any();
    let t2: u64 = kani::any();
    kani::assume(t0 <= t1 && t1 <= t2);
    (t0, t1, t2)
}

/// An oracle price in `1..=max_price` (Percolator bounds prices by `MAX_ORACLE_PRICE`).
pub fn oracle_price(max_price: u64) -> u64 {
    let v = u64_up_to(max_price);
    kani::assume(v > 0);
    v
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn generators_respect_domains() {
        kani::assert(bps() <= 10_000, "bps domain");
        let (a, b) = ordered_timestamps(1_000);
        kani::assert(a <= b && b - a <= 1_000, "ordered timestamps");
        let w = weights_bps::<3>();
        kani::assert(w[0] as u32 + w[1] as u32 + w[2] as u32 == 10_000, "weights sum to 100%");
        kani::assert(nonzero_lamports(MAX_SUPPLY_LAMPORTS) > 0, "nonzero lamports");
        kani::assert(score(100) <= 100, "score domain");
    }

    #[kani::proof]
    fn generators_are_not_vacuous() {
        let p = oracle_price(1_000);
        kani::cover!(p == 1_000, "upper price bound reachable");
        let w = weights_bps::<2>();
        kani::cover!(w[0] == 2_500, "non-trivial weight reachable");
    }
}
