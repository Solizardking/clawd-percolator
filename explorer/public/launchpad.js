// Thin form over the shipped curve. Classic script: no import map, no require.
(function () {
  var L = window.ClawdLaunchpad;
  var wallet = 'you';
  var coin = null;

  function $(id) { return document.getElementById(id); }

  function text(id, value) {
    var el = $(id);
    if (el) el.textContent = value;
  }

  function formatQuote(raw) {
    var micros = raw * 1000000n / L.QUOTE_SCALE;
    var whole = micros / 1000000n;
    var frac = (micros % 1000000n).toString();
    while (frac.length < 6) frac = '0' + frac;
    return whole.toString() + '.' + frac;
  }

  function formatCap(quoteRaw, tokenRaw, supply) {
    var micros = quoteRaw * supply * 1000000n / tokenRaw / L.QUOTE_SCALE;
    var whole = micros / 1000000n;
    var frac = (micros % 1000000n).toString();
    while (frac.length < 4) frac = '0' + frac;
    return whole.toString() + '.' + frac.slice(0, 4) + ' CLAWD';
  }

  function render() {
    if (!coin) {
      text('phase', 'no coin');
      text('price', '—');
      text('reserve', '—');
      text('supply', '—');
      text('sellable', '—');
      text('held', '—');
      text('balance', '—');
      return;
    }
    var quoteRaw = coin.phase === 'amm' ? coin.amm.quote : coin.curve.virtualQuote;
    var tokenRaw = coin.phase === 'amm' ? coin.amm.token : coin.curve.virtualToken;
    text('phase', coin.phase);
    text('status', coin.name + ' (' + coin.symbol + ')');
    text('price', formatCap(quoteRaw, tokenRaw, coin.supply));
    text('reserve', formatQuote(coin.curve.realQuote) + ' CLAWD');
    text('supply', coin.supply.toString());
    text('sellable', coin.curve.realToken.toString());
    text('held', (coin.holdings[wallet] || 0n).toString());
    text('balance', formatQuote(coin.balances[wallet] || 0n));
  }

  function parseQuote(value) {
    var match = /^(\d+)(?:\.(\d{1,6}))?$/.exec(String(value).trim());
    if (!match) return null;
    var frac = match[2] || '';
    while (frac.length < 6) frac += '0';
    var micros = BigInt(match[1]) * 1000000n + BigInt(frac);
    if (micros <= 0n) return null;
    return micros * L.QUOTE_SCALE / 1000000n;
  }

  $('create').onclick = function () {
    try {
      coin = L.createCoin({ name: $('name').value, symbol: $('symbol').value });
      L.fund(coin, wallet, 1000n * L.QUOTE_SCALE);
      text('notice', 'curve open');
      render();
    } catch (err) {
      text('notice', (err && err.message) || 'create failed');
    }
  };

  $('buy').onclick = function () {
    if (!coin) return text('notice', 'create a coin first');
    var amount = parseQuote($('amount').value);
    if (amount == null) return text('notice', 'amount');
    var result = L.buy(coin, {
      wallet: wallet,
      quoteIn: amount,
      creatorWallet: 'creator',
      holderWallet: 'holders',
    });
    text('notice', result.ok ? 'bought ' + result.tokensOut.toString() : result.reason);
    render();
  };

  $('sell').onclick = function () {
    if (!coin) return text('notice', 'create a coin first');
    var raw = String($('tokens').value).trim();
    if (!/^\d+$/.test(raw) || raw === '0') return text('notice', 'tokens');
    var result = L.sell(coin, {
      wallet: wallet,
      tokenIn: BigInt(raw),
      creatorWallet: 'creator',
      holderWallet: 'holders',
    });
    text('notice', result.ok ? 'sold for ' + result.quoteOut.toString() : result.reason);
    render();
  };

  render();
})();

