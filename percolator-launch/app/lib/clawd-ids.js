/**
 * Client-safe clawd cluster constants. No filesystem access.
 * test/launch-clawd.test.js asserts these equal deploy/clawd-percolator.json.
 */

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
function validateClawdRpc(value, internal = false) {
  let url;
  try { url = new URL(value); } catch { throw new Error("Invalid CLAWD RPC URL"); }
  const publicGateway = url.hostname === "rpc.clawdchain.trade" && url.protocol === "https:" && !url.port;
  const localValidator = internal && ["validator", "localhost", "127.0.0.1", "[::1]"].includes(url.hostname)
    && ["http:", "https:"].includes(url.protocol);
  if ((!publicGateway && !localValidator) || url.username || url.password || url.hash) {
    throw new Error("CLAWD RPC must use the Clawd test gateway or an internal validator");
  }
  return url.href;
}

function resolveClawdRpc(env = process.env) {
  // Internal container addresses must never be sent to the browser.
  return validateClawdRpc(env.CLAWD_PUBLIC_RPC || env.NEXT_PUBLIC_CLAWD_RPC || CLAWD_RPC);
}

function resolveClawdServerRpc(env = process.env) {
  return validateClawdRpc(env.CLAWD_RPC || "http://validator:8080", true);
}

module.exports = {
  validateClawdRpc,
  resolveClawdServerRpc,
  CLAWD_RPC,
  CLAWD_PROGRAMS,
  isClawdDeployment,
  resolveClawdRpc,
};
