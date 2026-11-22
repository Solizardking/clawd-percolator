# What this fork adds — work ledger

A reviewer's map of the work in this repository: what is upstream, what is
ours, where the evidence lives, and how to reproduce it.

## Upstream baseline

- The risk engine is **vendored** at `programs/percolator/`. It is toly's
  Percolator (https://github.com/aeyakovenko/percolator), reaching this fork
  through the `dcccrypto/percolator*` lineage.
- The engine compiled into the wrapper is pinned at upstream commit
  `35ddd692098f9028947a105df27f7a9f3f85ef43` plus fork grafts
  (`percolator deploy/v18.1-engine-267`, `c141d47f` + percolator#267) — the
  pin table is in `programs/percolator-prog/audit-scope.md`.
- This is a **downstream fork**. Nothing in this tree is merged upstream;
  where upstream work was ported in, the port is credited commit-by-commit
  below.

## The INV-077 story

INV-077 is the source-lien invariant: an account's settled claims must never
exceed what its source legs can actually pay. Porting upstream's fix into
this fork took three commits — and exposed a real gap in the fork, which
became the fork's own fix.

1. **Bounded lien refresh at trade admission** —
   [`510635b`](https://github.com/Solizardking/clawd-percolator/commit/510635b)
   (2026-11-14). Port of upstream PR #465: source-lien refresh work at trade
   admission is bounded, so admission cannot be turned into an unbounded
   lien walk.
2. **Chunked resolved-close** —
   [`93a6f30`](https://github.com/Solizardking/clawd-percolator/commit/93a6f30)
   (2026-11-17). Ports the engine change from upstream PR #471 (pinned
   upstream as `a55aceb3`) into `programs/percolator/src/v16.rs`: a resolved
   close now settles at most 2 legs per call
   (`RESOLVED_KF_SETTLEMENT_LEGS_PER_CALL`), so a resolved account holding
   many losing legs can actually finish closing instead of exceeding the
   transaction CU limit and blocking payouts market-wide. PR #471's five
   INV-077 tests were ported to `programs/percolator-prog/tests/v16_cu.rs`.
3. **The gap the port exposed — and the fork's fix** —
   [`696bac5`](https://github.com/Solizardking/clawd-percolator/commit/696bac5)
   (2026-11-17). The ported tests showed the fork had **no bounded Live lien
   release**: this fork's wrapper reaches the auto-crank only in Recovery,
   so in Live the Refresh crank never released source liens, and the convert
   pre-step consumed **1,399,676 of the 1,399,700 CU budget at 27 liens** —
   effectively stalled. The fix gives the source-lien release core a
   per-call domain budget (1 domain per call on the crank path,
   `AUTO_CRANK_SOURCE_LIEN_RELEASE_DOMAINS_PER_CALL`) and has the Live
   Refresh crank perform the bounded release: a release crank on the
   27-lien fixture measures **731,555 CU**. With it, the ported INV-077 test
   set is **5/5 green**. The full measurement write-up is in the commit
   message.

## Per-program ledger

### `programs/percolator-prog/` — the v16 wrapper (ours)

The Solana program that wraps the engine: market creation, trade, crank,
close. Its safety surface is specified and proved in
`programs/percolator-prog/kani_audit.md`: matcher ABI gates (every
documented failure mode rejected, valid returns accepted), admin
authorization (including the burned-admin ⇒ reject invariant), PDA
matching, nonce monotonicity with the u64::MAX overflow gate, CPI size
binding, `decide_trade_cpi` characterizations, crank decisions, oracle
inversion, EWMA clamp, circuit-breaker clamp, and fee-weighted EWMA.
Port notes: `V14_PORT.md`, `V14_TEST_PORT_COVERAGE.md`; security notes:
`security.md`, `audit-scope.md`.

### `programs/percolator-stake/` — insurance stake (ours)

LP staking against the insurance fund. The proved surface is the LP math:
deposit/withdraw conservation, anti-inflation (first-depositor and
pro-rata), fairness and monotonicity, no overflow/panic — in the dedicated
proof crate `programs/percolator-stake/kani-proofs/`, including two
genuinely inductive proofs (PERC-760, §14) that start from an arbitrary
pool state satisfying the pool invariant, and u32-mirror scale-invariance
(PERC-761). E2E coverage: `tests/v16_stake_insurance_e2e.rs`.

### `programs/percolator-nft/` — position NFTs (ours)

Transferable Token-2022 NFTs representing open positions: mint/burn of
supply-1 position NFTs, a `PositionNft` PDA linking mint → portfolio +
market, and a holder-only settle-funding crank before transfer. It reads
position state directly from portfolio accounts (CPI-free). See
`programs/percolator-nft/README.md`.

### `programs/percolator/` — the engine (upstream's)

Toly's engine, vendored and pinned as above. Not our work; it is here so
the wrapper builds against an exact, auditable engine revision.

## Verification — what the checked-in evidence supports

| Suite | Claim | Evidence |
|---|---|---|
| Engine Kani (upstream's suite, carried/pinned/run by this fork) | 305 harnesses, **305/305 PASS**, full overnight run 2026-04-25, Kani 0.67.0 (pinned) | `programs/percolator/kani-list.json`, `programs/percolator/kani_audit_final.tsv` |
| Wrapper Kani (this fork's) | 81 harnesses, **81/81 SUCCESSFUL**, 0 vacuous | `programs/percolator-prog/kani_audit.md` |
| Stake Kani (this fork's) | 54 harnesses checked in (52 bounded + 2 inductive) — **results log pending; no pass claim yet** | `programs/percolator-stake/kani-proofs/` |
| INV-077 tests (this fork's port + fix) | 5/5 green | `programs/percolator-prog/tests/v16_cu.rs`, commits `93a6f30` + `696bac5` |
| clawd-kani (separate crate) | 40 library harnesses, no checked-in results log | https://github.com/themusebook/clawd-kani — linked, not vendored |

## Reproduce it

Kani is pinned to 0.67.0 across this repo:

```
cargo install --locked kani-verifier@0.67.0 && cargo kani setup
```

Engine suite — full audit (writes the results TSV cited above):

```
cd programs/percolator
./scripts/run_kani_full_audit.sh
```

Wrapper suite — proofs and tests:

```
cd programs/percolator-prog
cargo kani --tests -j     # the 81-harness audit in kani_audit.md
./scripts/ci-test.sh      # full test suite, run the way CI runs it
```

Stake suite:

```
cd programs/percolator-stake/kani-proofs
cargo kani --lib
cargo kani --harness proof_deposit_withdraw_no_inflation_inductive   # one proof
```

## Deployments and status

- **Solana devnet — rehearsed.** The three-program set was deployed and
  rehearsed end-to-end on devnet (G4 rehearsal, 2026-11-14). Devnet IDs:
  wrapper `6q254qTbNZmTzkzfD6Y1jdPrHo6mHPE857jY1gpaz8ZT`,
  stake `Gah4CraBbLvvvKeZeDsMVbNVsNhCCWLiQa6HeJa6YvST`,
  NFT `4ekVQZatM719dWBLhRfFQCJ26MBdzGjmFPcNLXKF5scz`.
- **Mainnet — NOT deployed.** Terms are staged only; no mainnet deployment
  is claimed or implied anywhere in this repository.
- `cli/`, `deploy/`, `percolator-launch/`, `explorer/`, `extension/` are
  the earlier Clawd Chain testnet launch experience, kept in tree as-is.
