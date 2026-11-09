//! Example 5: authority compared by key but never required to sign.
//!
//! Bug: anyone can pass the authority's public key as a non-signer account and
//! drain the agent's vault.
//!
//! cargo kani --features solana-agent --tests --harness ex05_vulnerable   # expected: FAILED
//! cargo kani --features solana-agent --tests --harness ex05_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::agent::{assert_authorized_if_ok, assert_lamports_conserved, AgentAccount, AgentError};

fn sweep_vulnerable(auth: &AgentAccount, authority: &[u8; 32], vault: &mut AgentAccount, dest: &mut AgentAccount, amt: u64) -> Result<(), AgentError> {
    if auth.key != *authority {
        return Err(AgentError::Unauthorized);
    }
    vault.debit(amt)?;
    dest.credit(amt).map_err(|e| { vault.lamports += amt; e })
}

fn sweep_fixed(auth: &AgentAccount, authority: &[u8; 32], vault: &mut AgentAccount, dest: &mut AgentAccount, amt: u64) -> Result<(), AgentError> {
    if !auth.is_signer || auth.key != *authority {
        return Err(AgentError::Unauthorized);
    }
    vault.debit(amt)?;
    dest.credit(amt).map_err(|e| { vault.lamports += amt; e })
}

type Sweep = fn(&AgentAccount, &[u8; 32], &mut AgentAccount, &mut AgentAccount, u64) -> Result<(), AgentError>;

fn check(f: Sweep) {
    let authority: [u8; 32] = kani::any();
    let caller = AgentAccount::any(0);
    let mut vault = AgentAccount::any(u32::MAX as u64);
    let mut dest = AgentAccount::any(u32::MAX as u64);
    let before = [vault, dest];
    let r = f(&caller, &authority, &mut vault, &mut dest, kani::any());
    assert_lamports_conserved(&before, &[vault, dest]);
    assert_authorized_if_ok(&r, &caller, &authority);
}

#[kani::proof]
fn ex05_vulnerable() {
    check(sweep_vulnerable);
}

#[kani::proof]
fn ex05_fixed() {
    check(sweep_fixed);
}
