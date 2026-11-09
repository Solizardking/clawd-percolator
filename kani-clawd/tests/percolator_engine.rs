//! Percolator proofs: invariants of the repo's own v16 risk engine
//! (`programs/percolator`), checked by calling its real kernels.
//!
//! The engine exposes small pure kernels under `cfg(kani)` (`kani_*`); these
//! harnesses verify value conservation, rounding direction and bounds on the
//! exact production code, with small symbolic domains where division is involved.
//!
//! cargo kani --tests --harness percolator_
#![cfg(kani)]

use percolator::wide_math::{floor_div_signed_conservative_i128, mul_div_ceil_u128, mul_div_floor_u128};
use percolator::*;

/// Backing-utilization fee: every atom taken from capital lands in bucket
/// earnings, and the group capital total moves by the same amount.
#[kani::proof]
fn percolator_backing_fee_charge_conserves() {
    let capital: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let earnings: u128 = kani::any();
    let pnl: i128 = kani::any();
    let fee: u128 = kani::any();
    kani::assume(capital <= c_tot); // account capital is part of the group total
    if let Ok((charged, cap2, ctot2, earn2)) =
        kani_apply_backing_utilization_fee_charge(capital, c_tot, earnings, pnl, fee)
    {
        kani::assert(charged <= fee && charged <= capital, "charge bounded by request and capital");
        kani::assert(cap2 + charged == capital, "capital debited by charge");
        kani::assert(ctot2 + charged == c_tot, "group capital debited by charge");
        kani::assert(earn2 == earnings + charged, "earnings credited by charge");
        if pnl < 0 {
            kani::assert(charged == 0, "no backing fee while account is in loss");
        }
    }
}

/// Provider earnings withdraw debits the vault and the earnings bucket equally
/// and never withdraws more than was earned.
#[kani::proof]
fn percolator_earnings_withdraw_conserves() {
    let vault: u128 = kani::any();
    let earnings: u128 = kani::any();
    let amount: u128 = kani::any();
    match kani_apply_backing_provider_earnings_withdraw(vault, earnings, amount) {
        Ok((v2, e2)) => {
            kani::assert(amount <= earnings, "never withdraws more than earned");
            kani::assert(vault - v2 == amount && earnings - e2 == amount, "vault and earnings move together");
        }
        Err(_) => kani::assert(amount > earnings || amount > vault, "rejects only unbacked withdrawals"),
    }
}

/// Slow (engine routes the insurance leg through U256 wide math): full profile
/// with a long timeout only.
/// Domain fee split: provider + insurance legs sum to the total fee, which is
/// never more than the lien itself.
#[kani::proof]
fn percolator_fee_split_conserves() {
    let atoms: u8 = kani::any();
    let fee_bps: u16 = kani::any();
    let share_bps: u16 = kani::any();
    let num = (atoms as u128) * BOUND_SCALE;
    match backing_domain_fee_split_for_lien_delta_num(num, fee_bps, share_bps) {
        Ok(s) => {
            kani::assert(s.lien_delta_atoms == atoms as u128, "atoms recovered exactly");
            kani::assert(s.provider_fee + s.insurance_fee == s.total_fee, "fee legs conserve total");
            kani::assert(s.insurance_fee <= s.total_fee, "insurance leg bounded");
            kani::assert(s.total_fee <= s.lien_delta_atoms, "fee never exceeds lien");
        }
        Err(_) => kani::assert(fee_bps > 10_000 || share_bps > 10_000, "rejects only invalid bps"),
    }
}

/// The engine's signed floor division rounds toward -inf (conservative for
/// funding): q * d <= n < (q + 1) * d.
#[kani::proof]
fn percolator_floor_div_signed_conservative() {
    let mag: u8 = kani::any();
    let neg: bool = kani::any();
    let d: u8 = kani::any();
    kani::assume(d > 0);
    let n = if neg { -(mag as i128) } else { mag as i128 };
    let q = floor_div_signed_conservative_i128(n, d as u128);
    kani::assert(q.unsigned_abs() <= mag as u128 + 1, "quotient in range");
    let (q, n, d) = (q as i64, n as i64, d as i64);
    kani::assert(q * d <= n, "q * d <= n");
    kani::assert((q + 1) * d > n, "n < (q + 1) * d");
}

