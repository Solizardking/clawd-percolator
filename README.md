# Percolator — Clawd fork

Perpetual futures on Solana, built on **Percolator**, the open-source perps
engine by Anatoly Yakovenko (toly):

- Upstream engine: https://github.com/aeyakovenko/percolator
- This fork's program sources derive from the `dcccrypto/percolator*` packages,
  themselves derived from the upstream engine.

This repository collects the Clawd team's work on the engine: the on-chain
program set (wrapper, insurance stake, position NFT), the INV-077 invariant
port, and the test and audit material that backs them.

## Attribution — upstream vs this fork

- **Upstream work** (not ours): the Percolator engine itself, by Anatoly
  Yakovenko and contributors, and the engine crate's Kani proof suite
  (`programs/percolator/`) — both inherited through the `dcccrypto/percolator*`
  lineage. This fork carries, pins, and runs that engine suite; it did not
  author it.
- **This fork's own work** (the Clawd team, Solizardking): the wrapper
  program and its Kani suite (`programs/percolator-prog/`), the insurance
  stake program and its Kani proof crate (`programs/percolator-stake/`), the
  position NFT program (`programs/percolator-nft/`), the INV-077 invariant
  port, the chunked-close work that went upstream as PR #471, and the Solana
  devnet rehearsal.

## What this fork adds

- **INV-077 invariant port** — the upstream INV-077 source-lien invariant
  ported into the wrapper program: bounded source-lien refresh work at trade
  admission (port of upstream PR #465), integration of the upstream
  percolator-prog PR #471 engine changes (engine pin `a55aceb3`), and a bounded
  live lien release for the PermissionlessCrank. The INV-077 test set passes
  (five of five tests green).
- **Chunked close** — the position-close work that went upstream as
  [aeyakovenko/percolator PR #471](https://github.com/aeyakovenko/percolator/pull/471).
- **Insurance stake program** (`programs/percolator-stake`) and **position NFT
  program** (`programs/percolator-nft`, Token-2022) alongside the wrapper
  (`programs/percolator-prog`, with the engine crate in `programs/percolator`).
- **Devnet rehearsal** — the full three-program set deployed and rehearsed
  end-to-end on Solana devnet (G4 rehearsal, 2026-11-14). Devnet program IDs:

  | Program | Devnet ID |
  |---|---|
  | Wrapper | `6q254qTbNZmTzkzfD6Y1jdPrHo6mHPE857jY1gpaz8ZT` |
  | Stake | `Gah4CraBbLvvvKeZeDsMVbNVsNhCCWLiQa6HeJa6YvST` |
  | NFT | `4ekVQZatM719dWBLhRfFQCJ26MBdzGjmFPcNLXKF5scz` |

  These are devnet rehearsal deployments only. No mainnet deployment is
  claimed or implied.

## Formal verification (Kani)

The invariants are not just tested, they are *proved* — with the evidence
checked in, labelled by whose work each suite is:

- **Engine suite — upstream's, carried/pinned/run by this fork.**
  `programs/percolator/kani-list.json` lists **305 harnesses** (Kani 0.67.0,
  pinned). The checked-in final audit table
  (`programs/percolator/kani_audit_final.tsv`) records **305/305 PASS** from
  the full overnight run of 2026-04-25; the runner is
  `programs/percolator/scripts/run_kani_full_audit.sh`.
- **Wrapper suite — this fork's own.** `programs/percolator-prog/tests/v16_kani.rs`
  with the audit write-up in `programs/percolator-prog/kani_audit.md`:
  **81 harnesses, 81/81 SUCCESSFUL, 0 vacuous** — matcher ABI gates, admin
  authorization, PDA matching, nonce monotonicity, CPI size binding, crank
  and circuit-breaker decisions.
- **Stake suite — this fork's own.** `programs/percolator-stake/kani-proofs/`:
  **54 harnesses checked in (52 bounded + 2 genuinely inductive)** covering
  LP deposit/withdraw conservation, anti-inflation, and fairness. Harnesses
  are checked in; a results log for this suite is **pending** — no pass claim
  is made for it yet.
- **clawd-kani — a separate, already-public crate** of reusable Kani proof
  primitives (40 library harnesses):
  https://github.com/themusebook/clawd-kani — it likewise has no checked-in
  results log yet, so treat it as a suite + CI, not a cited green run.

## Layout

- `programs/percolator-prog/` — the wrapper program (market, trade, crank,
  close), its test suite (`tests/`, including the v16 fork tests and the
  INV-077 coverage), port notes (`V14_PORT.md`, `V14_TEST_PORT_COVERAGE.md`),
  and audit material (`security.md`, `kani_audit.md`, `audit-scope.md`).
- `programs/percolator/` — the pinned engine crate.
- `programs/percolator-stake/` — insurance staking.
- `programs/percolator-nft/` — position NFTs (Token-2022).
- `cli/`, `deploy/`, `percolator-launch/`, `explorer/`, `extension/`, `test/` —
  the earlier Clawd Chain testnet launch experience, kept in tree as-is.

## Build and test

Programs (per program crate, with the Solana BPF toolchain):

```
cargo build-sbf   # inside each programs/* crate
```

Wrapper test suite:

```
cd programs/percolator-prog
./scripts/ci-test.sh
# or: cargo test
```

## License

Apache License 2.0 — see `LICENSE`. The upstream Percolator engine is
Apache-2.0 licensed; per-folder upstream `LICENSE` files are preserved as
received. Upstream credit: Anatoly Yakovenko and the Percolator contributors.
See `NOTICE` for the full attribution.

## Notes

- Browser-signing only in the reference tooling: nothing in this tree signs or
  sends transactions server-side.
- No keypairs, `.env` files, ledgers, or build outputs belong in this
  repository.
