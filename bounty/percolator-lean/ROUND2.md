# Round 2 — percolator-lean bounty, reloaded (2026-11-25)

Round 1 was claimed 2026-11-25 19:50 EST (see WRITEUP.md). The house reloaded the
bounty the same evening with a **patched program** and a fresh market.

## Round-2 targets (verified on-chain 2026-11-25 ~21:41–21:47 EST)

- Market: `BhYaG2JYRHGrLpWs85TroedmkcG7toX1Ly5YbG1FDZHH` — insurance 70,000,000
  (70 USDC), totalInsShares 70,000,000, OI 6,674,572 long / 6,674,572 short
  (matched house pair, unlike round 1's zero OI)
- DEX: `CedQKVmmHuta5QcqRNkA96smV33LYtQBHo9wc1YcmFq3` (fresh — not round 1's DEX)
- Vault (shared DEX collateral vault): `6k2H4GiRDGa7o9X8Ai99sGkPQGAvrAW7gVbKkAKuH2Fk`
  — raw SPL balance moves with trader collateral (observed 70 → 120 → 70 → 120 →
  170 USDC, 50-USDC steps by `HqVkeePk36VrryZhXFBqmKTebbkcQ4P7zc7VdeLNcCYm`);
  the pot is the market's insurance accounting, not the raw vault balance
- Admin: `A3Mu2nQdjJXhJkuUDBbF2BdvgDs5KodNE9XsetXNMrCK`; market created
  2026-11-26T01:55:29Z, tx `5VSDQCSAGhn2stYD29Ws5sUkspn3AaRiEHZvVWN9aJE9NvM6NDctnKHybVxYihJ9EUCbpTzttMZaBv7MYJrLzbbG`
- Programs (all three upgraded in place, same sizes, new hashes):
  - core `GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm` — 73,224 B,
    sha256 `5f9e564b0db3802033dbe3d2c945a4075eed33f3edf7839474c3e7ffd6996f3b`
    (round 1: `c6dc8329…`)
  - matcher `EFLgSpuiyXxHJHFAWCZtmzdcTXeHguiMSUyRHN3z3FDR` — 8,744 B,
    sha256 `ccb26bef36235204e9ec6ac3e069fd1be1fc145fd969b2e7f9d2505f883ff052`
    (round 1: `9d79d159…`)
  - oracle_adapter `B4v96XfX4df9NtPbHjYsLTYkrjccGqWt4nsgyPpJbCSN` — 20,920 B,
    sha256 `7056ff1716783cb9e3592466b60c23fed38572b9d0462b610454c7ee42df434d`
    (round 1: `4a49d558…`)

## What the patch was (Track F — source diff + deployed-ELF confirmation)

The patch is purely the C-1 class:

- every Init handler zeroes the full account before writing fields
- length checks became exact (`==`)
- `WithdrawInsurance` gained a `curSh <= totSh` require
- `CloseAccount` zeroes all bytes and writes a permanent `MAGIC_CLOSED`
  tombstone — immediate `0x44454144` ("DEAD") confirmed present in the deployed
  core ELF at file offset 0xa04
- `handleFill`, `handleRefreshLeg`, `settleAndRefreshLeg` are NOT in the diff

## Track verdicts

- **Track E** (`findings/track-e.md`) — fresh audit. Headline E-1 (C-1 possibly
  alive) was indeterminate from this track's data (no round-1 ELF / round-2
  source locally); resolved by Track F: **C-1 is patched.** Everything else
  clean: no sibling revive route pays out, insurance math floor-rounds
  pool-favorable, seeded state consistent, no anomalies across 13 portfolios.
  Notable leftover: the round-1 forged receipt still carries
  8,536,876,367,535,870,429 phantom shares — inert, bound to the dead market.
- **Track F** (`findings/track-f.md`) — round-1 leads re-validated.
  - Track B F1 (matcher Fill mode byte unchecked): **still present** in source
    and ELF, but **not wired to the bounty market** — the only matcher cfg's LP
    sits on a different DEX (`3dgsgDDB…`, market `8ghSDBqr…`, insurance 0) and
    the core dex-consistency check blocks Fills against `BhYaG2…`. Becomes a
    pot route only if a matcher LP is deployed on DEX `CedQK…` and takes a
    position on `BhYaG2…`; endgame then caps at ~17.5 USDC per liquidation
    (maxInsDrawBps 2500). **Watch item.**
  - Lead-001 / F4 (RefreshLeg forceHaircut): **still present**, byte-identical
    semantics, warmupSlots = 0 so exercisable as griefing against the house
    legs once price moves — but it can never reduce the 70 USDC principal and
    attacker capture is bounded below the amount stolen. Not a pot route.
- **Track D** — independent ELF byte-diff; still running when this was
  committed. Its working artifacts are under `evidence/round2/trackd/`
  (round-1 devnet ELF recovered: `core_old.so`); its findings file follows in a
  later commit.

## Evidence

`evidence/round2/trackf/` — live round-2 ELFs (core/matcher/oracle), round-1
devnet ELFs, disassemblies, upstream Lean sources, `program.diff`, dump/state
scripts. `evidence/round2/trackd/` — Track D's ELF pair + programdata dumps.

## Status

- Pot untouched at commit time; pot-watch re-armed on the round-2 market
  (insurance/shares changes, OI swings > 1,000,000, vault < insurance, or
  market missing).
- No live transaction was sent or signed by any track. All work was read-only
  RPC, source diffing, and disassembly.
