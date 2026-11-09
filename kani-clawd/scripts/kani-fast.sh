#!/usr/bin/env bash
# Crate-local entrypoint → repo scripts/kani-smoke.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$ROOT/scripts/kani-smoke.sh" "$@"