// Deeper-liquidity form. Separate pool from the paper curve above.
(function () {
  var D = window.ClawdDbc;
  if (!D) return;
  var wallet = 'you';
  var pool = null;

  function $(id) { return document.getElementById(id); }

  function text(id, value) {
    var el = $(id);
    if (el) el.textContent = value;
  }

  function parseUnits(value, decimals) {
    var match = new RegExp('^(\\d+)(?:\\.(\\d{1,' + decimals + '}))?$').exec(String(value).trim());
    if (!match) return null;
    var frac = match[2] || '';
    while (frac.length < decimals) frac += '0';
    var scale = 10n ** BigInt(decimals);
    var raw = BigInt(match[1]) * scale + BigInt(frac);
    if (raw <= 0n) return null;
    return raw;
  }

  function rewardOf() {
    var book = pool && pool.owed[wallet] && pool.owed[wallet].trader;
    return book ? book.quote : 0n;
  }

  function renderDbc() {
    if (!pool) {
      text('dbc-phase', 'no pool');
      text('dbc-fee', '—');
      text('dbc-reserve', '—');
      text('dbc-reward', '—');
      return;
    }
    var fee = D.quotedFee(pool);
    text('dbc-phase', pool.migrationState);
    text('dbc-status', pool.name + ' (' + pool.symbol + ')');
    text('dbc-fee', D.formatFeePercent(fee.total));
    text('dbc-reserve', D.formatQuote(pool.quoteReserve));
    text('dbc-reward', D.formatQuote(rewardOf()));
  }

  var create = $('dbc-create');
  var buy = $('dbc-buy');
  var sell = $('dbc-sell');
  var migrate = $('dbc-migrate');
  var claim = $('dbc-claim');
  if (!create || !buy || !sell || !migrate || !claim) return;

  create.onclick = function () {
    try {
      var style = $('dbc-style').value;
      if (!style) style = 'opening';
      var cfg = D.preset(style);
      pool = D.createPool(cfg, {
        name: $('dbc-name').value,
        symbol: $('dbc-symbol').value,
        creator: wallet,
        feeClaimer: 'partner',
      });
      D.fund(pool, wallet, 1000n * D.QUOTE_UNIT);
      text('dbc-notice', 'curve open');
      renderDbc();
    } catch (err) {
      text('dbc-notice', (err && err.message) || 'create failed');
    }
  };

  buy.onclick = function () {
    if (!pool) return text('dbc-notice', 'create a pool first');
    var amount = parseUnits($('dbc-amount').value, 9);
    if (amount == null) return text('dbc-notice', 'amount');
    var result = D.act(pool, { wallet: wallet, as: 'trader' }, 'swap', { side: 'buy', quoteIn: amount });
    text('dbc-notice', result.ok ? 'bought ' + result.baseOut.toString() : result.reason);
    renderDbc();
  };

  sell.onclick = function () {
    if (!pool) return text('dbc-notice', 'create a pool first');
    var amount = parseUnits($('dbc-amount').value, pool.config.tokenDecimal);
    if (amount == null) return text('dbc-notice', 'amount');
    var result = D.act(pool, { wallet: wallet, as: 'trader' }, 'swap', { side: 'sell', baseIn: amount });
    text('dbc-notice', result.ok ? 'sold ' + result.quoteOut.toString() : result.reason);
    renderDbc();
  };

  migrate.onclick = function () {
    if (!pool) return text('dbc-notice', 'create a pool first');
    var result = D.act(pool, { wallet: wallet, as: 'keeper' }, 'migrate');
    text('dbc-notice', result.ok ? 'migrated' : result.reason);
    renderDbc();
  };

  claim.onclick = function () {
    if (!pool) return text('dbc-notice', 'create a pool first');
    var result = D.act(pool, { wallet: wallet, as: 'trader' }, 'claim', { asset: 'both' });
    text('dbc-notice', result.ok ? 'claimed ' + result.quote.toString() : result.reason);
    renderDbc();
  };

  renderDbc();
})();

