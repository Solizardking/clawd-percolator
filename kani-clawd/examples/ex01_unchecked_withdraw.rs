//! Example 1: unchecked withdraw.
//!
//! Bug: the balance debit uses wrapping arithmetic, so withdrawing more than
//! the balance wraps the user to a huge balance and drains the vault.
//!
//! cargo kani --tests --harness ex01_vulnerable   # expected: FAILED
//! cargo kani --tests --harness ex01_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

fn withdraw_vulnerable(balance: u64, vault: u64, amount: u64) -> Result<(u64, u64), ()> {
    Ok((balance.wrapping_sub(amount), vault.wrapping_sub(amount)))
}

fn withdraw_fixed(balance: u64, vault: u64, amount: u64) -> Result<(u64, u64), ()> {
    let b = balance.checked_sub(amount).ok_or(())?;
    let v = vault.checked_sub(amount).ok_or(())?;
    Ok((b, v))
}

fn check(f: fn(u64, u64, u64) -> Result<(u64, u64), ()>) {
    let balance: u64 = kani::any();
    let vault: u64 = kani::any();
    kani::assume(balance <= vault); // vault holds at least this user's balance
    let amount: u64 = kani::any();
    if let Ok((b, v)) = f(balance, vault, amount) {
        kani::assert(b <= balance, "withdraw never increases the user's balance");
        kani::assert(b <= v, "vault still covers the user's remaining balance");
        kani::assert(balance - b == amount, "debited exactly the amount paid out");
    }
}

#[kani::proof]
fn ex01_vulnerable() {
    check(withdraw_vulnerable);
}

#[kani::proof]
fn ex01_fixed() {
    check(withdraw_fixed);
}
