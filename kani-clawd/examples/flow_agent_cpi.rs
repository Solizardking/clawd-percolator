//! End-to-end flow 2: agent arms, reads an oracle via CPI, executes a guarded
//! lamport transfer plus an allowlisted token CPI, and settles.
//!
//! cargo kani --features solana-agent --tests --harness flow_agent_cpi   # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::agent::*;
use kani_clawd::{cpi_contract, cpi_stub};

const AGENT_PROGRAM: ProgramId = [1u8; 32];
const TOKEN_PROGRAM: ProgramId = [6u8; 32];
const ALLOWED: CpiAllowlist<1> = CpiAllowlist::new([TOKEN_PROGRAM]);
const MAX_PRICE: u64 = 1_000_000;

cpi_stub!(fn token_transfer(program: ProgramId, amount: u64) -> bool;
          allowlist = ALLOWED; target = program);
cpi_contract!(fn oracle_read(feed: u8) -> u64;
              requires feed < 3; ensures |p| p > 0 && p <= MAX_PRICE);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Armed,
    Executing,
    Settled,
}

impl StateMachine for Phase {
    fn can_transition(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Phase::Idle, Phase::Armed)
                | (Phase::Armed, Phase::Executing)
                | (Phase::Armed, Phase::Idle)
                | (Phase::Executing, Phase::Settled)
                | (Phase::Settled, Phase::Idle)
        )
    }
}

struct Agent {
    phase: Phase,
    authority: [u8; 32],
    guard: ReentrancyGuard,
    max_notional_per_price: u64,
}

fn run(agent: &mut Agent, signer: &AgentAccount, vault: &mut AgentAccount, dest: &mut AgentAccount, amount: u64) -> Result<(), AgentError> {
    if !is_authorized(signer, &agent.authority) {
        return Err(AgentError::Unauthorized);
    }
    guarded_transition(&mut agent.phase, Phase::Armed)?;
    let quotes = [oracle_read(0), oracle_read(1), oracle_read(2)];
    let price = match oracle_consensus(quotes, 100) {
        Some(p) => p,
        None => {
            guarded_transition(&mut agent.phase, Phase::Idle)?;
            return Err(AgentError::OracleDisagreement);
        }
    };
    assert_consensus_sound(quotes, 100, Some(price));
    if amount > agent.max_notional_per_price.saturating_mul(price) {
        guarded_transition(&mut agent.phase, Phase::Idle)?;
        return Err(AgentError::InsufficientFunds);
    }
    guarded_transition(&mut agent.phase, Phase::Executing)?;
    agent.guard.enter()?;
    let r = transfer_lamports(vault, dest, amount);
    if r.is_ok() {
        let _ = token_transfer(TOKEN_PROGRAM, amount);
    }
    agent.guard.exit();
    guarded_transition(&mut agent.phase, Phase::Settled)?;
    guarded_transition(&mut agent.phase, Phase::Idle)?;
    r
}

#[kani::proof]
#[kani::unwind(34)] // 32-byte key comparisons unroll memcmp
fn flow_agent_cpi() {
    let authority: [u8; 32] = kani::any();
    let mut agent = Agent { phase: Phase::Idle, authority, guard: ReentrancyGuard::default(), max_notional_per_price: kani::any::<u16>() as u64 };
    let signer = AgentAccount::any(0);
    let mut vault = AgentAccount::any_owned_by(AGENT_PROGRAM, u32::MAX as u64);
    vault.is_signer = true; // PDA signs via invoke_signed
    let mut dest = AgentAccount::any(u32::MAX as u64);
    let before = [vault, dest];

    let r = run(&mut agent, &signer, &mut vault, &mut dest, kani::any());

    assert_lamports_conserved(&before, &[vault, dest]);
    assert_authorized_if_ok(&r, &signer, &authority);
    assert_no_reentry(if agent.guard.locked { 2 } else { 1 });
    kani::assert(agent.phase == Phase::Idle || agent.phase == Phase::Armed, "flow ends idle (or armed if rejected)");
    if r.is_err() {
        kani::assert(vault.lamports == before[0].lamports, "failed run moves no lamports");
    }
    kani::cover!(r.is_ok() && vault.lamports < before[0].lamports, "a real transfer is reachable");
}
