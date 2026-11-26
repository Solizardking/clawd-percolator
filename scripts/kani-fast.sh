#!/usr/bin/env bash
# Alias for the smoke profile (brief name: kani-fast.sh).
exec "$(dirname "$0")/kani-smoke.sh" "$@"
