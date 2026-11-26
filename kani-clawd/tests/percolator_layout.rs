//! On-chain truth 2026-11-09: clawd house portfolios are 9539 B
//! (e.g. `A5cB2hDT6trFDPwwmgb7qEpjq2SEmph7hRZRxhErPv28`); markets 3163 B
//! (e.g. `DNnhA2Lvuvu3Ywn1KJRCJqvMeZkZ2qndBXJidUMTQ6PY`). Handoff 9555 is wrong.
//! Plain `cargo test` guard (no Kani needed) on the Percolator account sizes
//! the wrapper allocates. Mirrors `percolator_account_layout_sizes`.
use core::mem::size_of;
use percolator::{Market, MarketGroupV16HeaderAccount, PortfolioAccountV16Account};

const HEADER_LEN: usize = 16;
const WRAPPER_CONFIG_LEN: usize = 576;
const ASSET_ORACLE_WRAPPER_LEN: usize = 512;
const PORTFOLIO_MATCHER_CONFIG_LEN: usize = 104;

#[test]
fn market_account_sizes() {
    let group = size_of::<MarketGroupV16HeaderAccount>();
    let slot = size_of::<Market<[u8; ASSET_ORACLE_WRAPPER_LEN]>>();
    assert_eq!(HEADER_LEN + WRAPPER_CONFIG_LEN + group + slot, 3_163);
    assert_eq!(slot, 1_813);
}

#[test]
fn portfolio_account_sizes() {
    let state = size_of::<PortfolioAccountV16Account>();
    assert_eq!(state, 9_419);
    assert_eq!(HEADER_LEN + state + PORTFOLIO_MATCHER_CONFIG_LEN, 9_539 /* on-chain clawd+devnet 2026-11-09; NOT 9555 */);
}

#[test]
fn engine_layout_discriminator_is_v18() {
    // clawd's live wrapper layout is VERSION 18; the engine bumps its
    // discriminator in lockstep with Pod layout changes.
    assert_eq!(percolator::V16_LAYOUT_DISCRIMINATOR, 18);
}
