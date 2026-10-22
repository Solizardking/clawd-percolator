/**
 * Client-safe clawd cluster constants. No filesystem access.
 * test/launch-clawd.test.js asserts these equal deploy/clawd-percolator.json.
 */

const SOLANA_OR_HELIUS = /api\.devnet\.solana\.com|api\.mainnet-beta\.solana\.com|helius-rpc\.com/;

const CLAWD_RPC = "https://rpc.clawdchain.trade";

const CLAWD_PROGRAMS = {
  wrapper: "6Grr3eyuJVME5gDbeT6g6Q3pojQGSuSDaEmHDM7QYhzx",
  stake: "85rgQvJsqVq5gF9TPMtxmPQaoVMGK1wd6qx1FZqfXk5q",
  nft: "GDaUkuhbErfBuL92oV93bWWHEVn9NF7pRNikSeq6nHLR",
};

/**
 * True only for an explicit clawd deployment. Unset Next stays on its
 * existing mainnet default.
 * @param {NodeJS.ProcessEnv} [env]
 */
function isClawdDeployment(env = process.env) {
  const published = String(env.NEXT_PUBLIC_DEFAULT_NETWORK || "").trim().toLowerCase();
  if (published === "clawd") return true;
  if (published === "mainnet" || published === "devnet" || published === "testnet" || published === "mainnet-beta") {
    return false;
  }
  return String(env.NETWORK || "").trim().toLowerCase() === "clawd";
}

/**
 * Browser-facing clawd gateway RPC (rendered into data-rpc). CLAWD_PUBLIC_RPC
 * wins so container-internal CLAWD_RPC values (http://validator:8080) never
 * reach the page. Overrides that point at public Solana or Helius are
 * rejected. Otherwise the manifest gateway is used.
 * @param {NodeJS.ProcessEnv} [env]
 */
function resolveClawdRpc(env = process.env) {
  const override = env.CLAWD_PUBLIC_RPC || env.NEXT_PUBLIC_CLAWD_RPC || env.CLAWD_RPC;
  if (override) {
    if (SOLANA_OR_HELIUS.test(override)) {
      throw new Error("CLAWD_RPC must not target a Solana public or Helius endpoint");
    }
    return override;
  }
  return CLAWD_RPC;
}

module.exports = {
  CLAWD_RPC,
  CLAWD_PROGRAMS,
  isClawdDeployment,
  resolveClawdRpc,
};
