/**
 * Node loader for deploy/clawd-percolator.json. The Hetzner UI reads this
 * file so the served program ids cannot drift from the deploy manifest.
 */
const fs = require("fs");
const path = require("path");
const { resolveClawdRpc } = require("./clawd-ids.js");

function findManifest() {
  let dir = __dirname;
  for (let i = 0; i < 8; i++) {
    const candidate = path.join(dir, "deploy", "clawd-percolator.json");
    if (fs.existsSync(candidate)) return candidate;
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error("deploy/clawd-percolator.json not found");
}

function loadClawdCluster(env = process.env) {
  const manifest = JSON.parse(fs.readFileSync(findManifest(), "utf8"));
  return {
    network: manifest.network,
    rpc: resolveClawdRpc(env),
    programs: {
      wrapper: manifest.programs.wrapper,
      stake: manifest.programs.stake,
      nft: manifest.programs.nft,
    },
  };
}

module.exports = { loadClawdCluster };
