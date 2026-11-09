#!/usr/bin/env bash
# Agent flow profile: end-to-end agent CPI flow plus agent-layer proofs, timed.
source "$(dirname "$0")/kani-common.sh"
run_pass "solana-agent" \
  flow_agent_cpi \
  agent::proofs::agent_cpi_macros_compose \
  agent::proofs::agent_oracle_consensus_sound \
  agent::proofs::agent_fsm_guard_only_legal_edges
finish
