//! Agent-program invariants (feature `solana-agent`).
//!
//! A lightweight account model for autonomous on-chain agents (keepers,
//! vault managers, trading bots) that is cheap for CBMC: fixed-size keys,
//! `u64` lamports, explicit signer/writable flags. Helpers cover lamport
//! conservation, authorization, reentrancy, CPI allowlists, PDA seed limits,
//! oracle consensus and state-machine guards. The [`cpi_stub!`](crate::cpi_stub)
//! and [`cpi_contract!`](crate::cpi_contract) macros model external programs.


pub type Pubkey32 = [u8; 32];
pub type ProgramId = Pubkey32;

/// Solana's runtime limit on PDA seeds per derivation.
pub const MAX_SEEDS: usize = 16;
/// Solana's runtime limit on bytes per PDA seed.
pub const MAX_SEED_LEN: usize = 32;
/// Solana's CPI invoke depth limit.
pub const MAX_CPI_DEPTH: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentError {
    Unauthorized,
    Reentrancy,
    CpiNotAllowed,
    InvalidSeeds,
    OracleDisagreement,
    InvalidTransition,
    InsufficientFunds,
    Overflow,
}

/// A symbolic agent-visible account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentAccount {
    pub key: Pubkey32,
    pub owner: ProgramId,
    pub authority: Pubkey32,
    pub lamports: u64,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl AgentAccount {
    /// A fully symbolic account with `lamports <= max_lamports`.
    #[cfg(kani)]
    pub fn any(max_lamports: u64) -> Self {
        let a = AgentAccount {
            key: kani::any(),
            owner: kani::any(),
            authority: kani::any(),
            lamports: kani::any(),
            is_signer: kani::any(),
            is_writable: kani::any(),
        };
        kani::assume(a.lamports <= max_lamports);
        a
    }

    /// A symbolic account owned by `program` (e.g. the agent's own state).
    #[cfg(kani)]
    pub fn any_owned_by(program: ProgramId, max_lamports: u64) -> Self {
        let mut a = Self::any(max_lamports);
        a.owner = program;
        a
    }

    /// Debit `amount` lamports (checked).
    pub fn debit(&mut self, amount: u64) -> Result<(), AgentError> {
        self.lamports = self.lamports.checked_sub(amount).ok_or(AgentError::InsufficientFunds)?;
        Ok(())
    }

    /// Credit `amount` lamports (checked).
    pub fn credit(&mut self, amount: u64) -> Result<(), AgentError> {
        self.lamports = self.lamports.checked_add(amount).ok_or(AgentError::Overflow)?;
        Ok(())
    }
}

/// Sum of lamports across accounts, in `u128` so it cannot overflow.
pub fn total_lamports(accounts: &[AgentAccount]) -> u128 {
    let mut sum = 0u128;
    let mut i = 0;
    while i < accounts.len() {
        sum += accounts[i].lamports as u128;
        i += 1;
    }
    sum
}

/// Assert two lamport vectors have the same total.
#[track_caller]
pub fn assert_lamports_conserved(before: &[AgentAccount], after: &[AgentAccount]) {
    crate::check!(total_lamports(before) == total_lamports(after), "lamports not conserved");
}

/// Move lamports between two accounts; rejects unsigned or read-only sources.
pub fn transfer_lamports(
    from: &mut AgentAccount,
    to: &mut AgentAccount,
    amount: u64,
) -> Result<(), AgentError> {
    if !from.is_signer || !from.is_writable || !to.is_writable {
        return Err(AgentError::Unauthorized);
    }
    let new_from = from.lamports.checked_sub(amount).ok_or(AgentError::InsufficientFunds)?;
    let new_to = to.lamports.checked_add(amount).ok_or(AgentError::Overflow)?;
    from.lamports = new_from;
    to.lamports = new_to;
    Ok(())
}

/// The account is a signer whose key equals the stored authority.
pub fn is_authorized(signer: &AgentAccount, authority: &Pubkey32) -> bool {
    signer.is_signer && signer.key == *authority
}

/// Assert an operation that succeeded was authorized.
#[track_caller]
pub fn assert_authorized_if_ok<T, E>(result: &Result<T, E>, signer: &AgentAccount, authority: &Pubkey32) {
    if result.is_ok() {
        crate::check!(is_authorized(signer, authority), "state change without authority signature");
    }
}

/// Reentrancy lock. `enter` fails while held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReentrancyGuard {
    pub locked: bool,
    pub entries: u8,
}

impl ReentrancyGuard {
    pub fn enter(&mut self) -> Result<(), AgentError> {
        if self.locked {
            return Err(AgentError::Reentrancy);
        }
        self.locked = true;
        self.entries = self.entries.saturating_add(1);
        Ok(())
    }

    pub fn exit(&mut self) {
        self.locked = false;
    }
}

/// Assert a handler never runs nested: `active_frames` counts frames that are
/// inside the critical section at once.
#[track_caller]
pub fn assert_no_reentry(active_frames: u8) {
    crate::check!(active_frames <= 1, "handler re-entered while in critical section");
}

/// Fixed allowlist of CPI target programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpiAllowlist<const N: usize> {
    pub programs: [ProgramId; N],
}

