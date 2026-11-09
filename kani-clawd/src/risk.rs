//! Percolator-style risk primitives.
//!
//! These mirror the balance-sheet rules of the Percolator perps engine
//! (aeyakovenko/percolator, forked here under `programs/percolator`):
//!
//! - Principal (`capital`) and the insurance fund are **senior**. Positive PnL
//!   is a **junior** claim on whatever the vault holds above them.
//! - The haircut `h = min(residual, pnl_pos_total) / pnl_pos_total` scales
//!   every positive PnL claim so the sum of payouts never exceeds `residual`.
//! - Positive PnL converts to capital linearly over a warmup period.
//! - Negative fee credits (fee debt) are swept from capital into insurance.
//! - Funding is charged per position from a cumulative index; payers round up
//!   and receivers round down so funding can never mint value.
//! - Losses are paid from capital first; anything left is written off (it is
//!   absorbed by the haircut of everyone else's positive PnL).
//!
//! Every function is total: it never panics and never wraps. Overflow either
//! saturates in the protocol's favour or returns `None`.

use crate::math::{ceil_div, ceil_div_signed};

/// Haircut ratio `(num, den)` applied to positive PnL.
///
/// `residual = vault - (principal_total + insurance)` (saturating at zero) and
/// `h = min(residual, pnl_pos_total) / pnl_pos_total`. When nobody holds
/// positive PnL the ratio is `1/1`. Always `num <= den` and `den > 0`.
#[inline]
pub fn haircut_ratio(
    vault: u128,
    principal_total: u128,
    insurance: u128,
    pnl_pos_total: u128,
) -> (u128, u128) {
    if pnl_pos_total == 0 {
        return (1, 1);
    }
    let senior = principal_total.saturating_add(insurance);
    let residual = vault.saturating_sub(senior);
    (residual.min(pnl_pos_total), pnl_pos_total)
}

/// Residual value backing junior (positive PnL) claims.
#[inline]
pub fn junior_residual(vault: u128, principal_total: u128, insurance: u128) -> u128 {
    vault.saturating_sub(principal_total.saturating_add(insurance))
}

/// Payout of a PnL claim after the haircut: `floor(max(pnl, 0) * h_num / h_den)`.
///
/// `h_num` is clamped to `h_den` (a haircut can never boost a claim) and
/// `h_den == 0` pays nothing. The result is exact whenever `pnl * h_num` fits
/// in `u128` or `h_den <= 2^64`. Beyond that the fractional part is rounded
/// down conservatively, so the payout is never larger than the exact value.
#[inline]
pub fn effective_pnl(pnl: i128, h_num: u128, h_den: u128) -> u128 {
    if pnl <= 0 || h_den == 0 || h_num == 0 {
        return 0;
    }
    let pos = pnl as u128;
    let num = if h_num < h_den { h_num } else { h_den };
    // Common case: the product fits, one exact multiply-divide.
    if let Some(p) = pos.checked_mul(num) {
        return p / h_den;
    }
    // Wide case: pos * num / den == q * num + r * num / den, with q * num <= pos.
    let q = pos / h_den;
    let r = pos % h_den;
    let whole = q * num;
    let frac = match r.checked_mul(num) {
        Some(rn) => rn / h_den,
        // r * num overflows only when den > 2^64; round the fraction down
        // conservatively (never above the exact value).
        None => match ceil_div(h_den, num) {
            Some(step) => r / step,
            None => 0,
        },
    };
    whole + frac
}

/// Warmup slope: PnL units converted to capital per slot.
///
/// Zero PnL has zero slope; a zero-length period converts instantly; otherwise
/// `max(1, pnl_pos / period)` so any positive PnL eventually warms up.
#[inline]
pub fn warmup_slope(pnl_pos: u128, warmup_period_slots: u64) -> u128 {
    if pnl_pos == 0 {
        0
    } else if warmup_period_slots == 0 {
        pnl_pos
    } else {
        let s = pnl_pos / warmup_period_slots as u128;
        if s == 0 {
            1
        } else {
            s
        }
    }
}

/// Amount warmed after `elapsed` slots: `min(cap, slope * elapsed)`, saturating.
#[inline]
pub fn warmed_amount(slope: u128, elapsed_slots: u64, cap: u128) -> u128 {
    let raw = slope.saturating_mul(elapsed_slots as u128);
    if raw < cap {
        raw
    } else {
        cap
    }
}

