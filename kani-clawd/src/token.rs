//! Token-flow helpers: value-conserving splits and transfers.


/// Assert that `split(total)` returns parts that sum to exactly `total`.
#[track_caller]
pub fn assert_split_conserves(total: u128, split: impl Fn(u128) -> (u128, u128)) {
    let (a, b) = split(total);
    crate::check!(a.checked_add(b) == Some(total), "split does not conserve total");
}

/// N-way version of [`assert_split_conserves`].
#[track_caller]
pub fn assert_split_conserves_n<const N: usize>(total: u128, split: impl Fn(u128) -> [u128; N]) {
    let parts = split(total);
    let mut sum: Option<u128> = Some(0);
    let mut i = 0;
    while i < N {
        sum = sum.and_then(|s| s.checked_add(parts[i]));
        i += 1;
    }
    crate::check!(sum == Some(total), "n-way split does not conserve total");
}

/// Assert a transfer moves exactly `amount` from `from` to `to`, or is rejected
/// without touching either balance. `transfer` returns `Some((from', to'))` on success.
#[track_caller]
pub fn assert_transfer_conserves(
    from: u64,
    to: u64,
    amount: u64,
    transfer: impl Fn(u64, u64, u64) -> Option<(u64, u64)>,
) {
    match transfer(from, to, amount) {
        Some((f, t)) => {
            crate::check!(
                f as u128 + t as u128 == from as u128 + to as u128,
                "transfer changes total supply",
            );
            crate::check!(f as u128 + amount as u128 == from as u128, "sender not debited by amount");
        }
        None => crate::check!(
            amount > from || (to as u128 + amount as u128) > u64::MAX as u128,
            "valid transfer rejected",
        ),
    }
}

/// Reference fee split used in examples: `fee = floor(amount * bps / 10_000)`
/// with the remainder kept by the user, so `user + fee == amount`.
#[inline]
pub fn split_fee_bps(amount: u128, bps: u16) -> Option<(u128, u128)> {
    let fee = crate::math::bps_of(amount, bps)?;
    Some((amount - fee, fee))
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn token_split_fee_bps_conserves() {
        let amount: u8 = kani::any();
        let bps: u16 = kani::any();
        kani::assume(bps <= 10_000);
        assert_split_conserves(amount as u128, |t| split_fee_bps(t, bps).unwrap());
    }

    #[kani::proof]
    fn token_checked_transfer_conserves() {
        assert_transfer_conserves(kani::any(), kani::any(), kani::any(), |f, t, a| {
            Some((f.checked_sub(a)?, t.checked_add(a)?))
        });
    }

    #[kani::proof]
    fn token_three_way_split_conserves() {
        let total: u8 = kani::any();
        let a_bps: u16 = kani::any();
        let b_bps: u16 = kani::any();
        kani::assume(a_bps as u32 + b_bps as u32 <= 10_000);
        assert_split_conserves_n::<3>(total as u128, |t| {
            let a = crate::math::bps_of(t, a_bps).unwrap();
            let b = crate::math::bps_of(t, b_bps).unwrap();
            [a, b, t - a - b]
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concrete_token() {
        assert_split_conserves(1_001, |t| split_fee_bps(t, 30).unwrap());
        assert_transfer_conserves(10, 5, 3, |f, t, a| Some((f - a, t + a)));
    }
}
