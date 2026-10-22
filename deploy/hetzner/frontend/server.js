const http = require("http");
const path = require("path");
const fs = require("fs");
const Module = require("module");

const frontendModules = path.join(__dirname, "node_modules");
process.env.NODE_PATH = [frontendModules, process.env.NODE_PATH].filter(Boolean).join(path.delimiter);
Module._initPaths();

function firstExisting(candidates, label) {
  for (const candidate of candidates) if (fs.existsSync(candidate)) return candidate;
  throw new Error(label + " not found");
}
const componentPath = firstExisting([
  path.join(__dirname, "../app/components/clawd/ClawdChainApp.js"),
  path.join(__dirname, "../../../percolator-launch/app/components/clawd/ClawdChainApp.js"),
], "ClawdChainApp.js");
const publicDir = firstExisting([path.join(__dirname, "../public"), path.join(__dirname, "../../../percolator-launch/public")], "public dir");
const sharedDir = firstExisting([path.join(__dirname, "../shared"), path.join(__dirname, "../../../explorer/public")], "shared assets");
const powPath = firstExisting([path.join(__dirname, "../shared/pow.js"), path.join(__dirname, "../../../extension/pow.js")], "pow.js");

const React = require("react");
const { renderToStaticMarkup } = require("react-dom/server");
const { ClawdChainApp } = require(componentPath);
const P = require(path.join(publicDir, "percolator.js"));

const port = Number(process.env.PORT || 8080);
const HIDDEN = new Set(String(process.env.PERCOLATOR_HIDDEN_LAUNCHES || "").split(",").map((x) => x.trim()).filter(Boolean));
const cluster = require(path.join(path.dirname(componentPath), "../../lib/clawd-cluster.js")).loadClawdCluster();
let trading = null;
try {
  trading = require("./perc-api.js").init({ rpc: process.env.CLAWD_RPC || "http://validator:8080", dataDir: process.env.PERCOLATOR_DATA || "/data",
    programs: cluster.programs, registry: P.REGISTRY, parseMemo: P.parseMemo, hidden: HIDDEN });
} catch (e) { console.error("trading disabled:", e.message); }
const VALIDATOR = process.env.CLAWD_RPC || "http://validator:8080";
const EXPLORER = process.env.CLAWD_EXPLORER || "https://explorer.clawdchain.trade";
const FAUCET = process.env.CLAWD_FAUCET || "https://faucet.clawdchain.trade";

const NAV = '<nav><a class="brand" href="https://clawdchain.trade/"><img src="/logo.svg" width="34" alt=""> clawd</a><span class="sp"></span>'
  + '<a href="https://clawdchain.trade/">Home</a><a href="https://explorer.clawdchain.trade/explorer">Explorer</a>'
  + '<a class="btn hot" href="https://launch.clawdchain.trade/">Launchpad</a><a class="on" href="https://percolator.clawdchain.trade/">Percolator</a>'
  + '<a href="https://faucet.clawdchain.trade">Faucet</a><a href="https://shield.clawdchain.trade">Shield</a><a href="https://clawdchain.trade/far">Far</a>'
  + '<a href="https://clawdchain.trade/readme">Wallet</a><a href="https://payments.clawdchain.trade">Payments</a><a href="https://mint.clawdchain.trade">Mint</a>'
  + '<a href="https://api.clawdchain.trade">API</a><a href="https://github.com/Solizardking/clawd-chain">GitHub</a></nav>';
const PAGE_CSS = 'label{display:block;color:var(--dim);font-size:12px;font-weight:700;letter-spacing:1px;text-transform:uppercase;margin:12px 0 6px}'
  + '.actions{display:flex;gap:10px;flex-wrap:wrap;margin-top:14px}#notice{min-height:1.4em;margin:12px 0 0;color:var(--cy);word-break:break-all}'
  + '.kv{grid-template-columns:90px 1fr}.btn[disabled]{opacity:.45;cursor:not-allowed}table{width:100%;border-collapse:collapse}th,td{text-align:left;padding:8px 10px;border-bottom:1px solid var(--line);white-space:nowrap}th{color:var(--dim);font-size:12px}'
  + 'select{background:#0b0a15;border:1px solid var(--line);border-radius:12px;color:var(--txt);padding:12px 14px;font:inherit;width:100%}'
  + '@media(max-width:860px){.g2{grid-template-columns:1fr}}';

function page() {
  const app = renderToStaticMarkup(React.createElement(ClawdChainApp))
    .replace('<main ', `<main data-explorer="${EXPLORER}" data-faucet="${FAUCET}" `);
  return '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">'
    + '<title>clawd percolator</title><link rel="icon" href="/logo.svg"><link rel="stylesheet" href="/style.css"><style>' + PAGE_CSS + '</style></head><body>'
    + NAV + app
    + '<script src="/pow.js"></script><script src="/percolator.js"></script><script src="/percolator-ui.js"></script></body></html>';
}

