#!/usr/bin/env bash
# Smoke profile: fast core risk proofs + Percolator engine bridge + one fail/fix pair.
source "$(dirname "$0")/kani-common.sh"

run_pass "" \
  risk::proofs::risk_haircut_at_most_one \
  risk::proofs::risk_haircut_never_exceeds_residual \
  risk::proofs::risk_fee_sweep_conserves_value \
  risk::proofs::risk_loss_writeoff_conserves \
  risk::proofs::risk_warmup_slope_bounds \
  percolator_backing_fee_charge_conserves \
  percolator_earnings_withdraw_conserves \
  ex01_fixed
run_fail "" ex01_vulnerable
finish
