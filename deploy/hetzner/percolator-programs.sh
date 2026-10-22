#!/bin/bash
# Deploy percolator-prog, percolator-stake, and percolator-nft to the Clawd
# Hetzner gateway. Mirrors clawd-shield-main/scripts/deploy.sh. Never resets
# the ledger: a live genesis.bin stays where it is.
set -euo pipefail
if [ "${1:-}" = "--reset" ] || [ -n "${LEDGER_RESET_TOKEN:-}" ]; then
  echo "refusing ledger reset" >&2
  exit 1
fi
RPC="${RPC:-http://95.216.154.157:8080}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MANIFEST="$ROOT/deploy/clawd-percolator.json"

deploy_one() {
  local name="$1"
  local id="$2"
  local so=""
  local kp=""
  local candidate
  for candidate in \
    "$ROOT/$name/target/deploy/${name//-/_}.so" \
    "$ROOT/$name/target/deploy/$name.so" \
    "$ROOT/.programs/$name.so"
  do
    if [ -f "$candidate" ]; then
      so="$candidate"
      break
    fi
  done
  for candidate in \
    "$ROOT/.keys/${name}-keypair.json" \
    "$ROOT/$name/target/deploy/${name}-keypair.json"
  do
    if [ -f "$candidate" ]; then
      kp="$candidate"
      break
    fi
  done
  if [ -z "$so" ] || [ -z "$kp" ]; then
    echo "missing bytecode or keypair for $name (program id $id) on $RPC" >&2
    exit 1
  fi
  local bufdir="$ROOT/.keys"
  mkdir -p "$bufdir"
  local bufkey="$bufdir/${name}-buffer.json"
  [ -f "$bufkey" ] || solana-keygen new --no-bip39-passphrase -s -o "$bufkey" >/dev/null
  local buf
  buf="$(solana-keygen pubkey "$bufkey")"
  local i
  for i in $(seq 1 15); do
    solana program write-buffer "$so" --buffer "$bufkey" -u "$RPC" -k "$kp" --use-rpc >/tmp/wb.log 2>&1 || true
    solana program dump "$buf" /tmp/buf.so -u "$RPC" >/dev/null 2>&1 || true
    if cmp -s <(head -c "$(wc -c < "$so")" /tmp/buf.so) "$so"; then
      echo "buffer ok after $i"
      break
    fi
    echo "retry $i: $(tail -1 /tmp/wb.log)"
    sleep 8
  done
  solana program deploy --buffer "$buf" --program-id "$id" -u "$RPC" -k "$kp" --upgrade-authority "$kp"
  rm -f "$bufkey"
}

# Names and ids come from the clawd manifest: percolator-prog, percolator-stake, percolator-nft.
eval "$(node -e '
const fs = require("fs");
const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const order = [
  ["percolator-prog", "wrapper"],
  ["percolator-stake", "stake"],
  ["percolator-nft", "nft"],
];
for (const [name, key] of order) {
  if (m.binaries[key] !== name) {
    console.error("manifest binary mismatch for " + name);
    process.exit(1);
  }
  const id = m.programs[key];
  if (!id) {
    console.error("manifest missing programs." + key);
    process.exit(1);
  }
  process.stdout.write("deploy_one " + name + " " + id + "\n");
}
' "$MANIFEST")"
