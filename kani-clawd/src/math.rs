//! Checked integer kernels and spec-equivalence helpers.
//!
//! The kernels are deliberately small so CBMC can bit-blast them quickly. The
//! helpers take caller closures, so a protocol can prove its own optimized
//! implementation against a readable reference spec.


/// `floor(a * b / d)`, or `None` on `d == 0` or if `a * b` overflows `u128`.
#[inline]
pub fn mul_div_floor(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    Some(a.checked_mul(b)? / d)
}

/// `ceil(a * b / d)`, or `None` on `d == 0` or overflow.
#[inline]
pub fn mul_div_ceil(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    let p = a.checked_mul(b)?;
    let q = p / d;
    Some(if p % d == 0 { q } else { q + 1 })
}

/// `ceil(n / d)` for `d > 0` without the `n + d - 1` overflow.
#[inline]
pub fn ceil_div(n: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    let q = n / d;
    Some(if n % d == 0 { q } else { q + 1 })
}

/// Signed ceiling division `ceil(n / d)` for `d > 0`. Rounds toward +inf, so a
/// payer (n > 0) rounds up and a receiver (n < 0) rounds toward zero.
#[inline]
pub fn ceil_div_signed(n: i128, d: u128) -> Option<i128> {
    if d == 0 {
        return None;
    }
    let mag = n.unsigned_abs() / d;
    let rem = n.unsigned_abs() % d;
    if n >= 0 {
        let q = if rem == 0 { mag } else { mag.checked_add(1)? };
        i128::try_from(q).ok()
    } else {
        // ceil of a negative quotient truncates toward zero.
        i128::try_from(mag).ok().map(|m| -m)
    }
}

/// Basis-point share `floor(amount * bps / 10_000)`; `bps` must be `<= 10_000`.
#[inline]
pub fn bps_of(amount: u128, bps: u16) -> Option<u128> {
    if bps > 10_000 {
        return None;
    }
    mul_div_floor(amount, bps as u128, 10_000)
}

/// Assert `implementation(x) == spec(x)` for the given (usually symbolic) input.
#[track_caller]
pub fn assert_spec_equivalent<I: Copy, O: PartialEq>(
    input: I,
    implementation: impl Fn(I) -> O,
    spec: impl Fn(I) -> O,
) {
    crate::check!(
        implementation(input) == spec(input),
        "implementation diverges from reference spec",
    );
}

/// Assert `f` is monotone non-decreasing between two ordered inputs.
#[track_caller]
pub fn assert_monotone<I: PartialOrd + Copy, O: PartialOrd>(lo: I, hi: I, f: impl Fn(I) -> O) {
    if lo <= hi {
        crate::check!(f(lo) <= f(hi), "function is not monotone non-decreasing");
    }
}

/// Assert `f` is monotone non-increasing between two ordered inputs.
#[track_caller]
pub fn assert_antitone<I: PartialOrd + Copy, O: PartialOrd>(lo: I, hi: I, f: impl Fn(I) -> O) {
    if lo <= hi {
        crate::check!(f(lo) >= f(hi), "function is not monotone non-increasing");
    }
}

/// Assert a checked operation returns `None` exactly when `overflows(x)` says it must.
#[track_caller]
pub fn assert_checked_matches<I: Copy, O>(
    input: I,
    op: impl Fn(I) -> Option<O>,
    overflows: impl Fn(I) -> bool,
) {
    crate::check!(
        op(input).is_none() == overflows(input),
        "checked operation's None does not match the overflow predicate",
    );
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn math_mul_div_floor_le_ceil() {
        let a: u16 = kani::any();
        let b: u16 = kani::any();
        let d: u16 = kani::any();
        kani::assume(d > 0);
        let f = mul_div_floor(a as u128, b as u128, d as u128).unwrap();
        let c = mul_div_ceil(a as u128, b as u128, d as u128).unwrap();
        let exact = (a as u32) * (b as u32);
        kani::assert(f == (exact / d as u32) as u128, "floor is exact");
        kani::assert(c == f + (exact % d as u32 != 0) as u128, "ceil = floor + (rem != 0)");
    }

    #[kani::proof]
    fn math_ceil_div_signed_spec() {
        let n: u16 = kani::any();
        let neg: bool = kani::any();
        let d: u16 = kani::any();
        kani::assume(d > 0);
        let n = if neg { -(n as i32) } else { n as i32 };
        let q = ceil_div_signed(n as i128, d as u128).unwrap();
        kani::assert(q.unsigned_abs() <= u16::MAX as u128, "quotient in range");
        let q = q as i64;
        // q is the least integer with q * d >= n.
        kani::assert(q * (d as i64) >= n as i64, "q * d >= n");
        kani::assert((q - 1) * (d as i64) < n as i64, "(q - 1) * d < n");
    }

    #[kani::proof]
    fn math_ceil_div_signed_odd_symmetry() {
        let n: u16 = kani::any();
        let d: u16 = kani::any();
        kani::assume(d > 0);
        let pos = ceil_div_signed(n as i128, d as u128).unwrap();
        let neg = ceil_div_signed(-(n as i128), d as u128).unwrap();
        kani::assert(pos >= 0 && pos <= n as i128 && neg <= 0 && -neg <= n as i128, "in range");
        let sum = pos as i32 + neg as i32;
        kani::assert(sum == 0 || sum == 1, "ceil(x) + ceil(-x) in {0, 1}");
    }

    #[kani::proof]
    fn math_bps_of_bounded() {
        let amount: u32 = kani::any();
        let bps: u16 = kani::any();
        match bps_of(amount as u128, bps) {
            Some(v) => kani::assert(bps <= 10_000 && v <= amount as u128, "share <= amount"),
            None => kani::assert(bps > 10_000, "only invalid bps is rejected"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_kernels() {
        assert_eq!(mul_div_floor(10, 3, 4), Some(7));
        assert_eq!(mul_div_ceil(10, 3, 4), Some(8));
        assert_eq!(ceil_div_signed(-7, 2), Some(-3));
        assert_eq!(ceil_div_signed(7, 2), Some(4));
        assert_eq!(mul_div_floor(u128::MAX, 2, 1), None);
        assert_spec_equivalent(9u128, |x| x * 2, |x| x + x);
        assert_monotone(1u128, 5, |x| x * x);
    }
}
