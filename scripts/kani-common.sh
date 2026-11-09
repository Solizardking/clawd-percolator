#!/usr/bin/env bash
# Shared helpers for the kani-clawd proof profiles. Sourced, not executed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="$ROOT/kani-clawd"
TIMEOUT="${KANI_HARNESS_TIMEOUT:-600s}"
JOBS="${KANI_JOBS:-4}"
FAILED=()

# run_pass <features> <harness>...: every harness must verify.
run_pass() {
  local features="$1"; shift
  for h in "$@"; do
    echo "::group::PASS expected: $h"
    local start=$SECONDS
    if (cd "$CRATE" && cargo kani --tests ${features:+--features "$features"} \
          -Z unstable-options --harness-timeout "$TIMEOUT" --exact --harness "$h" --output-format terse); then
      echo "ok   $h ($((SECONDS - start))s)"
    else
      echo "FAIL $h ($((SECONDS - start))s)"; FAILED+=("$h")
    fi
    echo "::endgroup::"
  done
}

# run_fail <features> <harness>...: every harness must be refuted (vulnerable examples).
run_fail() {
  local features="$1"; shift
  for h in "$@"; do
    echo "::group::FAIL expected: $h"
    local start=$SECONDS
    if (cd "$CRATE" && cargo kani --tests ${features:+--features "$features"} \
          -Z unstable-options --harness-timeout "$TIMEOUT" --exact --harness "$h" --output-format terse); then
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
