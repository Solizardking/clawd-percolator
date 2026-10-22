# clawd-percolator

Percolator perpetuals on **Clawd Chain** (test network, test cUSD collateral). Live at https://percolator.clawdchain.trade.

## Program IDs (Clawd Chain, BPF upgradeable)

| Program | ID | Source |
|---|---|---|
| Wrapper (perps market) | `6Grr3eyuJVME5gDbeT6g6Q3pojQGSuSDaEmHDM7QYhzx` | `programs/percolator-prog` (+ engine crate `programs/percolator`) |
| Insurance stake | `85rgQvJsqVq5gF9TPMtxmPQaoVMGK1wd6qx1FZqfXk5q` | `programs/percolator-stake` |
| Position NFT (Token-2022) | `GDaUkuhbErfBuL92oV93bWWHEVn9NF7pRNikSeq6nHLR` | `programs/percolator-nft` |

Manifest: `deploy/clawd-percolator.json` (the UI and API read IDs from it). `ids.txt` is used by `cli/`.
Program sources are forks of github.com/dcccrypto/percolator* (itself derived from aeyakovenko/percolator); upstream LICENSE files are kept in each folder.

## Architecture

- `deploy/hetzner/frontend/server.js` - Node server: SSR page, `/api/markets` (registry memo launches via RPC `getSignaturesForAddress`), `/api/position`, `/api/perp-markets`, `/api/markets/provision`, `/api/tx/<action>`.
- `deploy/hetzner/frontend/perc-api.js` - builds unsigned txs for the browser wallet (portfolio, deposit, withdraw, trade, stakeDeposit, stakeWithdraw, nftMint, nftBurn). A **house** wallet (`$PERCOLATOR_DATA/house.json`, never in git) is market admin, LP counterparty and the test cUSD mint authority. Market state is in `$PERCOLATOR_DATA/markets.json`.
- `deploy/hetzner/frontend/perc-core.js` - instruction encoders, PDAs, account layouts.
- `percolator-launch/` - React component + client JS (`public/percolator-ui.js`).
- `explorer/`, `extension/pow.js` - shared clawd chain assets.
- `cli/` - operator scripts (expect keys in `../.keys/`, which is gitignored).

**Price:** each market trades at a fixed `initial_price` (100_000_000 = 100.000000 cUSD) set at market creation; there is no oracle or keeper yet.

## Deploy

Programs (from inside the validator container, faucet keypair as payer/upgrade authority):
```
cargo build-sbf   # in each programs/* crate; pinned engine (see percolator-prog build.rs)
solana program deploy --url http://127.0.0.1:8080 --keypair /data/ledger/faucet-keypair.json \
  --program-id <program-keypair.json> target/deploy/<name>.so
```
See `deploy/hetzner/percolator-programs.sh`.

Frontend/API: copy `deploy/hetzner/env.example` to `deploy/hetzner/.env` on the host, then `deploy/hetzner/up.sh` (uses `compose.json` and `percolator-launch.Dockerfile`). Put `house.json` in the data volume.

Never commit keypairs, `.env`, ledgers or build outputs.
