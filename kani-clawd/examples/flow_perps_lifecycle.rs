//! End-to-end flow 1: perps deposit -> trade -> funding -> settle -> withdraw.
//!
//! A two-account (user, LP) Percolator market built only from
//! `kani_clawd::risk` primitives. After every step the senior claims
//! (all capital + insurance) must be fully backed by the vault, and the user
//! can never withdraw more than the vault holds.
//!
//! cargo kani --tests --harness flow_perps_lifecycle   # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::math::bps_of;
use kani_clawd::risk::*;

struct Acct {
    capital: u128,
    pnl: i128,
    fee_credits: i128,
}

struct Market {
    vault: u128,
    insurance: u128,
    user: Acct,
    lp: Acct,
}

impl Market {
    fn senior(&self) -> u128 {
        self.user.capital + self.lp.capital + self.insurance
    }
    fn pnl_pos_total(&self) -> u128 {
        self.user.pnl.max(0) as u128 + self.lp.pnl.max(0) as u128
    }
    fn backed(&self) -> bool {
        self.senior() <= self.vault
    }
}

fn settle(m: &mut Market, is_user: bool, elapsed: u64, period: u64) {
    // 1) sweep fee debt into insurance
    let a = if is_user { &m.user } else { &m.lp };
    let s = fee_debt_sweep(a.capital, a.fee_credits, m.insurance).unwrap();
    m.insurance = s.insurance;
    let a = if is_user { &mut m.user } else { &mut m.lp };
    a.capital = s.capital;
    a.fee_credits = s.fee_credits;
    // 2) losses are paid from capital, remainder written off
    let w = loss_writeoff(a.capital, a.pnl);
    a.capital = w.capital;
    a.pnl = w.pnl;
    // 3) positive pnl converts through the haircut, limited by warmup
    let pnl = if is_user { m.user.pnl } else { m.lp.pnl };
    if pnl > 0 {
        let (n, d) = haircut_ratio(m.vault, m.user.capital + m.lp.capital, m.insurance, m.pnl_pos_total());
        let slope = warmup_slope(pnl as u128, period);
        let warmed = warmed_amount(slope, elapsed, pnl as u128);
        let payout = effective_pnl(warmed as i128, n, d);
        let a = if is_user { &mut m.user } else { &mut m.lp };
        a.capital += payout;
        a.pnl -= warmed as i128;
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn flow_perps_lifecycle() {
    // deposit
    let du: u8 = kani::any();
    let dl: u8 = kani::any();
    let mut m = Market {
        vault: du as u128 + dl as u128,
        insurance: 0,
        user: Acct { capital: du as u128, pnl: 0, fee_credits: 0 },
        lp: Acct { capital: dl as u128, pnl: 0, fee_credits: 0 },
    };
    kani::assert(m.backed(), "deposit: senior claims backed");

    // trade: zero-sum mark pnl between user and LP, taker fee charged as fee debt
    let pnl_mag: u8 = kani::any();
    let user_wins: bool = kani::any();
    let pnl: i128 = if user_wins { pnl_mag as i128 } else { -(pnl_mag as i128) };
    let notional: u8 = kani::any();
    let fee_bps: u16 = kani::any();
    kani::assume(fee_bps <= 100);
    m.user.pnl += pnl;
    m.lp.pnl -= pnl;
    m.user.fee_credits -= bps_of(notional as u128, fee_bps).unwrap() as i128;
    kani::assert(m.user.pnl + m.lp.pnl == 0, "trade pnl is zero-sum");
    kani::assert(m.backed(), "trade: senior claims backed");

    // funding
    let size: u8 = kani::any();
    let user_long: bool = kani::any();
    let delta: u8 = kani::any();
    kani::assume(size <= 15 && delta <= 15);
    let pos: i128 = if user_long { size as i128 } else { -(size as i128) };
    let fu = funding_payment(pos, delta as i128, 10).unwrap();
    let fl = funding_payment(-pos, delta as i128, 10).unwrap();
    kani::assert(fu + fl >= 0, "funding never creates value");
    m.user.pnl -= fu;
    m.lp.pnl -= fl;

    // settle (losers first, as the engine's touch order does not matter for backing)
    let (t0, t1) = (kani::any::<u8>() as u64 % 8, kani::any::<u8>() as u64 % 8);
    kani::assume(t0 <= t1);
    let period: u8 = kani::any();
    kani::assume(period <= 4);
    settle(&mut m, false, t1 - t0, period as u64);
    kani::assert(m.backed(), "settle LP: senior claims backed");
    settle(&mut m, true, t1 - t0, period as u64);
    kani::assert(m.backed(), "settle user: senior claims backed");

    // withdraw
    let amount: u8 = kani::any();
    if (amount as u128) <= m.user.capital {
        let vault_before = m.vault;
        m.user.capital -= amount as u128;
        m.vault -= amount as u128;
        kani::assert(amount as u128 <= vault_before, "withdrawal covered by vault");
    }
    kani::assert(m.backed(), "withdraw: senior claims backed");
}
