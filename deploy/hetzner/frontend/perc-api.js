// Percolator trading API for the clawd chain. Builds transactions server-side; the browser
// signs its own slot. The house wallet (testnet only) is market admin, LP counterparty and
// the test-collateral (cUSD) mint authority. Its key lives in DATA_DIR, never in git.
const fs = require("fs");
const path = require("path");
const web3 = require("@solana/web3.js");
const spl = require("@solana/spl-token");
const { create } = require("./perc-core.js");

const PK = (x) => new web3.PublicKey(x);
const isAddr = (a) => { try { return typeof a === "string" && PK(a).toBytes().length === 32; } catch (e) { return false; } };

function init(opts) {
  const C = create(opts.programs);
  const conn = new web3.Connection(opts.rpc, "confirmed");
  const dataDir = opts.dataDir;
  const statePath = path.join(dataDir, "markets.json");
  let house = null;
  try { house = web3.Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path.join(dataDir, "house.json"), "utf8")))); } catch (e) { house = null; }
  let state = { markets: [], portfolios: {} };
  try { state = Object.assign(state, JSON.parse(fs.readFileSync(statePath, "utf8"))); } catch (e) {}
  const save = () => { fs.writeFileSync(statePath + ".tmp", JSON.stringify(state, null, 1)); fs.renameSync(statePath + ".tmp", statePath); };
  const byMarket = (m) => state.markets.find((x) => x.market === m);
  const byLaunch = (s) => state.markets.find((x) => x.launchSig === s);
  const hits = new Map();
  setInterval(() => hits.clear(), 60000).unref();
  const limit = (key, n) => { const c = (hits.get(key) || 0) + 1; hits.set(key, c); if (c > n) throw httpErr(429, "Slow down and try again in a minute"); };
  function httpErr(code, msg) { const e = new Error(msg); e.code = code; return e; }
  // Confirm by polling: the in-cluster RPC has no websocket port, so
  // sendAndConfirmTransaction would wait until the blockhash expires.
  async function sendHouse(ixs, extra) {
    const tx = new web3.Transaction().add(...ixs);
    const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash("confirmed");
    tx.recentBlockhash = blockhash; tx.feePayer = house.publicKey;
    tx.sign(house, ...(extra || []));
    const raw = tx.serialize();
    const sig = await conn.sendRawTransaction(raw, { preflightCommitment: "confirmed" });
    for (let i = 0; i < 90; i++) {
      await new Promise((r) => setTimeout(r, 700));
      const st = (await conn.getSignatureStatuses([sig])).value[0];
      if (st && st.err) throw new Error("Transaction " + sig + " failed: " + JSON.stringify(st.err));
      if (st && (st.confirmationStatus === "confirmed" || st.confirmationStatus === "finalized")) return sig;
      if (i % 5 === 4) {
        if ((await conn.getBlockHeight("confirmed")) > lastValidBlockHeight) throw new Error("Transaction " + sig + " expired");
        conn.sendRawTransaction(raw, { skipPreflight: true }).catch(() => {});
      }
    }
    throw new Error("Transaction " + sig + " not confirmed");
  }
  const rent = (n) => conn.getMinimumBalanceForRentExemption(n);

  async function build(feePayer, ixs, partial) {
    const tx = new web3.Transaction().add(...ixs);
    tx.feePayer = PK(feePayer);
    tx.recentBlockhash = (await conn.getLatestBlockhash("confirmed")).blockhash;
    if (partial && partial.length) tx.partialSign(...partial);
    return { tx: tx.serialize({ requireAllSignatures: false, verifySignatures: false }).toString("base64") };
  }

  async function provision({ signature }) {
    if (!house) throw httpErr(503, "Trading is not configured on this server");
    if (typeof signature !== "string" || signature.length < 64) throw httpErr(400, "Missing launch signature");
    const existing = byLaunch(signature);
    if (existing) return existing;
    if (opts.hidden.has(signature)) throw httpErr(400, "This launch is hidden");
    limit("provision", 3);
    if (state.markets.length >= 200) throw httpErr(429, "Market limit reached");
    const t = await conn.getTransaction(signature, { commitment: "confirmed", maxSupportedTransactionVersion: 0 });
    if (!t || t.meta.err) throw httpErr(400, "Launch transaction not found or failed");
    const keys = t.transaction.message.staticAccountKeys || t.transaction.message.accountKeys;
    if (!keys.some((k) => k.toBase58() === opts.registry)) throw httpErr(400, "Not a Percolator launch");
    const memo = (t.meta.logMessages || []).map((l) => opts.parseMemo(l)).find(Boolean);
    if (!memo) throw httpErr(400, "Launch record missing");
    const out = await setupMarket(memo.name, memo.symbol);
    Object.assign(out, { launchSig: signature, creator: memo.creator, launchAccount: memo.market, created: t.blockTime });
    state.markets.push(out); save();
    return out;
  }

  async function setupMarket(name, symbol) {
    const mint = await spl.createMint(conn, house, house.publicKey, null, 6);
    const market = web3.Keypair.generate();
    const initSig = await sendHouse([C.createProgramAccount(house.publicKey, market.publicKey, await rent(C.MARKET_LEN), C.MARKET_LEN), ...C.budget(), C.initMarket(house.publicKey, market.publicKey, mint)], [market]);
    const st = { name, symbol: symbol || "", market: market.publicKey.toBase58(), mint: mint.toBase58(), initSig, price: C.DEFAULT_PARAMS.initial_price };
    st.vault = (await spl.getOrCreateAssociatedTokenAccount(conn, house, mint, C.vaultAuthority(market.publicKey), true)).address.toBase58();
    await sendHouse([...C.budget(), C.setNftProgram(house.publicKey, market.publicKey)]);
    const hp = web3.Keypair.generate();
    await sendHouse([C.createProgramAccount(house.publicKey, hp.publicKey, await rent(C.PORTFOLIO_LEN), C.PORTFOLIO_LEN), ...C.budget(), C.initPortfolio(house.publicKey, market.publicKey, hp.publicKey)], [hp]);
    st.housePortfolio = hp.publicKey.toBase58();
    const ata = await spl.getOrCreateAssociatedTokenAccount(conn, house, mint, house.publicKey);
    await spl.mintTo(conn, house, mint, ata.address, house, 1_000_000_000_000_000n);
    await sendHouse([...C.budget(), C.deposit(house.publicKey, st, hp.publicKey, ata.address, 100_000_000_000_000n)]);
    const lp = web3.Keypair.generate(), sv = web3.Keypair.generate();
    st.lpMint = lp.publicKey.toBase58(); st.stakeVault = sv.publicKey.toBase58();
    await sendHouse([web3.SystemProgram.createAccount({ fromPubkey: house.publicKey, newAccountPubkey: lp.publicKey, lamports: await rent(82), space: 82, programId: spl.TOKEN_PROGRAM_ID }),
      web3.SystemProgram.createAccount({ fromPubkey: house.publicKey, newAccountPubkey: sv.publicKey, lamports: await rent(165), space: 165, programId: spl.TOKEN_PROGRAM_ID }),
      ...C.budget(), C.initPool(house.publicKey, st, lp.publicKey, sv.publicKey, 10)], [lp, sv]);
    st.pool = C.pool(market.publicKey).toBase58();
    return st;
  }

  function need(body, ...fields) { for (const f of fields) if (!isAddr(body[f])) throw httpErr(400, "Bad " + f); }
  function marketOf(body) { const m = byMarket(body.market); if (!m) throw httpErr(404, "Unknown market"); return m; }
  const portfolioKey = (m, owner) => state.portfolios[m.market + ":" + owner];
  const amountArg = (v, max) => { const s = String(v || "").trim(); if (!/^\d+(\.\d{1,6})?$/.test(s)) throw httpErr(400, "Enter an amount"); const [w, f = ""] = s.split("."); const a = BigInt(w) * 1_000_000n + BigInt((f + "000000").slice(0, 6)); if (a <= 0n || a > max) throw httpErr(400, "Amount out of range"); return a; };

  const actions = {
    async usd(body) {
      need(body, "owner"); const m = marketOf(body); if (!house) throw httpErr(503, "Trading is not configured");
      limit("usd:" + body.owner, 3);
      const ata = spl.getAssociatedTokenAddressSync(PK(m.mint), PK(body.owner));
      const sig = await sendHouse([spl.createAssociatedTokenAccountIdempotentInstruction(house.publicKey, ata, PK(body.owner), PK(m.mint)), spl.createMintToInstruction(PK(m.mint), ata, house.publicKey, 1_000_000_000n)]);
      return { signature: sig };
    },
    async portfolio(body) {
      need(body, "owner"); const m = marketOf(body);
      const existing = portfolioKey(m, body.owner);
      if (existing && (await conn.getAccountInfo(PK(existing)))) throw httpErr(400, "Account already open");
      const p = web3.Keypair.generate();
      state.portfolios[m.market + ":" + body.owner] = p.publicKey.toBase58(); save();
      return build(body.owner, [C.createProgramAccount(body.owner, p.publicKey, await rent(C.PORTFOLIO_LEN), C.PORTFOLIO_LEN), ...C.budget(), C.initPortfolio(body.owner, m.market, p.publicKey)], [p]);
    },
    async deposit(body) {
      need(body, "owner"); const m = marketOf(body); const p = portfolioKey(m, body.owner); if (!p) throw httpErr(400, "Open a trading account first");
      const ata = spl.getAssociatedTokenAddressSync(PK(m.mint), PK(body.owner));
      return build(body.owner, [...C.budget(), C.deposit(body.owner, m, p, ata, amountArg(body.amount, 1_000_000_000_000n))]);
    },
    async withdraw(body) {
      need(body, "owner"); const m = marketOf(body); const p = portfolioKey(m, body.owner); if (!p) throw httpErr(400, "No trading account");
      const ata = spl.getAssociatedTokenAddressSync(PK(m.mint), PK(body.owner));
      return build(body.owner, [spl.createAssociatedTokenAccountIdempotentInstruction(PK(body.owner), ata, PK(body.owner), PK(m.mint)), ...C.budget(), C.withdraw(body.owner, m, p, ata, amountArg(body.amount, 1_000_000_000_000n))]);
    },
    async trade(body) {
      need(body, "owner"); const m = marketOf(body); const p = portfolioKey(m, body.owner); if (!p) throw httpErr(400, "Open a trading account first");
      if (!house) throw httpErr(503, "Trading is not configured");
      limit("trade:" + body.owner, 30);
      let size = amountArg(body.size, 1_000_000_000n);
      if (body.side === "short") size = -size; else if (body.side !== "long") throw httpErr(400, "Pick long or short");
      return build(body.owner, [...C.budget(), C.trade(body.owner, p, house.publicKey, m.housePortfolio, m.market, size, m.price, 10)], [house]);
    },
    async stakeDeposit(body) {
      need(body, "owner"); const m = marketOf(body);
      const src = spl.getAssociatedTokenAddressSync(PK(m.mint), PK(body.owner)); const lp = spl.getAssociatedTokenAddressSync(PK(m.lpMint), PK(body.owner));
      return build(body.owner, [spl.createAssociatedTokenAccountIdempotentInstruction(PK(body.owner), lp, PK(body.owner), PK(m.lpMint)), ...C.budget(), C.stakeDeposit(body.owner, m, src, lp, amountArg(body.amount, 1_000_000_000_000n))]);
    },
    async stakeWithdraw(body) {
      need(body, "owner"); const m = marketOf(body);
      const dst = spl.getAssociatedTokenAddressSync(PK(m.mint), PK(body.owner)); const lp = spl.getAssociatedTokenAddressSync(PK(m.lpMint), PK(body.owner));
      const bal = await conn.getTokenAccountBalance(lp).then((r) => BigInt(r.value.amount), () => 0n);
      if (bal <= 0n) throw httpErr(400, "Nothing staked");
      return build(body.owner, [...C.budget(), C.stakeWithdraw(body.owner, m, lp, dst, bal)]);
    },
    async nftMint(body) {
      need(body, "owner"); const m = marketOf(body); const p = portfolioKey(m, body.owner); if (!p) throw httpErr(400, "Open a position first");
      const acct = await conn.getAccountInfo(PK(p)); const pf = acct && C.readPortfolio(acct.data);
      if (!pf || pf.basis === "0") throw httpErr(400, "Open a position before minting its NFT");
      const nm = web3.Keypair.generate();
      state.portfolios[m.market + ":" + body.owner + ":nft"] = nm.publicKey.toBase58(); save();
      return build(body.owner, [...C.budget(), C.mintNft(body.owner, m, p, pf.marketId, nm.publicKey)], [nm]);
    },
    async nftBurn(body) {
      need(body, "owner"); const m = marketOf(body); const p = portfolioKey(m, body.owner); const nft = state.portfolios[m.market + ":" + body.owner + ":nft"];
      if (!p || !nft) throw httpErr(400, "No position NFT");
      const acct = await conn.getAccountInfo(PK(p)); const pf = C.readPortfolio(acct.data);
      return build(body.owner, [...C.budget(), C.burnNft(body.owner, m, p, pf.marketId, nft, pf.basis === "0")]);
    },
  };

  async function position(q) {
    const m = byMarket(q.market); if (!m || !isAddr(q.owner)) throw httpErr(400, "Bad request");
    const p = portfolioKey(m, q.owner); const nft = state.portfolios[m.market + ":" + q.owner + ":nft"];
    const bal = (mint) => conn.getTokenAccountBalance(spl.getAssociatedTokenAddressSync(PK(mint), PK(q.owner))).then((r) => r.value.amount, () => "0");
    const [usd, lp, acct, nftAcct] = await Promise.all([bal(m.mint), bal(m.lpMint), p ? conn.getAccountInfo(PK(p)) : null, nft ? conn.getTokenAccountBalance(spl.getAssociatedTokenAddressSync(PK(nft), PK(q.owner), false, spl.TOKEN_2022_PROGRAM_ID)).then((r) => r.value.amount, () => "0") : "0"]);
    return { portfolio: acct ? p : null, position: acct ? C.readPortfolio(acct.data) : null, usd, lp, nft: nftAcct === "1" ? nft : null, price: m.price };
  }

  return {
    enabled: () => !!house,
    programs: { wrapper: C.W.toBase58(), stake: C.S.toBase58(), nft: C.N.toBase58() },
    tradable: (sig) => byLaunch(sig) || null,
    list: () => state.markets.slice(),
    markets: () => state.markets,
    provision, actions, position,
  };
}

module.exports = { init };