impl<const N: usize> CpiAllowlist<N> {
    pub const fn new(programs: [ProgramId; N]) -> Self {
        Self { programs }
    }

    pub fn contains(&self, program: &ProgramId) -> bool {
        let mut i = 0;
        while i < N {
            if self.programs[i] == *program {
                return true;
            }
            i += 1;
        }
        false
    }

    pub fn require(&self, program: &ProgramId) -> Result<(), AgentError> {
        if self.contains(program) {
            Ok(())
        } else {
            Err(AgentError::CpiNotAllowed)
        }
    }
}

/// Assert a CPI target is allowlisted.
#[track_caller]
pub fn assert_cpi_allowed<const N: usize>(list: &CpiAllowlist<N>, program: &ProgramId) {
    crate::check!(list.contains(program), "CPI to program outside allowlist");
}

/// Assert CPI depth stays within the runtime limit.
#[track_caller]
pub fn assert_cpi_depth(depth: u8) {
    crate::check!(depth <= MAX_CPI_DEPTH, "CPI depth exceeds runtime limit");
}

/// PDA seeds satisfy Solana's count and length limits.
pub fn pda_seeds_valid(seeds: &[&[u8]]) -> bool {
    if seeds.len() > MAX_SEEDS {
        return false;
    }
    let mut i = 0;
    while i < seeds.len() {
        if seeds[i].len() > MAX_SEED_LEN {
            return false;
        }
        i += 1;
    }
    true
}

/// Assert PDA seeds satisfy Solana's limits.
#[track_caller]
pub fn assert_pda_seeds_valid(seeds: &[&[u8]]) {
    crate::check!(pda_seeds_valid(seeds), "PDA seeds exceed MAX_SEEDS/MAX_SEED_LEN");
}

/// Median-of-three oracle consensus: returns the median if every quote is
/// within `max_dev_bps` of it, else `None`. Zero prices are rejected.
pub fn oracle_consensus(quotes: [u64; 3], max_dev_bps: u16) -> Option<u64> {
    let [a, b, c] = quotes;
    let m = if (a <= b) == (b <= c) {
        b
    } else if (b <= a) == (a <= c) {
        a
    } else {
        c
    };
    if m == 0 || max_dev_bps > 10_000 {
        return None;
    }
    let band = (m as u128) * (max_dev_bps as u128);
    let mut i = 0;
    while i < 3 {
        let q = quotes[i];
        let dev = if q > m { q - m } else { m - q } as u128;
        if dev * 10_000 > band {
            return None;
        }
        i += 1;
    }
    Some(m)
}

/// Assert a consensus price is one of the quotes, inside [min, max], and every
/// quote is within the deviation band.
#[track_caller]
pub fn assert_consensus_sound(quotes: [u64; 3], max_dev_bps: u16, price: Option<u64>) {
    if let Some(p) = price {
        let lo = quotes[0].min(quotes[1]).min(quotes[2]);
        let hi = quotes[0].max(quotes[1]).max(quotes[2]);
        crate::check!(p >= lo && p <= hi, "consensus outside quote range");
        crate::check!(p == quotes[0] || p == quotes[1] || p == quotes[2], "consensus not a quote");
        let mut i = 0;
        while i < 3 {
            let q = quotes[i];
            let dev = if q > p { q - p } else { p - q } as u128;
            crate::check!(dev * 10_000 <= (p as u128) * (max_dev_bps as u128), "quote outside band accepted");
            i += 1;
        }
    }
}

/// A finite state machine whose legal edges are given by `can_transition`.
pub trait StateMachine: Copy + PartialEq {
    fn can_transition(self, to: Self) -> bool;
}

/// Apply a transition only if the edge is legal.
pub fn guarded_transition<S: StateMachine>(state: &mut S, to: S) -> Result<(), AgentError> {
    if state.can_transition(to) {
        *state = to;
        Ok(())
    } else {
        Err(AgentError::InvalidTransition)
    }
}

/// Assert `(from, to)` is in an explicit edge list (or a self-loop when `allow_stay`).
#[track_caller]
pub fn assert_transition_allowed<S: PartialEq + Copy>(from: S, to: S, edges: &[(S, S)], allow_stay: bool) {
    if allow_stay && from == to {
        return;
    }
    let mut ok = false;
    let mut i = 0;
    while i < edges.len() {
        if edges[i].0 == from && edges[i].1 == to {
            ok = true;
        }
        i += 1;
    }
    crate::check!(ok, "illegal state transition");
}

/// Stub an external program for Kani: asserts the CPI target is allowlisted and
/// returns an unconstrained (`kani::any()`) result.
///
/// ```ignore
/// cpi_stub!(fn spl_transfer(program: ProgramId, amount: u64) -> bool;
///           allowlist = ALLOWED; target = program);
/// ```
#[macro_export]
macro_rules! cpi_stub {
    ($vis:vis fn $name:ident ( $($arg:ident : $ty:ty),* $(,)? ) -> $ret:ty ;
     allowlist = $allow:expr ; target = $target:expr $(;)?) => {
        #[allow(unused_variables)]
        $vis fn $name($($arg: $ty),*) -> $ret {
            $crate::check!(
                ($allow).contains(&$target),
                concat!("CPI target not allowlisted in ", stringify!($name)),
            );
            $crate::__kani::any()
        }
    };
}

