// Percolator launch page wiring. Classic script; needs /percolator.js and /pow.js.
(function () {
  var P = window.ClawdPercolator, POW = window.ClawdPow;
  var main = document.querySelector('main[data-rpc]');
  var RPC = main.getAttribute('data-rpc');
  var FAUCET = main.getAttribute('data-faucet') || 'https://faucet.clawdchain.trade';
  var EXPLORER = main.getAttribute('data-explorer') || 'https://explorer.clawdchain.trade';
  var KEY = 'clawd.percolator.phrase';
  var wallet = null;
  function $(id) { return document.getElementById(id); }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) { return '&#' + c.charCodeAt(0) + ';'; }); }
  function short(a) { return a ? a.slice(0, 4) + '…' + a.slice(-4) : ''; }
  function ago(t) { if (!t) return '-'; var s = Math.max(0, Date.now() / 1000 - t); return s < 60 ? Math.floor(s) + 's ago' : s < 3600 ? Math.floor(s / 60) + 'm ago' : s < 86400 ? Math.floor(s / 3600) + 'h ago' : new Date(t * 1000).toLocaleString(); }
  function notice(msg, bad) { var n = $('notice'); n.innerHTML = msg; n.style.color = bad ? 'var(--red)' : 'var(--cy)'; }
  async function load(phrase) {
    wallet = await P.keyFromPhrase(phrase);
    localStorage.setItem(KEY, phrase);
    $('wallet-address').innerHTML = '<a href="' + EXPLORER + '/address/' + wallet.address + '" target="_blank" rel="noopener">' + wallet.address + '</a>';
    refresh(); position();
  }
  async function refresh() {
    if (!wallet) return;
    try { var b = await P.rpc(RPC, 'getBalance', [wallet.address, { commitment: 'confirmed' }]); $('wallet-balance').textContent = (b.value / 1e9).toLocaleString(undefined, { maximumFractionDigits: 4 }) + ' CLAWD'; }
    catch (e) { $('wallet-balance').textContent = 'RPC unreachable'; }
  }
  async function faucet() {
    if (!wallet) return;
    var btn = $('btn-faucet'); btn.disabled = true;
    try {
      notice('Solving faucet proof of work…');
      var ch = await (await fetch(FAUCET + '/challenge')).json();
      var s = await POW.solve(ch.challenge, ch.bits);
      var r = await (await fetch(FAUCET + '/airdrop', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ address: wallet.address, challenge: ch.challenge, nonce: s.nonce }) })).json();
      if (!r.signature) throw new Error(r.error || 'faucet error');
      notice('Faucet sent CLAWD. <a target="_blank" rel="noopener" href="' + EXPLORER + '/tx/' + r.signature + '">View transaction</a>');
      setTimeout(refresh, 2500);
    } catch (e) { notice(esc(e.message), true); } finally { btn.disabled = false; }
  }
  async function submit(ev) {
    ev.preventDefault();
    if (!wallet) return notice('Create or import a wallet first.', true);
    var btn = ev.target.querySelector('button[type=submit]'); btn.disabled = true;
    try {
      var name = P.cleanName($('clawd-market-name').value);
      notice('Signing and sending the market launch…');
      var r = await P.createMarket({ rpc: RPC, wallet: wallet, name: name, symbol: $('clawd-market-symbol').value });
      notice('Market <b>' + esc(r.name) + '</b> ' + (r.pending ? 'sent (still confirming)' : 'created') + '. <a target="_blank" rel="noopener" href="' + EXPLORER + '/tx/' + r.signature + '">Transaction</a> · <a target="_blank" rel="noopener" href="' + EXPLORER + '/address/' + r.market + '">Market account</a>');
      refresh();
      if (!r.pending) provision(r.signature, r.name);
      setTimeout(markets, 1500); setTimeout(markets, 6000);
    } catch (e) {
      var m = e.message || String(e);
      if (/insufficient|debit an account|no record of a prior credit/i.test(m)) m = 'Not enough CLAWD. Use “Get CLAWD from faucet” first.';
      notice(esc(m), true);
    } finally { btn.disabled = false; }
  }
  async function markets() {
    try {
      var response = await fetch('/api/markets', { cache: 'no-store' });
      var r = await response.json();
      if (!response.ok || r.error) throw new Error(r.error || 'Market service unavailable');
      var rows = (r.markets || []).map(function (m) {
        return '<tr><td><b>' + esc(m.name) + '</b>' + (m.symbol ? ' <span class="sub">' + esc(m.symbol) + '</span>' : '') + '</td>'
          + '<td class="mono"><a target="_blank" rel="noopener" href="' + EXPLORER + '/address/' + esc(m.creator) + '">' + esc(short(m.creator)) + '</a></td>'
          + '<td class="sub">' + esc(ago(m.time)) + '</td>'
          + '<td class="mono"><a target="_blank" rel="noopener" href="' + EXPLORER + '/address/' + esc(m.market) + '">' + esc(short(m.market)) + '</a></td>'
          + '<td class="mono"><a target="_blank" rel="noopener" href="' + EXPLORER + '/tx/' + esc(m.signature) + '">' + esc(short(m.signature)) + '</a></td>'
          + '<td>' + (m.tradable ? '<button class="btn" style="padding:4px 10px" data-trade="' + esc(m.perp) + '">Trade</button> <a class="mono sub" target="_blank" rel="noopener" href="' + EXPLORER + '/address/' + esc(m.perp) + '">' + esc(short(m.perp)) + '</a>' : (r.trading ? '<button class="btn" style="padding:4px 10px" data-activate="' + esc(m.signature) + '">Activate trading</button>' : '<span class="sub">–</span>')) + '</td></tr>';
      });
      $('markets').innerHTML = rows.join('') || '<tr><td colspan="6" class="sub">No markets yet. Launch the first one.</td></tr>';
      perpList((r.markets || []).filter(function (m) { return m.tradable; }));
      $('market-count').textContent = (r.markets || []).length;
    } catch (e) { $('market-count').textContent = 'unavailable'; $('markets').innerHTML = '<tr><td colspan="6" class="sub">Market list unavailable. Retry shortly; this does not mean zero markets.</td></tr>'; }
  }
  // ── Trading ──
  var perp = '';
  function tnote(msg, bad) { var n = $('trade-notice'); n.innerHTML = msg; n.style.color = bad ? 'var(--red)' : 'var(--cy)'; }
  function fmt(atoms) { var n = Number(atoms || 0) / 1e6; return n.toLocaleString(undefined, { maximumFractionDigits: 6 }); }
  function perpList(list) {
    var sel = $('perp-market'), cur = perp || sel.value;
    sel.innerHTML = list.map(function (m) { return '<option value="' + esc(m.perp) + '">' + esc(m.name) + (m.symbol ? ' (' + esc(m.symbol) + ')' : '') + ' · ' + esc(short(m.perp)) + '</option>'; }).join('') || '<option value="">No tradable markets yet</option>';
    if (cur && list.some(function (m) { return m.perp === cur; })) sel.value = cur;
    if (sel.value !== perp) { perp = sel.value; position(); }
  }
  async function api(path, body) {
    var r = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
    var j = await r.json().catch(function () { return {}; });
    if (!r.ok) throw new Error(j.error || ('HTTP ' + r.status));
    return j;
  }
  async function provision(sig, name) {
    try {
      notice('Provisioning a tradable Percolator market for <b>' + esc(name || '') + '</b>… (about 10 seconds)');
      var m = await api('/api/markets/provision', { signature: sig });
      notice('Market <b>' + esc(m.name) + '</b> is live for trading. <a target="_blank" rel="noopener" href="' + EXPLORER + '/address/' + m.market + '">Perp market account</a>');
      perp = m.market; markets();
    } catch (e) { notice(esc(e.message), true); }
  }
  async function act(action, extra, label) {
    if (!wallet) return tnote('Create or import a wallet first.', true);
    if (!perp) return tnote('Pick a tradable market first.', true);
    var buttons = document.querySelectorAll('[data-surface] .actions button'); buttons.forEach(function (b) { b.disabled = true; });
    try {
      tnote(label + '…');
      var body = Object.assign({ market: perp, owner: wallet.address }, extra || {});
      var r = await api('/api/tx/' + action, body);
      var sig = r.signature;
      if (r.tx) sig = await P.sendSigned(RPC, await P.signWire(r.tx, wallet));
      tnote(label + ' done. <a target="_blank" rel="noopener" href="' + EXPLORER + '/tx/' + sig + '">' + esc(short(sig)) + '</a>');
      window.lastTradeSig = sig;
      setTimeout(position, 800); refresh();
    } catch (e) {
      var m = e.message || String(e);
      if (/insufficient funds|no record of a prior credit/i.test(m)) m = 'Not enough CLAWD for fees. Use the faucet first.';
      tnote(esc(m), true);
    } finally { buttons.forEach(function (b) { b.disabled = false; }); }
  }
  async function position() {
    if (!wallet || !perp) return;
    try {
      var r = await (await fetch('/api/position?market=' + encodeURIComponent(perp) + '&owner=' + wallet.address, { cache: 'no-store' })).json();
      $('pos-price').textContent = r.price ? fmt(r.price) + ' cUSD' : '–';
      $('pos-usd').textContent = fmt(r.usd);
      $('pos-capital').textContent = r.position ? fmt(r.position.capital) + ' cUSD' : (r.portfolio ? '0' : 'no account yet');
      var b = r.position ? Number(r.position.basis) / 1e6 : 0;
      $('pos-size').textContent = b ? (b > 0 ? 'Long ' : 'Short ') + Math.abs(b) : 'flat';
      $('pos-lp').textContent = fmt(r.lp);
      $('pos-nft').innerHTML = r.nft ? '<a target="_blank" rel="noopener" href="' + EXPLORER + '/address/' + r.nft + '">' + esc(short(r.nft)) + '</a>' : 'none';
      $('btn-open').disabled = !!r.portfolio;
    } catch (e) {}
  }
  $('perp-market').onchange = function () { perp = this.value; position(); };
  $('btn-usd').onclick = function () { act('usd', {}, 'Minting 1,000 test cUSD'); };
  $('btn-open').onclick = function () { act('portfolio', {}, 'Opening trading account'); };
  $('btn-deposit').onclick = function () { act('deposit', { amount: $('amt-margin').value }, 'Depositing margin'); };
  $('btn-withdraw').onclick = function () { act('withdraw', { amount: $('amt-margin').value }, 'Withdrawing margin'); };
  $('btn-long').onclick = function () { act('trade', { side: 'long', size: $('amt-size').value }, 'Opening long'); };
  $('btn-short').onclick = function () { act('trade', { side: 'short', size: $('amt-size').value }, 'Opening short'); };
  $('btn-stake').onclick = function () { act('stakeDeposit', { amount: $('amt-stake').value }, 'Staking into insurance'); };
  $('btn-unstake').onclick = function () { act('stakeWithdraw', {}, 'Withdrawing stake'); };
  $('btn-mint').onclick = function () { act('nftMint', {}, 'Minting position NFT'); };
  $('btn-burn').onclick = $('btn-unwrap').onclick = function () { act('nftBurn', {}, 'Unwrapping position NFT'); };
  document.getElementById('markets').addEventListener('click', function (ev) {
    var t = ev.target.closest('button'); if (!t) return;
    if (t.dataset.trade) { perp = t.dataset.trade; $('perp-market').value = perp; position(); $('perp-market').scrollIntoView({ behavior: 'smooth', block: 'center' }); }
    if (t.dataset.activate) { t.disabled = true; provision(t.dataset.activate); }
  });
  setInterval(position, 10000);

  $('btn-new').onclick = function () { if (!wallet || confirm('Replace this browser wallet? Save its phrase first if it holds CLAWD.')) load(P.randomPhrase()); };
  $('btn-import').onclick = function () { var p = prompt('Paste a clawd wallet phrase'); if (p && p.trim()) load(p.trim()); };
  $('btn-export').onclick = function () { if (wallet) prompt('Your wallet phrase (keep it private):', localStorage.getItem(KEY)); };
  $('btn-faucet').onclick = faucet;
  document.querySelector('form[data-action=create-market]').addEventListener('submit', submit);
  var q = new URLSearchParams(location.search).get('name'); if (q) $('clawd-market-name').value = q.slice(0, 32);
  var saved = localStorage.getItem(KEY); load(saved || P.randomPhrase());
  markets(); setInterval(markets, 10000); setInterval(refresh, 15000);
})();
