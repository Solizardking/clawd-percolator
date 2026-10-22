// Percolator market launch on the clawd chain. Classic browser script and Node module.
// Builds a legacy SVM transaction by hand (no outside libraries) and signs it with WebCrypto Ed25519.
// A market launch = System createAccount (the market account) + a 0.001 CLAWD registry fee
// + a Memo v2 record "percolator:market:v1:{json}". The registry address has no private key.
(function (root, factory) {
  var api = factory(root);
  if (typeof module === 'object' && module && module.exports) module.exports = api;
  root.ClawdPercolator = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function (root) {
  var ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  var SYSTEM = '11111111111111111111111111111111';
  var MEMO = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr';
  var REGISTRY = '56dhUqcecQyQun9WbSLrTouVxP6UhPbc3VXvdRZEHg1y';
  var PREFIX = 'percolator:market:v1:';
  var MARKET_LAMPORTS = 890880n;
  var REGISTRY_FEE = 1000000n;
  var PKCS8_PREFIX = new Uint8Array([0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20]);
  var enc = new TextEncoder();
  var subtle = (root.crypto && root.crypto.subtle) || require('crypto').webcrypto.subtle;
  var getRandom = function (n) { var b = new Uint8Array(n); (root.crypto || require('crypto').webcrypto).getRandomValues(b); return b; };

  function b58encode(bytes) {
    var digits = [];
    for (var i = 0; i < bytes.length; i++) {
      var carry = bytes[i];
      for (var j = 0; j < digits.length; j++) { carry += digits[j] << 8; digits[j] = carry % 58; carry = (carry / 58) | 0; }
      while (carry) { digits.push(carry % 58); carry = (carry / 58) | 0; }
    }
    var out = '';
    for (var k = 0; k < bytes.length && bytes[k] === 0; k++) out += '1';
    for (var q = digits.length - 1; q >= 0; q--) out += ALPHABET[digits[q]];
    return out;
  }
  function b58decode(str) {
    var bytes = [];
    for (var i = 0; i < str.length; i++) {
      var v = ALPHABET.indexOf(str[i]); if (v < 0) throw new Error('bad base58');
      var carry = v;
      for (var j = 0; j < bytes.length; j++) { carry += bytes[j] * 58; bytes[j] = carry & 0xff; carry >>= 8; }
      while (carry) { bytes.push(carry & 0xff); carry >>= 8; }
    }
    for (var k = 0; k < str.length && str[k] === '1'; k++) bytes.push(0);
    return new Uint8Array(bytes.reverse());
  }
  function pk(str) { var b = b58decode(str); if (b.length !== 32) throw new Error('bad address ' + str); return b; }
  function shortvec(n) { var out = []; do { var b = n & 0x7f; n >>= 7; if (n) b |= 0x80; out.push(b); } while (n); return out; }
  function u32(n) { var b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; }
  function u64(n) { var b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(n), true); return b; }
  function concat(parts) {
    var len = 0; parts.forEach(function (p) { len += p.length; });
    var out = new Uint8Array(len), o = 0;
    parts.forEach(function (p) { out.set(p, o); o += p.length; });
    return out;
  }
  function b64(bytes) {
    if (typeof Buffer !== 'undefined') return Buffer.from(bytes).toString('base64');
    var s = ''; for (var i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]); return btoa(s);
  }
  function hex(bytes) { return Array.from(bytes, function (b) { return b.toString(16).padStart(2, '0'); }).join(''); }

  // Keys: same derivation as the clawd web wallet (seed = sha256(phrase)), so a wallet phrase works here too.
  async function keyFromPhrase(phrase) {
    var seed = new Uint8Array(await subtle.digest('SHA-256', enc.encode(phrase)));
    return keyFromSeed(seed);
  }
  async function keyFromSeed(seed) {
    var der = concat([PKCS8_PREFIX, seed]);
    var privateKey = await subtle.importKey('pkcs8', der, { name: 'Ed25519' }, true, ['sign']);
    var jwk = await subtle.exportKey('jwk', privateKey);
    var raw = Uint8Array.from(atobUrl(jwk.x));
    return { privateKey: privateKey, publicKey: raw, address: b58encode(raw) };
  }
  function atobUrl(s) {
    s = s.replace(/-/g, '+').replace(/_/g, '/'); while (s.length % 4) s += '=';
    if (typeof Buffer !== 'undefined') return Buffer.from(s, 'base64');
    return Array.from(atob(s), function (c) { return c.charCodeAt(0); });
  }
  function newKey() { return keyFromSeed(getRandom(32)); }
  function randomPhrase() { return hex(getRandom(32)); }

  function cleanName(name) {
    var n = String(name || '').trim().replace(/\s+/g, ' ');
    if (!n) throw new Error('Enter a market name');
    if (n.length > 32) throw new Error('Market name must be 32 characters or fewer');
    return n;
  }
  function memoText(opts) {
    return PREFIX + JSON.stringify({ n: cleanName(opts.name), c: opts.creator, m: opts.market, s: String(opts.symbol || '').trim().slice(0, 10) || undefined });
  }
  function parseMemo(text) {
    if (typeof text !== 'string') return null;
    var i = text.indexOf(PREFIX); if (i < 0) return null;
    var body = text.slice(i + PREFIX.length).replace(/^"|"$/g, '');
    try { var j = JSON.parse(body); if (!j || typeof j.n !== 'string') return null; return { name: j.n, creator: j.c, market: j.m, symbol: j.s || '' }; } catch (e) { return null; }
  }

  // Legacy message: header, account keys, blockhash, instructions.
  function buildMessage(opts) {
    var payer = pk(opts.payer), market = pk(opts.market), registry = pk(REGISTRY);
    var keys = [payer, market, registry, pk(SYSTEM), pk(MEMO)];
    var memo = enc.encode(memoText({ name: opts.name, symbol: opts.symbol, creator: opts.payer, market: opts.market }));
    var createData = concat([u32(0), u64(MARKET_LAMPORTS), u64(0), pk(SYSTEM)]);
    var transferData = concat([u32(2), u64(REGISTRY_FEE)]);
    function ix(prog, accts, data) { return concat([[prog], shortvec(accts.length), accts, shortvec(data.length), data].map(function (x) { return x instanceof Uint8Array ? x : new Uint8Array(x); })); }
    var ixs = [ix(3, [0, 1], createData), ix(3, [0, 2], transferData), ix(4, [], memo)];
    return concat([
      new Uint8Array([2, 0, 2]), // 2 signers, 0 readonly signed, 2 readonly unsigned (System, Memo)
      new Uint8Array(shortvec(keys.length))].concat(keys).concat([
      pk(opts.blockhash),
      new Uint8Array(shortvec(ixs.length))]).concat(ixs));
  }
  async function signTx(message, signers) {
    var sigs = [];
    for (var i = 0; i < signers.length; i++) sigs.push(new Uint8Array(await subtle.sign({ name: 'Ed25519' }, signers[i].privateKey, message)));
    return concat([new Uint8Array(shortvec(sigs.length))].concat(sigs).concat([message]));
  }
  async function rpc(url, method, params) {
    var r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: method, params: params || [] }) });
    var j = await r.json();
    if (j.error) throw new Error(j.error.message || 'rpc error');
    return j.result;
  }
  async function createMarket(opts) {
    var name = cleanName(opts.name);
    var wallet = opts.wallet, url = opts.rpc;
    var market = await newKey();
    var bh = (await rpc(url, 'getLatestBlockhash', [{ commitment: 'confirmed' }])).value.blockhash;
    var msg = buildMessage({ payer: wallet.address, market: market.address, blockhash: bh, name: name, symbol: opts.symbol });
    var tx = await signTx(msg, [wallet, market]);
    var sig = await rpc(url, 'sendTransaction', [b64(tx), { encoding: 'base64', preflightCommitment: 'confirmed' }]);
    for (var i = 0; i < 40; i++) {
      var st = (await rpc(url, 'getSignatureStatuses', [[sig]])).value[0];
      if (st && st.err) throw new Error('Transaction failed: ' + JSON.stringify(st.err));
      if (st && (st.confirmationStatus === 'confirmed' || st.confirmationStatus === 'finalized')) return { signature: sig, market: market.address, name: name };
      await new Promise(function (r) { setTimeout(r, 750); });
    }
    return { signature: sig, market: market.address, name: name, pending: true };
  }
  function readShortvec(b, o) { var n = 0, shift = 0, x; do { x = b[o++]; n |= (x & 0x7f) << shift; shift += 7; } while (x & 0x80); return [n, o]; }
  function fromB64(s) { if (typeof Buffer !== 'undefined') return new Uint8Array(Buffer.from(s, 'base64')); var bin = atob(s), out = new Uint8Array(bin.length); for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i); return out; }
  // Add this wallet's signature to a server-built, partially signed wire transaction.
  async function signWire(b64tx, wallet) {
    var tx = fromB64(b64tx);
    var r = readShortvec(tx, 0), nsig = r[0], sigStart = r[1], msgStart = sigStart + nsig * 64;
    var msg = tx.slice(msgStart);
    var nreq = msg[0];
    var k = readShortvec(msg, 3), keysStart = k[1];
    var idx = -1;
    for (var i = 0; i < nreq; i++) {
      var key = msg.slice(keysStart + i * 32, keysStart + (i + 1) * 32);
      if (b58encode(key) === wallet.address) { idx = i; break; }
    }
    if (idx < 0) throw new Error('Transaction does not need this wallet');
    var sig = new Uint8Array(await subtle.sign({ name: 'Ed25519' }, wallet.privateKey, msg));
    tx.set(sig, sigStart + idx * 64);
    return b64(tx);
  }
  async function sendSigned(url, b64tx) {
    var sig = await rpc(url, 'sendTransaction', [b64tx, { encoding: 'base64', preflightCommitment: 'confirmed' }]);
    for (var i = 0; i < 40; i++) {
      var st = (await rpc(url, 'getSignatureStatuses', [[sig]])).value[0];
      if (st && st.err) throw new Error('Transaction failed: ' + JSON.stringify(st.err));
      if (st && (st.confirmationStatus === 'confirmed' || st.confirmationStatus === 'finalized')) return sig;
      await new Promise(function (r) { setTimeout(r, 750); });
    }
    return sig;
  }
  return {
    SYSTEM: SYSTEM, MEMO: MEMO, REGISTRY: REGISTRY, PREFIX: PREFIX, REGISTRY_FEE: REGISTRY_FEE, MARKET_LAMPORTS: MARKET_LAMPORTS,
    b58encode: b58encode, b58decode: b58decode, keyFromPhrase: keyFromPhrase, newKey: newKey, randomPhrase: randomPhrase,
    buildMessage: buildMessage, signTx: signTx, rpc: rpc, createMarket: createMarket, memoText: memoText, parseMemo: parseMemo, cleanName: cleanName, signWire: signWire, sendSigned: sendSigned
  };
});