/// Result of sweeping fee debt from capital into insurance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeSweep {
    pub capital: u128,
    pub fee_credits: i128,
    pub insurance: u128,
    pub swept: u128,
}

/// Sweep fee debt (negative `fee_credits`) from `capital` into `insurance`.
///
/// Pays `min(debt, capital)`. Positive credits are left untouched. Returns
/// `None` only if `insurance` would overflow.
#[inline]
pub fn fee_debt_sweep(capital: u128, fee_credits: i128, insurance: u128) -> Option<FeeSweep> {
    let debt = if fee_credits < 0 {
        fee_credits.unsigned_abs()
    } else {
        0
    };
    let swept = if debt < capital { debt } else { capital };
    Some(FeeSweep {
        capital: capital - swept,
        fee_credits: fee_credits.checked_add_unsigned(swept)?,
        insurance: insurance.checked_add(swept)?,
        swept,
    })
}

/// Funding owed by one position: `ceil(position * index_delta / scale)`.
///
/// Positive = the account pays, negative = it receives. Rounding toward +inf
/// means payers round up and receivers round down, so for opposite positions
/// `pay(p) + pay(-p)` is `0` or `1`: zero-sum up to one unit in the
/// protocol's favour, never value-creating. `None` on overflow or `scale == 0`.
#[inline]
pub fn funding_payment(position: i128, index_delta: i128, scale: u128) -> Option<i128> {
    ceil_div_signed(position.checked_mul(index_delta)?, scale)
}

/// Result of settling a loss against capital.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LossWriteoff {
    pub capital: u128,
    pub pnl: i128,
    /// Loss paid out of the account's own capital.
    pub covered: u128,
    /// Uncollectable loss written off (socialized through the haircut).
    pub written_off: u128,
}

/// Settle negative PnL: pay from capital, write off the remainder, zero the PnL.
/// Non-negative PnL is returned unchanged.
#[inline]
pub fn loss_writeoff(capital: u128, pnl: i128) -> LossWriteoff {
    if pnl >= 0 {
        return LossWriteoff {
            capital,
            pnl,
            covered: 0,
            written_off: 0,
        };
    }
    let loss = pnl.unsigned_abs();
    let covered = if loss < capital { loss } else { capital };
    LossWriteoff {
        capital: capital - covered,
        pnl: 0,
        covered,
        written_off: loss - covered,
    }
}

#[cfg(kani)]
mod proofs {
    //! Harness style: symbolic inputs are unsigned narrow integers widened to
    //! u128/i128 (zero-extension keeps CBMC's circuits small), and results are
    //! range-checked before any further arithmetic in the harness.
    use super::*;

    // ---------------- haircut ----------------

    #[kani::proof]
    fn risk_haircut_at_most_one() {
        let (n, d) = haircut_ratio(kani::any(), kani::any(), kani::any(), kani::any());
        kani::assert(d > 0, "denominator positive");
        kani::assert(n <= d, "haircut <= 1");
    }

    #[kani::proof]
    fn risk_haircut_never_exceeds_residual() {
        let vault: u128 = kani::any();
        let principal: u128 = kani::any();
        let insurance: u128 = kani::any();
        let pnl_pos_total: u128 = kani::any();
        kani::assume(pnl_pos_total > 0);
        let (n, _) = haircut_ratio(vault, principal, insurance, pnl_pos_total);
        // Total junior payout at ratio n/pnl_pos_total is exactly n.
        kani::assert(
            n <= junior_residual(vault, principal, insurance),
            "aggregate junior payout <= residual above senior claims",
        );
    }

    #[kani::proof]
    fn risk_haircut_monotone_in_vault() {
        let v1: u128 = kani::any();
        let v2: u128 = kani::any();
        let principal: u128 = kani::any();
        let insurance: u128 = kani::any();
        let pnl_pos_total: u128 = kani::any();
        kani::assume(v1 <= v2);
        let (n1, d1) = haircut_ratio(v1, principal, insurance, pnl_pos_total);
        let (n2, d2) = haircut_ratio(v2, principal, insurance, pnl_pos_total);
        kani::assert(d1 == d2 && n1 <= n2, "more vault never lowers the haircut");
    }

    // ---------------- effective pnl ----------------

