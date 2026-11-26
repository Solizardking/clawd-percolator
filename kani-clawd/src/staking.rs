//! Staking helpers: monotone, bounded reward multipliers.


/// Assert a lock-duration multiplier never decreases as the lock grows.
#[track_caller]
pub fn assert_multiplier_monotone(d1: u64, d2: u64, multiplier: impl Fn(u64) -> u128) {
    if d1 <= d2 {
        crate::check!(multiplier(d1) <= multiplier(d2), "staking multiplier decreases with lock duration");
    }
}

/// Assert a multiplier stays within `[min, max]` (in the caller's fixed-point scale).
#[track_caller]
pub fn assert_multiplier_bounded(duration: u64, min: u128, max: u128, multiplier: impl Fn(u64) -> u128) {
    let m = multiplier(duration);
    crate::check!(m >= min && m <= max, "staking multiplier out of bounds");
}

/// Assert stake-weighted rewards are monotone in stake for a fixed multiplier.
#[track_caller]
pub fn assert_reward_monotone_in_stake(
    s1: u64,
    s2: u64,
    duration: u64,
    reward: impl Fn(u64, u64) -> u128,
) {
    if s1 <= s2 {
        crate::check!(reward(s1, duration) <= reward(s2, duration), "reward decreases with stake");
    }
}

/// Reference linear multiplier in bps: 1.0x (10_000) at zero lock, rising to
/// `max_bps` at `max_lock` slots, flat afterwards.
#[inline]
pub fn linear_multiplier_bps(duration: u64, max_lock: u64, max_bps: u16) -> u128 {
    let top: u32 = if max_bps as u32 > 10_000 { max_bps as u32 } else { 10_000 };
    if max_lock == 0 || duration >= max_lock {
        return top as u128;
    }
    // span < 2^16 and duration < 2^64, so the product fits in u128.
    let span = (top - 10_000) as u128;
    10_000 + span * duration as u128 / max_lock as u128
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn staking_linear_multiplier_monotone() {
        let max_lock: u8 = kani::any();
        let max_bps: u16 = kani::any();
        let d1: u8 = kani::any();
        let d2: u8 = kani::any();
        kani::assume(d1 <= d2);
        let m = |d: u8| linear_multiplier_bps(d as u64, max_lock as u64, max_bps);
        let (m1, m2) = (m(d1), m(d2));
        let top = (max_bps as u32).max(10_000);
        crate::check!(m1 >= 10_000 && m1 <= top as u128 && m2 >= 10_000 && m2 <= top as u128, "in range");
        if max_lock > d2 {
            // Both below the cap: floors of span * d / max_lock (lemma, checked then assumed).
            let (e1, e2, span, ml) = (m1 as u32 - 10_000, m2 as u32 - 10_000, top - 10_000, max_lock as u32);
            let lemma = e1 * ml <= span * (d1 as u32) && span * (d2 as u32) < (e2 + 1) * ml;
            crate::check!(lemma, "multipliers are floors");
            kani::assume(lemma);
        }
        assert_multiplier_monotone(d1 as u64, d2 as u64, |d| if d == d1 as u64 { m1 } else { m2 });
    }

    #[kani::proof]
    fn staking_linear_multiplier_bounded() {
        let max_lock: u8 = kani::any();
        let max_bps: u16 = kani::any();
        kani::assume(max_bps >= 10_000);
        assert_multiplier_bounded(kani::any::<u16>() as u64, 10_000, max_bps as u128, |d| {
            linear_multiplier_bps(d, max_lock as u64, max_bps)
        });
    }

    #[kani::proof]
    fn staking_reward_monotone_in_stake() {
        let max_lock: u8 = kani::any();
        assert_reward_monotone_in_stake(
            kani::any::<u8>() as u64,
            kani::any::<u8>() as u64,
            kani::any::<u8>() as u64,
            |s, d| {
                let m = linear_multiplier_bps(d, max_lock as u64, 20_000);
                crate::check!(m <= 20_000, "multiplier in range");
                (s as u64 * m as u16 as u64 / 10_000) as u128
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concrete_staking() {
        assert_eq!(linear_multiplier_bps(0, 100, 20_000), 10_000);
        assert_eq!(linear_multiplier_bps(50, 100, 20_000), 15_000);
        assert_eq!(linear_multiplier_bps(500, 100, 20_000), 20_000);
    }
}