/// Assume-guarantee contract for an external program: the caller must satisfy
/// `requires` (asserted), and the callee's result is any value satisfying
/// `ensures` (assumed).
///
/// ```ignore
/// cpi_contract!(fn oracle_read(slot: u64) -> u64;
///               requires slot > 0; ensures |p| p > 0 && p <= MAX_PRICE);
/// ```
#[macro_export]
macro_rules! cpi_contract {
    ($vis:vis fn $name:ident ( $($arg:ident : $ty:ty),* $(,)? ) -> $ret:ty ;
     requires $pre:expr ; ensures |$r:ident| $post:expr $(;)?) => {
        #[allow(unused_variables)]
        $vis fn $name($($arg: $ty),*) -> $ret {
            $crate::check!($pre, concat!("CPI precondition violated in ", stringify!($name)));
            let $r: $ret = $crate::__kani::any();
            $crate::__kani::assume($post);
            $r
        }
    };
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn agent_transfer_conserves_lamports() {
        let mut a = AgentAccount::any(u64::MAX);
        let mut b = AgentAccount::any(u64::MAX);
        let before = [a, b];
        let amount: u64 = kani::any();
        let r = transfer_lamports(&mut a, &mut b, amount);
        assert_lamports_conserved(&before, &[a, b]);
        if r.is_ok() {
            kani::assert(before[0].is_signer, "only signed sources are debited");
        } else {
            kani::assert(a == before[0] && b == before[1], "failed transfer has no effect");
        }
    }

    #[kani::proof]
    fn agent_reentrancy_guard_blocks_nested_entry() {
        let mut g = ReentrancyGuard::default();
        kani::assert(g.enter().is_ok(), "first entry allowed");
        kani::assert(g.enter() == Err(AgentError::Reentrancy), "nested entry rejected");
        g.exit();
        kani::assert(g.enter().is_ok(), "re-entry after exit allowed");
    }

    #[kani::proof]
    fn agent_cpi_allowlist_exact() {
        const LIST: CpiAllowlist<2> = CpiAllowlist::new([[1u8; 32], [2u8; 32]]);
        let p: ProgramId = kani::any();
        let ok = LIST.require(&p).is_ok();
        kani::assert(ok == (p == [1u8; 32] || p == [2u8; 32]), "allowlist accepts exactly its members");
    }

    #[kani::proof]
    #[kani::unwind(4)]
    fn agent_pda_seed_limits() {
        let a: [u8; 33] = kani::any();
        let la: usize = kani::any();
        kani::assume(la <= 33);
        let seeds: [&[u8]; 2] = [&a[..la], b"vault"];
        kani::assert(pda_seeds_valid(&seeds) == (la <= MAX_SEED_LEN), "seed length limit enforced");
    }

    #[kani::proof]
    #[kani::unwind(4)]
    fn agent_oracle_consensus_sound() {
        let q: [u64; 3] = [kani::any(), kani::any(), kani::any()];
        let bps: u16 = kani::any();
        let r = oracle_consensus(q, bps);
        assert_consensus_sound(q, bps, r);
        kani::cover!(r.is_some() && q[0] != q[1], "non-trivial consensus reachable");
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug, kani::Arbitrary)]
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

    #[kani::proof]
    #[kani::unwind(7)]
    fn agent_fsm_guard_only_legal_edges() {
        let mut s: Phase = kani::any();
        let from = s;
        let to: Phase = kani::any();
        let r = guarded_transition(&mut s, to);
        const EDGES: [(Phase, Phase); 5] = [
            (Phase::Idle, Phase::Armed),
            (Phase::Armed, Phase::Executing),
            (Phase::Armed, Phase::Idle),
            (Phase::Executing, Phase::Settled),
            (Phase::Settled, Phase::Idle),
        ];
        if r.is_ok() {
            assert_transition_allowed(from, s, &EDGES, false);
        } else {
            kani::assert(s == from, "rejected transition leaves state unchanged");
        }
    }

    const ALLOWED: CpiAllowlist<1> = CpiAllowlist::new([[7u8; 32]]);
    crate::cpi_stub!(fn stub_transfer(program: ProgramId, amount: u64) -> bool;
                     allowlist = ALLOWED; target = program);
    crate::cpi_contract!(fn oracle_read(slot: u64) -> u64;
                         requires slot > 0; ensures |p| p > 0 && p <= 1_000_000);

    #[kani::proof]
    fn agent_cpi_macros_compose() {
        let _ = stub_transfer([7u8; 32], kani::any());
        let p = oracle_read(1);
        kani::assert(p > 0 && p <= 1_000_000, "contract postcondition holds for caller");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concrete_agent() {
        assert_eq!(oracle_consensus([100, 101, 99], 200), Some(100));
        assert_eq!(oracle_consensus([100, 150, 99], 200), None);
        assert!(pda_seeds_valid(&[b"a", b"b"]));
    }
}
