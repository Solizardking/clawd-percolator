# CLAWD Percolator integration

This build uses **Clawd Chain testnet**, not Solana public devnet or mainnet.
All collateral is test cUSD. Test CLAWD pays fees. There is no real-money mode.
The supplied ClawdChainApp.js, clawd-cluster.js and clawd-ids.js matched the repository baseline and were used as the integration starting point.

## Foundations inspected

- dcccrypto/percolator-launch: `9cc8d8228574ec8608fd7c30e6ccc16793f87f32`. Reference for launch, browse, trade and insurance navigation. No Privy iframe or mock APY/TVL copied.
- aeyakovenko/percolator: `37cb14ee1436eaf3b7d97e1901b0dc1bf38f5311`. Research risk engine; its README explicitly says not audited or production ready.
- Existing CLAWD sources remain in programs/percolator, programs/percolator-prog, programs/percolator-stake and programs/percolator-nft, with upstream licenses preserved. No unreviewed engine replacement or deployed-program change is included.

The React interface retains the existing browser wallet, registry/provisioning API, trade actions, insurance staking and NFT actions. Program IDs come from deploy/clawd-percolator.json. Public browser RPC and private server RPC are resolved separately. The application only accepts its named HTTPS Clawd gateway or local/container validator endpoints (server only). This endpoint restriction is configuration protection, not independent attestation of the remote chain's genesis or deployed binaries.

## Fixed bugs

1. Browser RPC inherited CLAWD_RPC=http://validator:8080. The browser now receives the public gateway; the server keeps its internal URL.
2. RPC blacklist accepted arbitrary providers and malformed URLs. Parsed URL allowlists now reject unknown hosts, credentials and unsafe protocols before server startup.
3. /api/markets failures looked like an empty successful response. The client now checks HTTP/error status and shows unavailable, not zero markets. Error table colspan corrected to six.

## Open limitations / bug follow-up

- Current prices are fixed initial values; live oracle validation and a keeper are absent.
- The house is market admin, liquidity counterparty and cUSD mint authority. Programs are upgradeable. Do not advertise burned admin keys or immutability.
- No verified creator-fee payout, APY, volume, leaderboard or 60-second deployment guarantee is implemented.
- Existing wrapper build.rs expects a standalone sibling engine Git HEAD matching ci/deployed-refs.env. In this vendored monorepo it resolves the parent repository HEAD, so the current provenance check needs a reviewed content-pin migration before reproducible SBF deployment. Do not bypass the guard for deployment.
- A browser wallet stores its phrase in localStorage. Only dedicated disposable test wallets should be used.
- On-chain execution, minting and deployment require a configured test validator and house wallet; they are not validated by the local UI tests.

## Local verification

```
cd deploy/hetzner/frontend
npm ci --ignore-scripts
cd ../../..
NODE_PATH="$PWD/deploy/hetzner/frontend/node_modules" node --test test/*.test.js
PORT=8080 node deploy/hetzner/frontend/server.js
```

With no house wallet, transaction actions remain unconfigured. Existing deployment instructions remain in README.md. No deployment or admin-authority change is part of this integration.
