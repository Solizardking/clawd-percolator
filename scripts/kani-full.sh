#!/usr/bin/env bash
# Full profile: every library proof, every Percolator engine proof, all seven
# fail/fix example pairs and the perps lifecycle flow.
source "$(dirname "$0")/kani-common.sh"
FEATURES="solana-agent,solana-account-info"

# All library harnesses (risk, math, token, staking, bounds, generators, agent, account_info).
echo "::group::library proofs"
start=$SECONDS
if (cd "$CRATE" && cargo kani --lib --features "$FEATURES" \
      -Z unstable-options --harness-timeout "$TIMEOUT" -j "$JOBS" --output-format terse); then
  echo "ok   library proofs ($((SECONDS - start))s)"
else
  echo "FAIL library proofs"; FAILED+=("library")
fi
echo "::endgroup::"

run_pass "" \
  percolator_backing_fee_charge_conserves \
  percolator_earnings_withdraw_conserves \
  percolator_fee_split_conserves \
  percolator_floor_div_signed_conservative \
  percolator_mul_div_matches_exact \
  percolator_unattributed_loss_lock \
  percolator_effective_pnl_matches_engine_mul_div \
  flow_perps_lifecycle \
  flow_loss_writeoff_haircut

run_pass "$FEATURES" ex01_fixed ex02_fixed ex03_fixed ex04_fixed ex05_fixed ex06_fixed ex07_fixed
run_fail "$FEATURES" ex01_vulnerable ex02_vulnerable ex03_vulnerable ex04_vulnerable \
  ex05_vulnerable ex06_vulnerable ex07_vulnerable
finish
