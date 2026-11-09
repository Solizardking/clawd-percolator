//! Example 7: market lifecycle that lets an admin reopen a resolved market.
//!
//! Bug: `set_phase` only checks the admin flag, so `Resolved -> Active` (or
//! `Closed -> Active`) is reachable and settled positions can be re-traded.
//!
//! cargo kani --features solana-agent --tests --harness ex07_vulnerable   # expected: FAILED
//! cargo kani --features solana-agent --tests --harness ex07_fixed        # expected: SUCCESSFUL
#![cfg(kani)]

use kani_clawd::agent::{assert_transition_allowed, guarded_transition, AgentError, StateMachine};

#[derive(Clone, Copy, PartialEq, Eq, Debug, kani::Arbitrary)]
enum Phase {
    Pending,
    Active,
    Resolved,
    Closed,
}

impl StateMachine for Phase {
    fn can_transition(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Phase::Pending, Phase::Active) | (Phase::Active, Phase::Resolved) | (Phase::Resolved, Phase::Closed)
        )
    }
}

const EDGES: [(Phase, Phase); 3] = [
    (Phase::Pending, Phase::Active),
    (Phase::Active, Phase::Resolved),
    (Phase::Resolved, Phase::Closed),
];

fn set_phase_vulnerable(state: &mut Phase, to: Phase, is_admin: bool) -> Result<(), AgentError> {
    if !is_admin {
        return Err(AgentError::Unauthorized);
    }
    *state = to;
    Ok(())
}

fn set_phase_fixed(state: &mut Phase, to: Phase, is_admin: bool) -> Result<(), AgentError> {
    if !is_admin {
        return Err(AgentError::Unauthorized);
    }
    guarded_transition(state, to)
}

fn check(f: fn(&mut Phase, Phase, bool) -> Result<(), AgentError>) {
    let mut s: Phase = kani::any();
    let from = s;
    if f(&mut s, kani::any(), kani::any()).is_ok() {
        assert_transition_allowed(from, s, &EDGES, true);
    } else {
        kani::assert(s == from, "rejected call leaves phase unchanged");
    }
}

#[kani::proof]
#[kani::unwind(5)]
fn ex07_vulnerable() {
    check(set_phase_vulnerable);
}

#[kani::proof]
#[kani::unwind(5)]
fn ex07_fixed() {
    check(set_phase_fixed);
}
