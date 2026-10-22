// On-chain launches list (reads the clawd-launch tracker) + tab switch between it and the paper curve.
(function () {
  var API = 'https://clawd-launch-production.up.railway.app/api/launches';
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); }
  function short(a) { return a ? a.slice(0, 4) + '…' + a.slice(-4) : ''; }
  function num(n, d) { return Number(n || 0).toLocaleString(undefined, { maximumFractionDigits: d == null ? 2 : d }); }
  function ago(t) { var s = Math.max(0, Date.now() / 1000 - t); return s < 3600 ? Math.round(s / 60) + 'm ago' : s < 86400 ? Math.round(s / 3600) + 'h ago' : Math.round(s / 86400) + 'd ago'; }
  function tab() {
    var t = location.hash === '#paper' ? 'paper' : 'onchain';
    document.getElementById('tab-onchain').hidden = t !== 'onchain';
    document.getElementById('tab-paper').hidden = t !== 'paper';
    document.querySelectorAll('.lptabs a').forEach(function (a) { a.classList.toggle('hot', a.dataset.t === t); });
  }
  function load() {
    fetch(API).then(function (r) { return r.json(); }).then(function (d) {
      var L = (d.launches || []).slice().sort(function (a, b) { return b.createdAt - a.createdAt; });
      document.getElementById('lcount').textContent = L.length + ' coin' + (L.length === 1 ? '' : 's') + ' tracked · updated ' + new Date(d.indexedAt).toLocaleTimeString();
      document.getElementById('lrows').innerHTML = L.length ? L.map(function (l) {
        var ex = 'https://explorer.clawdchain.trade/address/';
        return '<tr><td><a href="' + ex + esc(l.mint) + '"><b>' + esc(l.symbol) + '</b> ' + esc(l.name) + '</a></td>' +
          '<td><a class="mono" href="' + ex + esc(l.creator) + '">' + short(l.creator) + '</a></td>' +
          '<td>' + ago(l.createdAt) + '</td><td>' + num(l.raisedClawd) + ' CLAWD</td><td>' + num((l.progress || 0) * 100, 1) + '%</td>' +
          '<td>' + l.trades + ' / ' + l.uniqueTraders + ' wallets</td><td>' + num(l.volumeClawd) + ' CLAWD</td>' +
          '<td>' + (l.graduated ? '🎓 AMM' : l.complete ? '✅ complete' : '📈 curve') + '</td></tr>';
      }).join('') : '<tr><td colspan="8">No coins yet.</td></tr>';
    }).catch(function () { document.getElementById('lcount').textContent = 'tracker unreachable'; });
  }
  window.addEventListener('hashchange', tab); tab(); load(); setInterval(load, 15000);
})();