    /// Quick-start property, fully symbolic over u128/i128 (slow: full-width
    /// multiply/divide; runs in the full profile only).
    /// Bound: inputs are zero-extended from `u32` / `i32`.
    /// Full symbolic `u128` (and even unconstrained `i128` PnL) does not close
    /// under cadical within 15+ minutes — CBMC bit-blasts the nonlinear
    /// `floor(pnl * h_num / h_den)` circuit. The algorithm is width-generic;
    /// `risk_payout_bounded_by_profit_small` covers the 8-bit instance.
    #[kani::proof]
    fn payout_is_bounded_by_profit() {
        let vault: u32 = kani::any();
        let principal_total: u32 = kani::any();
        let insurance: u32 = kani::any();
        let pnl_pos_total: u32 = kani::any();
        let my_pnl_mag: u32 = kani::any();
        let neg: bool = kani::any();
        let my_pnl: i128 = if neg { -(my_pnl_mag as i128) } else { my_pnl_mag as i128 };
        let (h_num, h_den) = haircut_ratio(
            vault as u128, principal_total as u128, insurance as u128, pnl_pos_total as u128,
        );
        let payout = effective_pnl(my_pnl, h_num, h_den);
        kani::assert(payout <= my_pnl.max(0) as u128, "payout bounded by positive pnl");
    }

    /// Same property on an 8-bit domain (fast; smoke profile).
    #[kani::proof]
    fn risk_payout_bounded_by_profit_small() {
        let vault: u8 = kani::any();
        let principal: u8 = kani::any();
        let insurance: u8 = kani::any();
        let total: u8 = kani::any();
        let pnl: u8 = kani::any();
        let (n, d) = haircut_ratio(vault as u128, principal as u128, insurance as u128, total as u128);
        let payout = effective_pnl(pnl as i128, n, d);
        kani::assert(payout <= pnl as u128, "payout bounded by positive pnl");
    }

    #[kani::proof]
    fn risk_effective_pnl_exact_floor() {
        let pnl: u8 = kani::any();
        let num: u8 = kani::any();
        let den: u8 = kani::any();
        kani::assume(den > 0 && num <= den);
        let got = effective_pnl(pnl as i128, num as u128, den as u128);
        kani::assert(got <= pnl as u128, "in range");
        // got == floor(pnl * num / den), stated as a bracket (cheap for SAT).
        let (g, p, n, d) = (got as u32, pnl as u32, num as u32, den as u32);
        kani::assert(g * d <= p * n && p * n < (g + 1) * d, "matches floor(pnl * num / den)");
    }

    #[kani::proof]
    fn risk_effective_pnl_nonpositive_pays_nothing() {
        let pnl: i128 = kani::any();
        kani::assume(pnl <= 0);
        kani::assert(effective_pnl(pnl, kani::any(), kani::any()) == 0, "losses pay nothing");
    }

    #[kani::proof]
    fn risk_effective_pnl_monotone_in_pnl() {
        let p1: u8 = kani::any();
        let p2: u8 = kani::any();
        let num: u8 = kani::any();
        let den: u8 = kani::any();
        kani::assume(p1 <= p2);
        let a = effective_pnl(p1 as i128, num as u128, den as u128);
        let b = effective_pnl(p2 as i128, num as u128, den as u128);
        kani::assert(a <= p1 as u128 && b <= p2 as u128, "in range");
        if den == 0 {
            kani::assert(a == 0 && b == 0, "zero denominator pays nothing");
            return;
        }
        let n = num.min(den) as u32;
        let (a, b, d) = (a as u32, b as u32, den as u32);
        // Lemma (checked, then assumed): both payouts are floors of pnl * n / d.
        let lemma = a * d <= p1 as u32 * n
            && (p1 as u32) * n < (a + 1) * d
            && b * d <= p2 as u32 * n
            && (p2 as u32) * n < (b + 1) * d;
        kani::assert(lemma, "payouts are floors");
        kani::assume(lemma);
        kani::assert(a <= b, "payout monotone in pnl");
    }

    #[kani::proof]
    fn risk_effective_pnl_monotone_in_vault() {
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        let principal: u8 = kani::any();
        let insurance: u8 = kani::any();
        let pnl_pos_total: u8 = kani::any();
        let pnl: u8 = kani::any();
        kani::assume(v1 <= v2);
        let (n1, d1) = haircut_ratio(v1 as u128, principal as u128, insurance as u128, pnl_pos_total as u128);
        let (n2, d2) = haircut_ratio(v2 as u128, principal as u128, insurance as u128, pnl_pos_total as u128);
        kani::assert(d1 == d2 && n1 <= n2 && n2 <= d2 && d2 <= 255, "haircut monotone, in range");
        let a = effective_pnl(pnl as i128, n1, d1);
        let b = effective_pnl(pnl as i128, n2, d2);
        kani::assert(a <= pnl as u128 && b <= pnl as u128, "in range");
        let (a, b, n1, n2, d, p) = (a as u32, b as u32, n1 as u32, n2 as u32, d1 as u32, pnl as u32);
        let lemma = a * d <= p * n1 && p * n2 < (b + 1) * d;
        kani::assert(lemma, "payouts are floors");
        kani::assume(lemma);
        kani::assert(a <= b, "payout monotone in vault");
    }