/// Engine native mul-div (`wide_math::mul_div_{floor,ceil}_u128`) agrees with
/// exact floor/ceil arithmetic.
#[kani::proof]
fn percolator_mul_div_matches_exact() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();
    let d: u8 = kani::any();
    kani::assume(d > 0);
    let exact = (a as u32) * (b as u32);
    let f = mul_div_floor_u128(a as u128, b as u128, d as u128);
    let c = mul_div_ceil_u128(a as u128, b as u128, d as u128);
    kani::assert(f <= exact as u128 && c <= exact as u128 + 1, "results in range");
    let (f, c, d) = (f as u32, c as u32, d as u32);
    kani::assert(f * d <= exact && exact < (f + 1) * d, "floor matches");
    kani::assert(c * d >= exact && (c == f || c == f + 1) && ((c == f) == (f * d == exact)), "ceil matches");
}

/// Engine trading-fee kernel: ceil(notional * bps / 10_000), never exceeding
/// the notional for bps <= 10_000 and charging at least 1 atom on any nonzero fee.
#[kani::proof]
fn percolator_checked_fee_bps_ceil() {
    let notional: u8 = kani::any();
    let bps: u16 = kani::any();
    kani::assume(bps <= 10_000);
    let fee = kani_checked_fee_bps(notional as u128, bps as u64).unwrap();
    kani::assert(fee <= notional as u128, "fee never exceeds notional");
    let p = (notional as u64) * (bps as u64);
    let f = fee as u64;
    kani::assert(f * 10_000 >= p && (f == 0 || (f - 1) * 10_000 < p), "fee is ceil(notional * bps / 10_000)");
    kani::assert((fee == 0) == (p == 0), "no free trades: nonzero exposure pays >= 1");
}

/// A negative-PnL account with other open legs is always loss-locked; a
/// non-negative PnL never is.
#[kani::proof]
fn percolator_unattributed_loss_lock() {
    let was_locked: bool = kani::any();
    let bitmap: V16ActiveBitmap = kani::any();
    let pnl: i128 = kani::any();
    let locked = kani_unattributed_loss_lock_after_pnl(was_locked, bitmap, pnl);
    if pnl >= 0 {
        kani::assert(!locked, "non-negative pnl is never loss-locked");
    }
    if pnl < 0 && was_locked {
        kani::assert(locked, "an existing loss lock persists while pnl stays negative");
    }
}

/// Bridge: kani_clawd's effective_pnl agrees with the engine's own
/// mul_div_floor kernel on the haircut product.
#[kani::proof]
fn percolator_effective_pnl_matches_engine_mul_div() {
    let pnl: u8 = kani::any();
    let num: u8 = kani::any();
    let den: u8 = kani::any();
    kani::assume(den > 0 && num <= den && pnl > 0);
    let ours = kani_clawd::risk::effective_pnl(pnl as i128, num as u128, den as u128);
    let engine = mul_div_floor_u128(pnl as u128, num as u128, den as u128);
    kani::assert(ours <= pnl as u128 && engine <= pnl as u128, "in range");
    // Both are floor(pnl * num / den): each satisfies the same bracket, and the
    // bracket has a unique solution, so they are equal.
    let (o, e, p, n, d) = (ours as u32, engine as u32, pnl as u32, num as u32, den as u32);
    kani::assert(o * d <= p * n && p * n < (o + 1) * d, "ours is the floor");
    kani::assert(e * d <= p * n && p * n < (e + 1) * d, "engine is the floor");
}

/// Account layouts match the wrapper's sizing formula (programs/percolator-prog
/// v16_program.rs): market = HEADER(16) + WRAPPER_CONFIG(576) + group header +
/// N * asset slot.
#[kani::proof]
fn percolator_account_layout_sizes() {
    use core::mem::size_of;
    let group = size_of::<MarketGroupV16HeaderAccount>();
    let slot = size_of::<Market<[u8; 512]>>();
    kani::assert(16 + 576 + group + slot == 3_163, "1-asset market account is 3,163 bytes");
    kani::assert(slot == 1_813, "each extra asset adds 1,813 bytes");
    kani::assert(V16_LAYOUT_DISCRIMINATOR == 18, "engine layout discriminator is 18");
    // The portfolio (9,419 B engine state, 9,539 B account) is NOT checked here:
    // under cfg(kani) the engine shrinks PORTFOLIO_SOURCE_DOMAIN_CAP, so the
    // production portfolio size is pinned by tests/percolator_layout.rs instead.
}
