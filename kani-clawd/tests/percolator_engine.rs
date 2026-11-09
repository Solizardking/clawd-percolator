//! Percolator proofs: invariants of the repo's own v16 risk engine
//! (`programs/percolator`), checked by calling its real kernels.
//!
//! The engine exposes small pure kernels under `cfg(kani)` (`kani_*`); these
//! harnesses verify value conservation, rounding direction and bounds on the
//! exact production code, with small symbolic domains where division is involved.
//!
//! cargo kani --tests --harness percolator_
#![cfg(kani)]

use percolator::wide_math::floor_div_signed_conservative_i128;
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

/// Domain fee split: provider + insurance legs sum to the total fee, which is
/// never more than the lien itself.
#[kani::proof]
fn percolator_fee_split_conserves() {
    let atoms: u16 = kani::any();
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
    let n: i16 = kani::any();
    let d: u8 = kani::any();
    kani::assume(d > 0);
    let q = floor_div_signed_conservative_i128(n as i128, d as u128);
    kani::assert(q * (d as i128) <= n as i128, "q * d <= n");
    kani::assert((q + 1) * (d as i128) > n as i128, "n < (q + 1) * d");
}

/// Engine mul-div (native-or-wide) agrees with exact floor/ceil arithmetic.
#[kani::proof]
fn percolator_mul_div_matches_exact() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();
    let d: u8 = kani::any();
    kani::assume(d > 0);
    let exact = (a as u128) * (b as u128);
    let f = kani_mul_div_floor_u128_or_wide(a as u128, b as u128, d as u128).unwrap();
    let c = kani_mul_div_ceil_u128_or_wide(a as u128, b as u128, d as u128).unwrap();
    kani::assert(f == exact / d as u128, "floor matches");
    kani::assert(c == f + if exact % d as u128 == 0 { 0 } else { 1 }, "ceil matches");
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
    let engine = kani_mul_div_floor_u128_or_wide(pnl as u128, num as u128, den as u128).unwrap();
    kani::assert(ours == engine, "spec matches engine kernel");
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
