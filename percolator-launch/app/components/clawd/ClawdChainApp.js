const React = require("react");
const { loadClawdCluster } = require("../../lib/clawd-cluster.js");
const h = React.createElement;

function WalletPanel() {
  return h("div", { className: "card", id: "wallet", "data-surface": "wallet" },
    h("h3", { style: { marginTop: 0 } }, "Browser wallet"),
    h("p", { className: "sub" }, "Test wallet stored in this browser. Use a dedicated test phrase; never import a wallet holding real funds."),
    h("div", { className: "kv" },
      h("div", null, "Address"), h("div", { className: "mono", id: "wallet-address" }, "…"),
      h("div", null, "Balance"), h("div", { id: "wallet-balance" }, "…")),
    h("div", { className: "actions" },
      h("button", { type: "button", className: "btn", id: "btn-faucet" }, "Get CLAWD from faucet"),
      h("button", { type: "button", className: "btn", id: "btn-new" }, "New wallet"),
      h("button", { type: "button", className: "btn", id: "btn-import" }, "Import phrase"),
      h("button", { type: "button", className: "btn", id: "btn-export" }, "Show phrase")));
}

function MarketLaunch(props) {
  const cluster = props.cluster;
  return h("section", { className: "card", id: "launch", "data-surface": "market-launch" },
    h("h2", { style: { marginTop: 0 } }, "Market launch"),
    h("form", { "data-action": "create-market" },
      h("label", { htmlFor: "clawd-market-name" }, "Market name"),
      h("input", { id: "clawd-market-name", name: "name", "aria-label": "Market name", maxLength: 32, placeholder: "clawd", required: true }),
      h("label", { htmlFor: "clawd-market-symbol" }, "Symbol (optional)"),
      h("input", { id: "clawd-market-symbol", name: "symbol", "aria-label": "Symbol", maxLength: 10, placeholder: "CLAWD" }),
      h("div", { className: "actions" }, h("button", { type: "submit", className: "btn hot" }, "Create market"))),
    h("p", { id: "notice" }),
    h("p", { className: "sub" }, "Records the launch on the clawd chain (fee 0.001 CLAWD + rent), then provisions a tradable Percolator market with an insurance pool and NFT registry. Wrapper program: ",
      h("span", { className: "mono", "data-program": "wrapper" }, cluster.programs.wrapper)));
}

function MarketList() {
  return h("section", { className: "card", id: "browse", "data-surface": "markets", style: { marginTop: 18 } },
    h("h2", { style: { marginTop: 0 } }, "Launched markets ", h("span", { className: "pill ok", id: "market-count" }, "…")),
    h("div", { style: { overflowX: "auto" } },
      h("table", null,
        h("thead", null, h("tr", null, h("th", null, "Market"), h("th", null, "Creator"), h("th", null, "Launched"), h("th", null, "Account"), h("th", null, "Transaction"), h("th", null, "Trading"))),
        h("tbody", { id: "markets" }, h("tr", null, h("td", { colSpan: 6, className: "sub" }, "Loading…"))))));
}

