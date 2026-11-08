const test = require('node:test');
const assert = require('node:assert/strict');
const { resolveClawdRpc, resolveClawdServerRpc, CLAWD_PROGRAMS } = require('../percolator-launch/app/lib/clawd-ids');
const manifest = require('../deploy/clawd-percolator.json');
test('browser uses the public gateway even with an internal server RPC', () => {
  assert.equal(resolveClawdRpc({ CLAWD_RPC: 'http://validator:8080' }), 'https://rpc.clawdchain.trade/');
  assert.equal(resolveClawdServerRpc({ CLAWD_RPC: 'http://validator:8080' }), 'http://validator:8080/');
  assert.deepEqual(CLAWD_PROGRAMS, manifest.programs);
});
test('reject external, malformed and credential-bearing RPC overrides', () => {
  for (const url of ['https://api.mainnet-beta.solana.com', 'https://api.devnet.solana.com', 'https://mainnet.helius-rpc.com', 'https://rpc.clawdchain.trade.evil.example', 'https://rpc.clawdchain.trade@evil.example', 'javascript:alert(1)', 'https://user:pass@rpc.clawdchain.trade', 'http://rpc.clawdchain.trade']) {
    assert.throws(() => resolveClawdRpc({ CLAWD_PUBLIC_RPC: url }));
    assert.throws(() => resolveClawdServerRpc({ CLAWD_RPC: url }));
  }
  assert.throws(() => resolveClawdRpc({ CLAWD_PUBLIC_RPC: 'http://validator:8080' }));
});
test('render preserves action targets and discloses actual test-network capabilities', () => {
  const React = require('../deploy/hetzner/frontend/node_modules/react');
  const { renderToStaticMarkup } = require('../deploy/hetzner/frontend/node_modules/react-dom/server');
  const { ClawdChainApp } = require('../percolator-launch/app/components/clawd/ClawdChainApp');
  const html = renderToStaticMarkup(React.createElement(ClawdChainApp));
  for (const id of ['launch','browse','trade','earn','wallet','markets','perp-market','btn-long','btn-short','btn-stake','btn-mint']) assert.ok(html.includes(`id="${id}"`), id);
  for (const text of ['NOT REAL MONEY','No live oracle','house-managed','have not been burned']) assert.ok(html.includes(text), text);
  assert.ok(!html.includes('privy.percolator.trade'));
});