const STATIC = {
  "/style.css": [path.join(sharedDir, "style.css"), "text/css"],
  "/logo.svg": [path.join(sharedDir, "logo.svg"), "image/svg+xml"],
  "/pow.js": [powPath, "text/javascript"],
  "/percolator.js": [path.join(publicDir, "percolator.js"), "text/javascript"],
  "/percolator-ui.js": [path.join(publicDir, "percolator-ui.js"), "text/javascript"],
};

function isAddr(a) { try { return typeof a === "string" && P.b58decode(a).length === 32; } catch (e) { return false; } }
let cache = { exp: 0, body: null };
async function listMarkets() {
  if (cache.body && Date.now() < cache.exp) return cache.body;
  const r = await fetch(VALIDATOR, { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getSignaturesForAddress", params: [P.REGISTRY, { limit: 1000, commitment: "confirmed" }] }),
    signal: AbortSignal.timeout(5000) });
  const j = await r.json();
  if (j.error) throw new Error(j.error.message);
  const markets = [];
  for (const s of j.result || []) {
    if (s.err || HIDDEN.has(s.signature)) continue;
    const m = P.parseMemo(s.memo);
    if (!m || !isAddr(m.creator) || !isAddr(m.market)) continue;
    markets.push({ name: m.name, symbol: m.symbol, creator: m.creator, market: m.market, signature: s.signature, slot: s.slot, time: s.blockTime,
      tx: EXPLORER + "/tx/" + s.signature, account: EXPLORER + "/address/" + m.market,
      tradable: !!(trading && trading.tradable(s.signature)), perp: trading && trading.tradable(s.signature) ? trading.tradable(s.signature).market : null });
  }
  // Validator ledger history is pruned, so memo launches can drop out of
  // getSignaturesForAddress. Keep markets that have live trading state listed.
  if (trading) {
    const seen = new Set(markets.map((x) => x.signature));
    for (const t of trading.list()) {
      if (!t.launchSig || seen.has(t.launchSig) || HIDDEN.has(t.launchSig)) continue;
      markets.push({ name: t.name, symbol: t.symbol || "", creator: t.creator || null, market: t.launchAccount || t.market, signature: t.launchSig, slot: null, time: t.created || null,
        tx: EXPLORER + "/tx/" + t.launchSig, account: EXPLORER + "/address/" + (t.launchAccount || t.market), tradable: true, perp: t.market });
    }
  }
  cache = { exp: Date.now() + 3000, body: JSON.stringify({ registry: P.REGISTRY, count: markets.length, trading: !!(trading && trading.enabled()), programs: cluster.programs, markets }) };
  return cache.body;
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url || "/", "http://x");
  if (url.pathname === "/health") { res.writeHead(200); return res.end("ok"); }
  if (url.pathname === "/api/markets") {
    if (req.method === "OPTIONS") { res.writeHead(204, { "access-control-allow-origin": "*", "access-control-allow-methods": "GET" }); return res.end(); }
    return listMarkets().then(
      (b) => { res.writeHead(200, { "content-type": "application/json", "cache-control": "no-cache", "access-control-allow-origin": "*" }); res.end(b); },
      () => { res.writeHead(502, { "content-type": "application/json", "access-control-allow-origin": "*" }); res.end('{"markets":[],"error":"rpc unavailable"}'); });
  }
  if (url.pathname.startsWith("/api/") && trading) {
    const json = (code, o) => { res.writeHead(code, { "content-type": "application/json", "cache-control": "no-store" }); res.end(JSON.stringify(o)); };
    const fail = (e) => json(e.code && e.code >= 400 && e.code < 600 ? e.code : 500, { error: e.code ? e.message : "Transaction build failed: " + String(e.message || e).slice(0, 200) });
    if (url.pathname === "/api/position" && req.method === "GET") {
      return trading.position({ market: url.searchParams.get("market"), owner: url.searchParams.get("owner") }).then((o) => json(200, o), fail);
    }
    if (url.pathname === "/api/perp-markets" && req.method === "GET") return json(200, { markets: trading.markets() });
    const route = url.pathname === "/api/markets/provision" ? "provision" : (url.pathname.match(/^\/api\/tx\/([a-zA-Z]+)$/) || [])[1];
    if (route && req.method === "POST") {
      let b = "";
      req.on("data", (d) => { b += d; if (b.length > 4096) req.destroy(); });
      req.on("end", () => {
        let body; try { body = JSON.parse(b || "{}"); } catch (e) { return json(400, { error: "Bad JSON" }); }
        const fn = route === "provision" ? trading.provision : trading.actions[route];
        if (!fn) return json(404, { error: "Unknown action" });
        Promise.resolve().then(() => fn(body)).then((o) => { cache.exp = 0; json(200, o); }, fail);
      });
      return;
    }
  }
  const s = STATIC[url.pathname];
  if (s) {
    return fs.readFile(s[0], (err, buf) => {
      if (err) { res.writeHead(404, { "content-type": "text/plain" }); return res.end("not found"); }
      res.writeHead(200, { "content-type": s[1], "cache-control": "public, max-age=60" }); res.end(buf);
    });
  }
  if (url.pathname !== "/" && url.pathname !== "/index.html") {
    res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    return res.end("not found");
  }
  res.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-cache" });
  res.end(page());
});

server.listen(port, "0.0.0.0");
