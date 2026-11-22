# Percolator — Clawd fork

**A downstream fork of [Percolator](https://github.com/aeyakovenko/percolator),
toly's open-source perpetuals engine for Solana** — carrying the engine
(vendored, pinned), a v16 wrapper program, an insurance stake program, and a
position NFT program, with the fork's own Kani formal-verification suites and
the INV-077 invariant port. Program sources derive from the
`dcccrypto/percolator*` packages, themselves derived from the upstream engine.

## 📒 Start here: [WORK.md — the work ledger](WORK.md)

What is upstream, what this fork built, the INV-077 gap-and-fix story with
commit links, per-program ledgers, the verification table, and how to
reproduce every claim: **[WORK.md](WORK.md)**.

## What we built

- **Wrapper program** (`programs/percolator-prog/`) — the v16 Solana wrapper:
  matcher ABI gates, admin authorization, PDA matching, nonce monotonicity,
  CPI size binding, crank / oracle / EWMA / circuit-breaker logic.
- **Insurance stake program** (`programs/percolator-stake/`) — LP staking
  with proved deposit/withdraw conservation and anti-inflation math.
- **Position NFT program** (`programs/percolator-nft/`) — transferable
  Token-2022 NFTs representing open positions.
- **INV-077 port + fix** — upstream PR #465's bounded lien refresh and
  PR #471's chunked resolved-close ported in; the port exposed that Live
  had no bounded source-lien release (1,399,676 / 1,399,700 CU at 27 liens),
  and the fork's bounded-release fix closed it — 5/5 INV-077 tests green.
  Story: [WORK.md](WORK.md).
- **Kani suites for the wrapper and stake programs** — see the table below.

## Verification at a glance

| Suite | Result |
|---|---|
| Engine Kani — **upstream's** suite, carried/pinned/run by this fork | 305 harnesses, **305/305 PASS** (full run 2026-04-25, Kani 0.67.0) |
| Wrapper Kani — this fork's | 81 harnesses, **81/81 SUCCESSFUL**, 0 vacuous |
| Stake Kani — this fork's | 54 harnesses checked in (2 inductive) — results log pending, no pass claim yet |
| clawd-kani — separate public crate | 40 library harnesses — [github.com/themusebook/clawd-kani](https://github.com/themusebook/clawd-kani) |

Evidence links and reproduction commands: [WORK.md](WORK.md).

## Repo map

- `programs/percolator/` — the vendored upstream engine (pinned; not ours).
- `programs/percolator-prog/` — wrapper program + its tests and audit docs.
- `programs/percolator-stake/` — insurance stake program + `kani-proofs/`.
- `programs/percolator-nft/` — position NFT program (Token-2022).
- `cli/`, `deploy/`, `percolator-launch/`, `explorer/`, `extension/`, `test/` —
  the earlier Clawd Chain testnet launch experience, kept in tree as-is.

## Attribution — upstream vs this fork

- **Upstream work** (not ours): the Percolator engine itself, by Anatoly
  Yakovenko and contributors, and the engine crate's Kani proof suite —
  inherited through the `dcccrypto/percolator*` lineage. This fork carries,
  pins, and runs that engine suite; it did not author it.
- **This fork's own work** (the Clawd team, Solizardking): the wrapper,
  stake, and NFT programs and their Kani suites, the INV-077 port and the
  bounded Live lien release fix, and the Solana devnet rehearsal. Full
  ledger: [WORK.md](WORK.md). License attribution: `NOTICE`.

## Status

- **Devnet — rehearsed end-to-end** (G4 rehearsal, 2026-11-14). Devnet IDs:
  wrapper `6q254qTbNZmTzkzfD6Y1jdPrHo6mHPE857jY1gpaz8ZT`,
  stake `Gah4CraBbLvvvKeZeDsMVbNVsNhCCWLiQa6HeJa6YvST`,
  NFT `4ekVQZatM719dWBLhRfFQCJ26MBdzGjmFPcNLXKF5scz`.
- **Mainnet — NOT deployed.** No mainnet deployment is claimed or implied.

## Build and test

```
cargo build-sbf   # inside each programs/* crate
cd programs/percolator-prog && ./scripts/ci-test.sh
```

Kani reproduction commands are in [WORK.md](WORK.md).

## License

Apache License 2.0 — see `LICENSE`. Per-folder upstream `LICENSE` files are
preserved as received. Upstream credit: Anatoly Yakovenko and the Percolator
contributors. See `NOTICE` for the full attribution.

## Notes

- Browser-signing only in the reference tooling: nothing in this tree signs
  or sends transactions server-side.
- No keypairs, `.env` files, ledgers, or build outputs belong in this
  repository.
