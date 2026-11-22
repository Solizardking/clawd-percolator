# percolator-stake

Insurance stake program for the Clawd Percolator fork: LP staking against
the insurance fund. The LP math (deposit/withdraw conservation,
anti-inflation, fairness) is formally verified in the dedicated proof crate
at `kani-proofs/` (54 harnesses, incl. 2 inductive). Fork-wide context and
the full work ledger: [`../../WORK.md`](../../WORK.md).