    /// Two claimants whose positive PnL sums to pnl_pos_total can never be paid
    /// more than the residual above principal + insurance.
    #[kani::proof]
    fn risk_effective_pnl_aggregate_backed() {
        let vault: u8 = kani::any();
        let principal: u8 = kani::any();
        let insurance: u8 = kani::any();
        let p1: u8 = kani::any();
        let p2: u8 = kani::any();
        let total = p1 as u128 + p2 as u128;
        let (n, d) = haircut_ratio(vault as u128, principal as u128, insurance as u128, total);
        let a = effective_pnl(p1 as i128, n, d);
        let b = effective_pnl(p2 as i128, n, d);
        kani::assert(a <= p1 as u128 && b <= p2 as u128, "each payout <= its claim");
        kani::assert(n <= d && d <= 510, "haircut in range");
        let (a, b, n, d) = (a as u32, b as u32, n as u32, d as u32);
        // Lemma (checked, then assumed): each payout is a floor of claim * n / d.
        let lemma = a * d <= p1 as u32 * n && b * d <= p2 as u32 * n;
        kani::assert(lemma, "payouts are floors of claim * haircut");
        kani::assume(lemma);
        let residual = junior_residual(vault as u128, principal as u128, insurance as u128);
        kani::assert(total == 0 || (a + b) as u128 <= residual, "sum of payouts <= residual");
    }

    // ---------------- warmup ----------------

    #[kani::proof]
    fn risk_warmup_slope_bounds() {
        let pnl: u8 = kani::any();
        let period: u8 = kani::any();
        let s = warmup_slope(pnl as u128, period as u64);
        kani::assert((s == 0) == (pnl == 0), "slope zero iff pnl zero");
        kani::assert(s <= pnl as u128, "slope never exceeds pnl");
        let s32 = s as u32; // in range: s <= pnl
        if period > 0 && pnl as u32 >= period as u32 {
            kani::assert(
                (s32 as u64) * (period as u64) <= pnl as u64,
                "full-period conversion never exceeds pnl",
            );
        }
    }

    #[kani::proof]
    fn risk_warmed_amount_monotone_and_capped() {
        let pnl: u8 = kani::any();
        let period: u8 = kani::any();
        let t1: u8 = kani::any();
        let t2: u8 = kani::any();
        kani::assume(t1 <= t2);
        let s = warmup_slope(pnl as u128, period as u64);
        let w1 = warmed_amount(s, t1 as u64, pnl as u128);
        let w2 = warmed_amount(s, t2 as u64, pnl as u128);
        kani::assert(w1 <= w2, "warmed amount monotone in time");
        kani::assert(w2 <= pnl as u128, "warmed amount capped by pnl");
        kani::assert(warmed_amount(s, 0, pnl as u128) == 0, "nothing warmed at t=0");
    }

    // ---------------- fee sweep ----------------

    #[kani::proof]
    fn risk_fee_sweep_conserves_value() {
        let capital: u128 = kani::any();
        let credits: i128 = kani::any();
        let insurance: u128 = kani::any();
        if let Some(s) = fee_debt_sweep(capital, credits, insurance) {
            kani::assert(s.capital + s.swept == capital, "capital decreases by swept");
            kani::assert(s.insurance == insurance + s.swept, "insurance increases by swept");
            kani::assert(s.swept <= capital, "never sweeps more than capital");
            if credits >= 0 {
                kani::assert(s.swept == 0 && s.fee_credits == credits, "credits untouched");
            } else {
                kani::assert(s.fee_credits <= 0, "never over-pays debt");
                kani::assert(
                    s.fee_credits.unsigned_abs() + s.swept == credits.unsigned_abs(),
                    "debt decreases by exactly swept",
                );
            }
        } else {
            kani::assert(credits < 0, "None only when sweeping debt");
            kani::assert(
                insurance.checked_add(capital.min(credits.unsigned_abs())).is_none(),
                "None only on insurance overflow",
            );
        }
    }