// Clawdium constant-product pool. Separate from the paper coin and the sqrt-price pool.
(function () {
  var C = window.Clawdium;
  if (!C) return;
  var wallet = 'you';
  var book = C.createBook();
  var pool = null;

  function $(id) { return document.getElementById(id); }

  function text(id, value) {
    var el = $(id);
    if (el) el.textContent = value;
  }

  function parseUnits(value, decimals) {
    var match = new RegExp('^(\\d+)(?:\\.(\\d{1,' + decimals + '}))?$').exec(String(value).trim());
    if (!match) return null;
    var frac = match[2] || '';
    while (frac.length < decimals) frac += '0';
    var scale = 10n ** BigInt(decimals);
    var raw = BigInt(match[1]) * scale + BigInt(frac);
    if (raw <= 0n) return null;
    return raw;
  }

  function renderClawdium() {
    if (!pool) {
      text('clawdium-phase', 'no pool');
      text('clawdium-fee', '—');
      text('clawdium-reserve', '—');
      text('clawdium-lp', '—');
      text('clawdium-price', '—');
      return;
    }
    var curve = C.curveBalances(pool);
    var quoteSide = pool.quoteMint === pool.mintA ? curve.a : curve.b;
    var baseSide = pool.quoteMint === pool.mintA ? curve.b : curve.a;
    var baseDecimals = pool.decimals[pool.baseMint];
    if (baseDecimals == null) baseDecimals = 6;
    var price = baseSide === 0n ? 0n : quoteSide * (10n ** BigInt(baseDecimals)) / baseSide;
    text('clawdium-phase', pool.status);
    text('clawdium-status', pool.name + ' (' + pool.symbol + ')');
    text('clawdium-fee', C.formatTradeFee(pool));
    text('clawdium-reserve', C.formatQuote(quoteSide));
    text('clawdium-lp', (pool.lpBalances[wallet] || 0n).toString());
    text('clawdium-price', C.formatQuote(price));
  }

  var create = $('clawdium-create');
  var swapIn = $('clawdium-swap-in');
  var swapOut = $('clawdium-swap-out');
  if (!create || !swapIn || !swapOut) return;

  create.onclick = function () {
    try {
      var name = $('clawdium-name').value;
      var symbol = $('clawdium-symbol').value;
      if (!String(name).trim()) throw new Error('name');
      if (!String(symbol).trim()) throw new Error('symbol');
      var baseMint = 'base' + book.seq;
      C.fundLamports(book, wallet, 2000000000n);
      C.fund(book, wallet, C.CLAWD_MINT, 2000n * C.QUOTE_UNIT);
      C.fund(book, wallet, baseMint, 2000000000000n);
      pool = C.createPool(book, C.standardConfig(), {
        name: name,
        symbol: symbol,
        creator: wallet,
        mint0: C.CLAWD_MINT,
        mint1: baseMint,
        amount0: 1000n * C.QUOTE_UNIT,
        amount1: 2000000000000n,
        quoteMint: C.CLAWD_MINT,
        quoteDecimals: 9,
        baseDecimals: 6,
      });
      text('clawdium-notice', 'pool open');
      renderClawdium();
    } catch (err) {
      text('clawdium-notice', (err && err.message) || 'create failed');
    }
  };

  swapIn.onclick = function () {
    if (!pool) return text('clawdium-notice', 'create a pool first');
    var amount = parseUnits($('clawdium-amount').value, 9);
    if (amount == null) return text('clawdium-notice', 'amount');
    var result = C.act(pool, { wallet: wallet, as: 'trader' }, 'swapBaseInput', {
      inputMint: pool.quoteMint,
      amountIn: amount,
    });
    text('clawdium-notice', result.ok ? 'swapped ' + result.amountOut.toString() : result.reason);
    renderClawdium();
  };

  swapOut.onclick = function () {
    if (!pool) return text('clawdium-notice', 'create a pool first');
    var decimals = pool.decimals[pool.baseMint];
    if (decimals == null) decimals = 6;
    var amount = parseUnits($('clawdium-amount').value, decimals);
    if (amount == null) return text('clawdium-notice', 'amount');
    var result = C.act(pool, { wallet: wallet, as: 'trader' }, 'swapBaseInput', {
      inputMint: pool.baseMint,
      amountIn: amount,
    });
    text('clawdium-notice', result.ok ? 'swapped ' + result.amountOut.toString() : result.reason);
    renderClawdium();
  };

  renderClawdium();
})();
