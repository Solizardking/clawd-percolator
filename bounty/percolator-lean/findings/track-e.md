# Track E — Round 2 fresh audit (non-C-1 routes + seeded-state anomalies)

*Reconstructed verbatim in substance from the Track E agent's final report (2026-11-25 ~21:56 EST). The agent's own write to this path failed twice on the home-volume EDQUOT fault; this file was committed by the coordinating agent from that report. Analysis + local reasoning only — no live transaction was sent or signed; all chain interaction was read-only RPC.*

## E-1 (best lead) — CRITICAL if unpatched, INDETERMINATE at time of writing: C-1 may still be alive in the round-2 ELF

- The round-2 ELF is exactly the same size as round 1 (73,224 B; fetched read-only from programdata `2nXzb69CFjK2DYXU951YcgoNvY3m1JvV2eC4pEgKCtBK`, sha256 `5f9e564b0db3802033dbe3d2c945a4075eed33f3edf7839474c3e7ffd6996f3b` verified firsthand). Same size fits a targeted `shares = 0` init fix — but equally fits a minimal unrelated change that leaves the bug intact.
- The round-1 ELF exists nowhere in the workspace, so no byte diff was possible from this track; round-2 source was not available locally to this track. *(Resolved by Track F: the patch IS the C-1 fix class — see track-f.md and ROUND2.md.)*
- Attack preconditions all confirmed live: portfolio creation on the round-2 DEX `CedQKVmmHuta5QcqRNkA96smV33LYtQBHo9wc1YcmFq3` is permissionless and free (createFee 0, nextPortfolioId 5), a fresh portfolio meets all CloseAccount preconditions immediately, and the pot was re-seeded identically via the shares path (insurance 70,000,000 = totalInsShares 70,000,000, all held by admin receipt `HRwLbjSzNMJipYrjafBUPgbtmx72tCF6R5UoGQyPAJZ9`).
- Timing shield: WithdrawInsurance requires OI/rawBasis == 0; the house pair's 6,674,572/6,674,572 currently blocks any forged-share withdraw. The window opens when the house pair fully unwinds while insurance remains.
- Decider (not run by this track): the round-1 harness `poc-runner-main.rs::c1_close_revive` against the round-2 ELF in LiteSVM. *(The coordinating agent did not execute this step.)*

PoC status: suspected-open at time of writing, unconfirmed. Superseded by Track F's source diff + ELF tombstone confirmation.

## E-2 — C-1 siblings (confirmed-by-reading): no second route pays out

- InitPortfolio revive is clean (init's omitted fields are exactly CloseAccount's zero preconditions).
- Phantom market via CreateMarket on a recycled portfolio aliases real values (totalInsShares/OI ≈ 2^63 from pubkey fragments, rawBasisShort ≈ 2^62 from entryK) but WithdrawInsurance and Resolve both demand OI/rawBasis == 0 and every reduction path is bounded by real legs — phantom OI can never be zeroed, so it can never pay out.
- Phantom DEX totals are self-scoped (fresh empty vault). Track A F3's verdict stands.

## E-3 — Insurance math (confirmed-by-reading): no over-credit

- Withdraw payout floor-rounds (pool-favorable).
- LEAD-002/C-2 par-mint stays dead — draws are capped at 25% (maxInsDrawBps 2500) so insurance can never reach exactly 0, and the house holds all 70M shares.
- Lead 4 dead (pool non-empty). tradeFeeBps = 0 kills the fee-accrual bootstrap.
- `totalSurplus` has no payout instruction anywhere — stranded, not extractable.

## E-4 — Verdict changes from live state

- Track B F1 is NOT live on round 2: the only matcher cfg (`3vgX57bVnFGNV3ytcrm3ex1zHZnkUzh7tGdNCM2eh8rz`) binds LP portfolio `3XsXCzHZwbt4cKGwsudNK85Wo4NkJumvjfFgTJdStxeu` on market `8ghSDBqrPMj9u1x8F9jZiULqnAsx9ZhQhWDWRskWyc7x` (insurance 0); the round-2 house portfolios (`BTD2y4hoWsbt6ZSkf2QAm3hsVzZu9jom8Wwzvq8Dje96` short / `HamaLhsCpnS6RA3zGESdU1uCk9xuwRML32ko4HC5sr9a` long, plain keypair owners) appear nowhere in it.
- LEAD-001/F4 has no target: both round-2 legs show posPnl = 0 and entryK exactly at market K (K_ZERO ± 818,307).
- The seeded pair is clean and symmetric (50,000,000 capital each, MM 23,749,999 each); DEX totals consistent (vault 170M = capital 100M + insurance 70M); no anomalies across all 13 portfolios.

## E-5 — Inert round-1 leftovers

- The forged C-1 receipt `5atDktvTRPJuy92dYMUFXHdPP6kURLCmG2VQHSGV9wQw` still exists with shares @80 = 8,536,876,367,535,870,429 still credited (the round-1 claim burned only ~70M of the forged ~8.537e18) — inert, bound to the dead round-1 market.
- Admin's orphaned 70M-share receipt `4pxsDAv31curT4YFUaYRFhFyRSnGY2yrAw4grtc2w5F7` likewise inert.
- A zeroed 352 B adapter-owned account (`JDuvXsuTmECkheGmXJZeK5CUgYpUBKe1sc81TuVT8Quh`) is an uninitialized cfg slot — inert, since a rogue cfg's crank PDA wouldn't match the market's oracleAuth `AY9VrAbe4Tqn2DVfVnuEiBvM1zugP9mmTtMfXSNbUTbF` (round-2 adapter cfg is `4P2D6KUS7AcpfDniZrFJvNbw55Mh3qxo95pMW1CE5KZL`).
