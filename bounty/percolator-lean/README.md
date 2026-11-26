# Percolator LIVE-market bug bounty — C-1 write-up & on-chain claim

Toly's percolator-lean bounty: *"Break the live market and keep the insurance fund… If you pull it out on-chain, it's yours."* (percolator-cli `README.md`). This repo documents the winning exploit (**C-1**), the full analysis, a reproducible PoC, the exact instruction sequence, the fix, and the confirmed on-chain claim.

## Result — bounty won

- **Net profit: +70.000000 USDC** (deposited 0.000001, withdrew 70.000001). The market's entire insurance pot was drained.
- **Claimer / signer wallet:** `J82YD5pPBcBqJThYif1DaCH2vLyU22RbWQDRLtz3v4DB`
- **Target (mainnet):** core `GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm` (sha256 `c6dc8329…`), DEX `6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH`, market `76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq`, vault `8MqHSrm6v6ccJVQunjnLqfBBWoAhVGrqUV4hdSH9F8i9`.

### Claim transactions (mainnet-beta)

| Step | Instructions | Signature / Solscan |
|---|---|---|
| A | SystemCreateAccount(952) + InitPortfolio(owner=signer) | [`2uv9ii…hS9Wt`](https://solscan.io/tx/2uv9iiQxS9Vt8nSvAqvxXke2jrZBMSy8J9vrZcXy4tcY2dWmXUe6KqEk4snkJuA9UmEE8tkB4WcgvV9nNEyhS9Wt) |
| TX1 | fund sink → CloseAccount(P, dest=sink) → transfer(signer→P, rent) → DepositInsurance(P, 1) | [`4Znsq4…9Gfs1`](https://solscan.io/tx/4Znsq4z7so5fcBot87ZBsZW8LmEppdjsCSFg1DWhUo2NqKaiX7b4FavFeUK37MDsDJ8QuHHDT6ZcRxNRpxs9Gfs1) |
| TX2 | WithdrawInsurance(P, burn = totalInsShares = 70000001) | [`44B4uc…3M9K`](https://solscan.io/tx/44B4ucXWSXYvVZwiVPvZN2aYxNwJNPDkqYo8Romsz3UipTTrNUqPAkewzTZP4ctRUq9p7tacmhRjKRyTJ3Zf7M9K) |

Portfolio account reused as the forged insurance receipt: `5atDktvTRPJuy92dYMUFXHdPP6kURLCmG2VQHSGV9wQw` (offset-80 = signer `owner[24:32]` = 8536876367605870429).

## The bug (C-1), in one paragraph

`handleCloseAccount` zeroes only the 4-byte magic and drains lamports — **every other byte survives**. `handleDepositInsurance`'s receipt-init branch (taken when `magic == 0`) writes every field **except `shares` (offset 80)**, then **adds** the minted amount to whatever bytes are already there. In a portfolio, offset 80 is inside the `owner` pubkey (`owner` = 56..88, so byte 80 = `owner[24:32]`). So: create a portfolio you own, close it (magic→0, bytes survive), revive it as an insurance receipt in the same transaction — the leftover owner bytes preload a `shares` balance ≈ 2^63 — then `WithdrawInsurance` the whole pot. `WithdrawInsurance` is **not** subject to the 25% `maxInsDrawBps` draw cap (that bounds only liquidation), so the live market config does not protect it.

The Lean proofs miss it because the verified `Spec.lean` has no byte layer or account lifecycle; the init branch silently assumes *"magic==0 ⇒ freshly system-zeroed,"* which is false for an account recycled through the program's own `closeAccount`. `Program.lean` itself carries no theorems.

## Fix (apply both)

1. **DepositInsurance init branch must set `shares = 0`** before `addU64At … shares minted`. Never `add` into a field that was never explicitly zeroed.
2. **CloseAccount must zero the whole account** (`data[0..dataLen]`), not just the magic, before draining lamports.

Defense in depth: require exact receipt size on init, and/or gate the `magic==0` init on a verifiably freshly-created (system-zeroed, correct-length) account.

## Runtime note (mainnet vs. the LiteSVM PoC)

`handleCloseAccount` reads three distinct accounts `(acc, authority, dest)`. If `dest` equals `authority`, the key appears twice in one instruction and Solana serializes the second as an 8-byte **duplicate stub**, which the Lean `AccountView` never resolves (see `reports/LEAD-001…`), so close fails `InvalidAccountData`. The working attack uses a **separate** lamport sink for `dest` — a second instance of the same dup-stub blind spot.

## Contents

- `reports/` — full C-1 write-up, consolidated report, and leads LEAD-001…004.
- `poc/poc-runner-main.rs` — LiteSVM PoC harness (reproduces C-1 against the mainnet-identical ELF).
- `poc/claim.py`, `poc/claim_bc.py` — the mainnet claim scripts (every broadcast gated on `simulateTransaction`; secrets are read from local files by path and are **not** included here).

*Read-only research + a single sanctioned claim into the owner's own wallet. No secrets are committed.*
