//! # kani-clawd
//!
//! Reusable [Kani](https://github.com/model-checking/kani) verification
//! primitives and proof harnesses for Solana protocol math, built around the
//! Percolator perpetuals risk engine used by Clawd Chain.
//!
//! Everything in this crate is compiled only under `cfg(kani)` (and under
//! `cfg(test)` for the crate's own unit tests). A normal `cargo build` of a
//! dependent crate compiles `kani_clawd` to an empty library, so it is safe to
//! keep in `[dev-dependencies]` with zero runtime cost.
//!
//! Modules:
//! - [`risk`]: checked Percolator-style risk math (haircut, effective PnL,
//!   warmup, fee-debt sweep, funding, loss write-off) with proofs.
//! - [`math`], [`token`], [`staking`], [`bounds`]: assert-style helpers that
//!   wrap caller closures (spec equivalence, split conservation, monotone
//!   multipliers, bounded outputs).
//! - [`generators`] (kani only): constrained symbolic values for Solana domains.
//! - [`agent`] (feature `solana-agent`): agent-account invariants, reentrancy,
//!   CPI allowlists, PDA seed limits, oracle consensus, FSM guards.
//! - [`account_info`] (feature `solana-account-info`): symbolic
//!   `solana_program::AccountInfo` plus [`account_info::LamportSnapshot`].
#![no_std]
#![cfg_attr(not(any(kani, test)), allow(unused))]

#[cfg(any(kani, test))]
extern crate std;

/// Report a property violation. Under Kani this is `kani::assert` (a proof
/// obligation); under `cargo test` it is a plain `assert!`. The message must be
/// a string literal (Kani requirement).
#[macro_export]
macro_rules! check {
    ($cond:expr, $msg:expr $(,)?) => {{
        #[cfg(kani)]
        $crate::__kani::assert($cond, $msg);
        #[cfg(not(kani))]
        assert!($cond, "{}", $msg);
    }};
}

/// Restrict the search space. Under Kani this is `kani::assume`; under
/// `cargo test` an unmet assumption is a test-setup bug and panics.
#[cfg(any(kani, test))]
#[inline(always)]
#[track_caller]
pub fn require(cond: bool) {
    #[cfg(kani)]
    kani::assume(cond);
    #[cfg(not(kani))]
    assert!(cond, "assumption violated in concrete test");
}

#[cfg(kani)]
#[doc(hidden)]
pub use kani as __kani;

#[cfg(any(kani, test))]
pub mod bounds;
#[cfg(any(kani, test))]
pub mod math;
#[cfg(any(kani, test))]
pub mod risk;
#[cfg(any(kani, test))]
pub mod staking;
#[cfg(any(kani, test))]
pub mod token;

#[cfg(kani)]
pub mod generators;

#[cfg(all(any(kani, test), feature = "solana-agent"))]
pub mod agent;

#[cfg(all(kani, feature = "solana-account-info"))]
pub mod account_info;
