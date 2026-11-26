//! Output-bound helpers.


/// Assert `f(x)` lies in `[lo, hi]`.
#[track_caller]
pub fn assert_bounded<I: Copy, O: PartialOrd>(x: I, lo: O, hi: O, f: impl Fn(I) -> O) {
    let y = f(x);
    crate::check!(y >= lo && y <= hi, "output out of bounds");
}

/// Assert `f(x) <= cap(x)`: an output never exceeds an input-dependent ceiling
/// (e.g. payout <= deposit, withdrawal <= balance).
#[track_caller]
pub fn assert_never_exceeds<I: Copy, O: PartialOrd>(x: I, f: impl Fn(I) -> O, cap: impl Fn(I) -> O) {
    crate::check!(f(x) <= cap(x), "output exceeds its ceiling");
}

/// Assert a fallible operation only fails when `may_fail(x)` holds (no spurious errors).
#[track_caller]
pub fn assert_total_on<I: Copy, O, E>(x: I, f: impl Fn(I) -> Result<O, E>, may_fail: impl Fn(I) -> bool) {
    if !may_fail(x) {
        crate::check!(f(x).is_ok(), "operation failed on an input it must accept");
    }
}

/// Basis points are in `0..=10_000`.
#[inline]
pub fn is_valid_bps(bps: u64) -> bool {
    bps <= 10_000
}

#[cfg(kani)]
mod proofs {
    use super::*;
    use crate::risk;

    #[kani::proof]
    fn bounds_haircut_in_unit_interval() {
        let v: u128 = kani::any();
        let p: u128 = kani::any();
        let i: u128 = kani::any();
        let t: u128 = kani::any();
        assert_bounded((v, p, i, t), 0u128, 1u128, |(v, p, i, t)| {
            let (n, d) = risk::haircut_ratio(v, p, i, t);
            n / d
        });
    }

    #[kani::proof]
    fn bounds_writeoff_never_exceeds_loss() {
        let c: u128 = kani::any();
        let p: i128 = kani::any();
        assert_never_exceeds(
            (c, p),
            |(c, p)| risk::loss_writeoff(c, p).written_off,
            |(_, p)| if p < 0 { p.unsigned_abs() } else { 0 },
        );
    }

    #[kani::proof]
    fn bounds_bps_split_total() {
        let amount: u8 = kani::any();
        let bps: u16 = kani::any();
        assert_total_on(
            (amount, bps),
            |(a, b)| crate::math::bps_of(a as u128, b).ok_or(()),
            |(_, b)| !is_valid_bps(b as u64),
        );
    }
}
