#!/usr/bin/env bash
# Shared helpers for the kani-clawd proof profiles. Sourced, not executed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="$ROOT/kani-clawd"
TIMEOUT="${KANI_HARNESS_TIMEOUT:-600s}"
JOBS="${KANI_JOBS:-4}"
FAILED=()
# Always enable solana-agent when using --tests: several [[test]] targets declare
# required-features = ["solana-agent"], and cargo-kani errors (instead of skipping)
# if those features are missing.
BASE_FEATURES="${KANI_BASE_FEATURES:-solana-agent}"

merge_features() {
  local extra="${1:-}"
  if [[ -z "$extra" ]]; then
    echo "$BASE_FEATURES"
  elif [[ "$extra" == *"$BASE_FEATURES"* ]]; then
    echo "$extra"
  else
    echo "$BASE_FEATURES,$extra"
  fi
}

# Classify harness: library proofs live under risk:: / math:: / ...; examples/flows are bare names.
is_lib_harness() {
  [[ "$1" == *::* ]]
}

# run_pass <extra-features> <harness>...: every harness must verify.
run_pass() {
  local extra="$1"; shift
  local features
  features="$(merge_features "$extra")"
  for h in "$@"; do
    echo "::group::PASS expected: $h"
    local start=$SECONDS
    local args=(--features "$features" -Z unstable-options --harness-timeout "$TIMEOUT" --exact --harness "$h" --output-format terse -j "$JOBS")
    if is_lib_harness "$h"; then
      args=(--lib "${args[@]}")
    else
      args=(--tests "${args[@]}")
    fi
    if (cd "$CRATE" && cargo kani "${args[@]}"); then
      echo "ok   $h ($((SECONDS - start))s)"
    else
      echo "FAIL $h ($((SECONDS - start))s)"; FAILED+=("$h")
    fi
    echo "::endgroup::"
  done
}

# run_fail <extra-features> <harness>...: every harness must be refuted (vulnerable examples).
run_fail() {
  local extra="$1"; shift
  local features
  features="$(merge_features "$extra")"
  for h in "$@"; do
    echo "::group::FAIL expected: $h"
    local start=$SECONDS
    local args=(--features "$features" -Z unstable-options --harness-timeout "$TIMEOUT" --exact --harness "$h" --output-format terse -j "$JOBS")
    if is_lib_harness "$h"; then
      args=(--lib "${args[@]}")
    else
      args=(--tests "${args[@]}")
    fi
    if (cd "$CRATE" && cargo kani "${args[@]}"); then
      echo "UNEXPECTED PASS $h ($((SECONDS - start))s)"; FAILED+=("$h")
    else
      echo "ok   $h refuted as expected ($((SECONDS - start))s)"
    fi
    echo "::endgroup::"
  done
}

finish() {
  if ((${#FAILED[@]})); then
    echo "Profile failed: ${FAILED[*]}"; exit 1
  fi
  echo "Profile passed."
}
