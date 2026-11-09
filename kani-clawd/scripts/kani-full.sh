#!/usr/bin/env bash
# Crate-local entrypoint → repo scripts/kani-full.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$ROOT/scripts/kani-full.sh" "$@"
