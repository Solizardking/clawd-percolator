//! Example 6: CPI callback re-enters withdraw before state is updated.
//!
//! Bug: the vault checks the balance, makes an external call that may call
//! back into `withdraw`, and only then debits. The nested call sees the stale
//! balance and pays out twice.
//!
//! cargo kani --features solana-agent --tests --harness ex06_vulnerable   # expected: FAILED
//! cargo kani --features solana-agent --tests --harness ex06_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::agent::{assert_no_reentry, AgentError, ReentrancyGuard};

struct Vault {
    balance: u64,
    paid_out: u64,
    guard: ReentrancyGuard,
    active: u8,
    max_active: u8,
}

/// The external program: nondeterministically calls back into the vault once.
fn external_call(v: &mut Vault, amount: u64, depth: u8, fixed: bool) {
    if depth == 0 && kani::any() {
        let _ = if fixed { withdraw_fixed(v, amount, 1) } else { withdraw_vulnerable(v, amount, 1) };
    }
}

fn withdraw_vulnerable(v: &mut Vault, amount: u64, depth: u8) -> Result<(), AgentError> {
    if v.balance < amount {
        return Err(AgentError::InsufficientFunds);
    }
    v.active += 1;
    v.max_active = v.max_active.max(v.active);
    v.paid_out += amount; // transfer out
    external_call(v, amount, depth, false); // CPI before effects
    v.balance = v.balance.saturating_sub(amount);
    v.active -= 1;
    Ok(())
}

fn withdraw_fixed(v: &mut Vault, amount: u64, depth: u8) -> Result<(), AgentError> {
    v.guard.enter()?;
    if v.balance < amount {
        v.guard.exit();
        return Err(AgentError::InsufficientFunds);
    }
    v.active += 1;
    v.max_active = v.max_active.max(v.active);
    v.balance -= amount; // effects first
    v.paid_out += amount;
    external_call(v, amount, depth, true);
    v.active -= 1;
    v.guard.exit();
    Ok(())
}

fn check(fixed: bool) {
    let start: u32 = kani::any();
    let amount: u32 = kani::any();
    let mut v = Vault { balance: start as u64, paid_out: 0, guard: ReentrancyGuard::default(), active: 0, max_active: 0 };
    let _ = if fixed { withdraw_fixed(&mut v, amount as u64, 0) } else { withdraw_vulnerable(&mut v, amount as u64, 0) };
    kani::assert(v.paid_out <= start as u64, "never pays out more than the vault held");
    kani::assert(v.paid_out + v.balance == start as u64, "balance + paid_out conserved");
    assert_no_reentry(v.max_active);
}

#[kani::proof]
#[kani::unwind(3)]
fn ex06_vulnerable() {
    check(false);
}

#[kani::proof]
#[kani::unwind(3)]
fn ex06_fixed() {
    check(true);
}