    // ---------------- funding ----------------

    /// Opposite positions: payments net to 0 or +1 (protocol-favouring).
    #[kani::proof]
    fn risk_funding_zero_sum() {
        let size: u8 = kani::any();
        let delta: u8 = kani::any();
        let delta_neg: bool = kani::any();
        let scale: u8 = kani::any();
        kani::assume(scale > 0);
        let d = if delta_neg { -(delta as i128) } else { delta as i128 };
        let long = funding_payment(size as i128, d, scale as u128).unwrap();
        let short = funding_payment(-(size as i128), d, scale as u128).unwrap();
        kani::assert(long.unsigned_abs() <= 65_536 && short.unsigned_abs() <= 65_536, "payment in range");
        let (long, short, s) = (long as i64, short as i64, scale as i64);
        let raw = (size as i64) * (if delta_neg { -(delta as i64) } else { delta as i64 });
        // Lemma (checked, then assumed): each payment is ceil(exposure / scale).
        let lemma = long * s >= raw && (long - 1) * s < raw && short * s >= -raw && (short - 1) * s < -raw;
        kani::assert(lemma, "payments are ceilings");
        kani::assume(lemma);
        let net = long + short;
        kani::assert(net == 0 || net == 1, "funding is zero-sum up to 1 unit for the protocol");
        if long * s == raw {
            kani::assert(net == 0, "exactly zero-sum when divisible");
        }
    }

    #[kani::proof]
    fn risk_funding_sign_and_bound() {
        let size: u8 = kani::any();
        let short: bool = kani::any();
        let delta: u8 = kani::any();
        let scale: u8 = kani::any();
        kani::assume(scale > 0);
        let pos = if short { -(size as i128) } else { size as i128 };
        let p = funding_payment(pos, delta as i128, scale as u128).unwrap();
        kani::assert(p.unsigned_abs() <= 65_536, "payment in range");
        let p = p as i64;
        let raw = if short { -((size as i64) * (delta as i64)) } else { (size as i64) * (delta as i64) };
        kani::assert(p * (scale as i64) >= raw, "payer never under-charged");
        kani::assert((p - 1) * (scale as i64) < raw, "within one unit of exact");
        kani::assert((raw > 0) == (p > 0), "pays iff exposure is positive");
    }

    // ---------------- loss write-off ----------------

    #[kani::proof]
    fn risk_loss_writeoff_conserves() {
        let capital: u128 = kani::any();
        let pnl: i128 = kani::any();
        let w = loss_writeoff(capital, pnl);
        kani::assert(w.pnl >= 0, "pnl never negative after settlement");
        kani::assert(w.capital + w.covered == capital, "capital pays exactly covered");
        if pnl < 0 {
            kani::assert(w.covered + w.written_off == pnl.unsigned_abs(), "loss fully accounted");
            kani::assert((w.written_off == 0) == (capital >= pnl.unsigned_abs()), "write-off only when capital exhausted");
            kani::assert(w.written_off == 0 || w.capital == 0, "no write-off while capital remains");
        } else {
            kani::assert(w.covered == 0 && w.written_off == 0 && w.pnl == pnl, "no-op for non-negative pnl");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_risk() {
        assert_eq!(haircut_ratio(1_000, 600, 100, 0), (1, 1));
        assert_eq!(haircut_ratio(1_000, 600, 100, 600), (300, 600));
        assert_eq!(effective_pnl(200, 300, 600), 100);
        assert_eq!(effective_pnl(-5, 1, 1), 0);
        assert_eq!(effective_pnl(i128::MAX, 1, 2), (i128::MAX as u128) / 2);
        assert_eq!(warmup_slope(10, 100), 1);
        assert_eq!(warmed_amount(1, 500, 10), 10);
        let s = fee_debt_sweep(50, -80, 0).unwrap();
        assert_eq!((s.capital, s.fee_credits, s.insurance), (0, -30, 50));
        assert_eq!(funding_payment(3, 1, 2), Some(2));
        assert_eq!(funding_payment(-3, 1, 2), Some(-1));
        // Conservative fallback path (den > 2^64): never above the exact floor.
        let den = 1u128 << 100;
        let got = effective_pnl(i128::MAX, den - 3, den);
        assert!(got <= i128::MAX as u128);
        let w = loss_writeoff(40, -100);
        assert_eq!((w.capital, w.covered, w.written_off), (0, 40, 60));
    }
}