function btn(id, label, action, hot) { return h("button", { type: "button", className: hot ? "btn hot" : "btn", id, "data-action": action }, label); }
function TradePanel({ cluster }) {
  return h("section", { className: "card", id: "trade", "data-surface": "trade" },
    h("h3", { style: { marginTop: 0 } }, "Trade ", h("span", { className: "pill ok", id: "trade-status" }, "test network")),
    h("p", { className: "sub" }, "Trade against the clawd wrapper. Collateral is test cUSD; the house LP takes the other side at a fixed initial price. No live oracle is connected."),
    h("label", { htmlFor: "perp-market" }, "Market"),
    h("select", { id: "perp-market" }, h("option", { value: "" }, "Loading…")),
    h("div", { className: "kv", style: { marginTop: 12 } },
      h("div", null, "Mark"), h("div", { id: "pos-price" }, "–"),
      h("div", null, "cUSD"), h("div", { id: "pos-usd" }, "–"),
      h("div", null, "Margin"), h("div", { id: "pos-capital" }, "–"),
      h("div", null, "Position"), h("div", { id: "pos-size" }, "–")),
    h("div", { className: "actions" }, btn("btn-usd", "Get test cUSD", "usd"), btn("btn-open", "Open account", "portfolio")),
    h("label", { htmlFor: "amt-margin" }, "Margin amount (cUSD)"),
    h("input", { id: "amt-margin", defaultValue: "500", inputMode: "decimal" }),
    h("div", { className: "actions" }, btn("btn-deposit", "Deposit", "deposit"), btn("btn-withdraw", "Withdraw", "withdraw")),
    h("label", { htmlFor: "amt-size" }, "Size (contracts)"),
    h("input", { id: "amt-size", defaultValue: "1", inputMode: "decimal" }),
    h("div", { className: "actions" }, btn("btn-long", "Long", "trade", true), btn("btn-short", "Short", "trade")),
    h("p", { className: "sub mono", "data-program": "wrapper" }, cluster.programs.wrapper));
}
function InsuranceStake({ cluster }) {
  return h("section", { className: "card", id: "earn", "data-surface": "insurance-stake" },
    h("h3", { style: { marginTop: 0 } }, "Insurance stake"),
    h("p", { className: "sub" }, "Stake test cUSD into the market insurance pool and receive test LP tokens. No real-money yield or guaranteed coverage."),
    h("div", { className: "kv" }, h("div", null, "Staked LP"), h("div", { id: "pos-lp" }, "–")),
    h("label", { htmlFor: "amt-stake" }, "Amount (cUSD)"),
    h("input", { id: "amt-stake", defaultValue: "100", inputMode: "decimal" }),
    h("div", { className: "actions" }, btn("btn-stake", "Deposit", "deposit"), btn("btn-unstake", "Withdraw", "withdraw")),
    h("p", { className: "sub mono", "data-program": "stake" }, cluster.programs.stake));
}
function PositionNft({ cluster }) {
  return h("section", { className: "card", "data-surface": "position-nft" },
    h("h3", { style: { marginTop: 0 } }, "Position NFT"),
    h("p", { className: "sub" }, "Wrap your open position into a transferable Token-2022 NFT, or unwrap it back."),
    h("div", { className: "kv" }, h("div", null, "NFT"), h("div", { id: "pos-nft", className: "mono" }, "–")),
    h("div", { className: "actions" }, btn("btn-mint", "Mint", "mint"), btn("btn-burn", "Burn", "burn"), btn("btn-unwrap", "Unwrap", "unwrap")),
    h("p", { className: "sub mono", "data-program": "nft" }, cluster.programs.nft));
}
function ClawdChainApp(props) {
  const cluster = props && props.cluster ? props.cluster : loadClawdCluster();
  return h("main", { className: "wrap", "data-network": "clawd", "data-rpc": cluster.rpc, "data-wrapper": cluster.programs.wrapper },
    h("div", { className: "test-banner", role: "note" }, "CLAWD CHAIN TESTNET · NOT REAL MONEY · Break things. Report bugs. Don’t trust, verify."),
    h("header", { className: "hero" },
      h("span", { className: "pill" }, "CLAWD / PERCOLATOR"),
      h("h1", null, "Your market. Your test ground."),
      h("p", { className: "sub" }, "Launch and trade experimental perpetual markets on our own Clawd Chain. Test CLAWD pays network fees. Test cUSD is collateral. Neither represents real money."),
      h("nav", { className: "jump-nav", "aria-label": "Percolator sections" },
        h("a", { className: "btn hot", href: "#launch" }, "Launch a market"),
        h("a", { className: "btn", href: "#browse" }, "Browse markets"),
        h("a", { className: "btn", href: "#trade" }, "Trade"),
        h("a", { className: "btn", href: "#earn" }, "Insurance pools"),
        h("a", { className: "btn", href: "#wallet" }, "Test wallet")),
      h("p", { className: "sub" }, "Current build: fixed initial prices · house-managed market admin and LP · no live APY or volume claims · mainnet is not enabled.")),
    h("section", { className: "card", "aria-label": "How it works" },
      h("h2", null, "Three steps to your test market"),
      h("ol", null,
        h("li", null, "Create a dedicated test wallet and request faucet CLAWD."),
        h("li", null, "Name your market. Submit its registry transaction, then provision trading."),
        h("li", null, "Request test cUSD, open an account, and explore trades, insurance staking, and position NFTs.")),
      h("p", { className: "sub" }, "Provisioning depends on validator confirmation and server configuration. Insurance is a loss buffer, not a solvency guarantee.")),
    h("div", { className: "grid g2", style: { alignItems: "start" } }, h(MarketLaunch, { cluster }), h(WalletPanel)),
    h(MarketList),
    h("p", { id: "trade-notice", style: { minHeight: "1.4em", color: "var(--cy)", wordBreak: "break-all" } }),
    h("div", { className: "grid g4", style: { marginTop: 8, alignItems: "start" } }, h(TradePanel, { cluster }), h(InsuranceStake, { cluster }), h(PositionNft, { cluster })),
    h("footer", { className: "card", style: { marginTop: 24 } },
      h("h2", null, "Don’t trust, verify."),
      h("p", null, "Our wrapper, insurance stake, and position NFT addresses are shown above. Verify transactions in the Clawd explorer. Admin and upgrade authorities have not been burned."),
      h("a", { href: "https://github.com/Solizardking/clawd-percolator/issues/new" }, "Report a bug"), " · ",
      h("a", { href: "https://github.com/dcccrypto/percolator-launch" }, "Launcher foundation"), " · ",
      h("a", { href: "https://github.com/aeyakovenko/percolator" }, "Risk engine research")));
}

module.exports = { ClawdChainApp, MarketLaunch, MarketList, WalletPanel, TradePanel, InsuranceStake, PositionNft };
