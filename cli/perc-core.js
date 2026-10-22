// Percolator instruction builders for the clawd chain (server side). No keys here.
const web3 = require('@solana/web3.js');
const spl = require('@solana/spl-token');
const { PublicKey, SystemProgram, TransactionInstruction, ComputeBudgetProgram } = web3;
const PK = (x) => (x instanceof PublicKey ? x : new PublicKey(x));
const MARKET_LEN = 3163, PORTFOLIO_LEN = 9539;
const OFF = { capital: 148, marketId: 348, basis: 370 };
const u16 = (n) => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const u128 = (n) => { n = BigInt(n); const b = Buffer.alloc(16); b.writeBigUInt64LE(n & ((1n << 64n) - 1n)); b.writeBigUInt64LE(n >> 64n, 8); return b; };
const i128 = (n) => { n = BigInt(n); if (n < 0n) n = (1n << 128n) + n; return u128(n); };
const meta = (pubkey, isSigner, isWritable) => ({ pubkey: PK(pubkey), isSigner, isWritable });

function create(ids) {
  const W = PK(ids.wrapper), S = PK(ids.stake), N = PK(ids.nft);
  const wix = (keys, data) => new TransactionInstruction({ programId: W, keys, data });
  const budget = () => [ComputeBudgetProgram.requestHeapFrame({ bytes: 128 * 1024 }), ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 })];
  const pdaW = (...s) => PublicKey.findProgramAddressSync(s, W)[0];
  const pdaS = (...s) => PublicKey.findProgramAddressSync(s, S)[0];
  const pdaN = (...s) => PublicKey.findProgramAddressSync(s, N)[0];
  const vaultAuthority = (market) => pdaW(Buffer.from('vault'), PK(market).toBuffer());
  const registry = (market) => pdaW(Buffer.from('nft_registry'), PK(market).toBuffer());
  const pool = (market) => pdaS(Buffer.from('stake_pool'), PK(market).toBuffer());
  const stakeVaultAuth = (market) => pdaS(Buffer.from('vault_auth'), pool(market).toBuffer());
  const depositPda = (market, user) => pdaS(Buffer.from('stake_deposit'), pool(market).toBuffer(), PK(user).toBuffer());
  const mintAuthority = () => pdaN(Buffer.from('mint_authority'));
  const positionNft = (portfolio, marketId) => pdaN(Buffer.from('position_nft'), PK(portfolio).toBuffer(), u64(marketId));
  const extraMetas = (nftMint) => pdaN(Buffer.from('extra-account-metas'), PK(nftMint).toBuffer());

  const DEFAULT_PARAMS = { max_portfolio_assets: 1, h_min: 0, h_max: 10, initial_price: 100_000_000, min_nonzero_mm_req: 1, min_nonzero_im_req: 2,
    maintenance_margin_bps: 10000, initial_margin_bps: 10000, max_trading_fee_bps: 1000, trade_fee_base_bps: 0, liquidation_fee_bps: 0,
    liquidation_fee_cap: 0, min_liquidation_abs: 0, max_price_move_bps_per_slot: 100, max_accrual_dt_slots: 100, max_abs_funding_e9_per_slot: 0,
    min_funding_lifetime_slots: 100, max_account_b_settlement_chunks: 1, max_bankrupt_close_chunks: 1, max_bankrupt_close_lifetime_slots: 100,
    public_b_chunk_atoms: 10_000_000_000_000_000n, maintenance_fee_per_slot: 0 };
  function initMarketData(p) {
    return Buffer.concat([Buffer.from([0]), u16(p.max_portfolio_assets), u64(p.h_min), u64(p.h_max), u64(p.initial_price), u128(p.min_nonzero_mm_req), u128(p.min_nonzero_im_req),
      u64(p.maintenance_margin_bps), u64(p.initial_margin_bps), u64(p.max_trading_fee_bps), u64(p.trade_fee_base_bps), u64(p.liquidation_fee_bps), u128(p.liquidation_fee_cap),
      u128(p.min_liquidation_abs), u64(p.max_price_move_bps_per_slot), u64(p.max_accrual_dt_slots), u64(p.max_abs_funding_e9_per_slot), u64(p.min_funding_lifetime_slots),
      u64(p.max_account_b_settlement_chunks), u64(p.max_bankrupt_close_chunks), u64(p.max_bankrupt_close_lifetime_slots), u128(p.public_b_chunk_atoms), u128(p.maintenance_fee_per_slot)]);
  }
  return {
    W, S, N, MARKET_LEN, PORTFOLIO_LEN, OFF, DEFAULT_PARAMS, budget, vaultAuthority, registry, pool, stakeVaultAuth, depositPda, mintAuthority, positionNft, extraMetas,
    createProgramAccount: (payer, account, lamports, space) => SystemProgram.createAccount({ fromPubkey: PK(payer), newAccountPubkey: PK(account), lamports, space, programId: W }),
    initMarket: (admin, market, mint, params) => wix([meta(admin, true, true), meta(market, false, true), meta(mint, false, false)], initMarketData(Object.assign({}, DEFAULT_PARAMS, params || {}))),
    setNftProgram: (admin, market) => wix([meta(admin, true, true), meta(market, false, false), meta(registry(market), false, true), meta(SystemProgram.programId, false, false)], Buffer.concat([Buffer.from([73]), N.toBuffer()])),
    initPortfolio: (owner, market, portfolio) => wix([meta(owner, true, true), meta(market, false, true), meta(portfolio, false, true)], Buffer.from([1])),
    deposit: (owner, st, portfolio, source, amount) => wix([meta(owner, true, true), meta(st.market, false, true), meta(portfolio, false, true), meta(source, false, true), meta(st.vault, false, true), meta(spl.TOKEN_PROGRAM_ID, false, false)], Buffer.concat([Buffer.from([3]), u128(amount)])),
    withdraw: (owner, st, portfolio, dest, amount) => wix([meta(owner, true, true), meta(st.market, false, true), meta(portfolio, false, true), meta(dest, false, true), meta(st.vault, false, true), meta(vaultAuthority(st.market), false, false), meta(spl.TOKEN_PROGRAM_ID, false, false)], Buffer.concat([Buffer.from([4]), u128(amount)])),
    trade: (a, pa, b, pb, market, size, price, feeBps) => wix([meta(a, true, true), meta(b, true, true), meta(market, false, true), meta(pa, false, true), meta(pb, false, true)], Buffer.concat([Buffer.from([6]), u16(0), i128(size), u64(price), u64(feeBps)])),
    initPool: (admin, st, lpMint, stakeVault, cooldown) => new TransactionInstruction({ programId: S, data: Buffer.concat([Buffer.from([0]), u64(cooldown), u64(0)]),
      keys: [meta(admin, true, true), meta(st.market, false, true), meta(pool(st.market), false, true), meta(lpMint, false, true), meta(stakeVault, false, true), meta(stakeVaultAuth(st.market), false, false), meta(st.mint, false, false), meta(W, false, false), meta(spl.TOKEN_PROGRAM_ID, false, false), meta(SystemProgram.programId, false, false), meta(web3.SYSVAR_RENT_PUBKEY, false, false)] }),
    stakeDeposit: (user, st, src, userLp, amount) => new TransactionInstruction({ programId: S, data: Buffer.concat([Buffer.from([1]), u64(amount)]),
      keys: [meta(user, true, true), meta(pool(st.market), false, true), meta(src, false, true), meta(st.stakeVault, false, true), meta(st.lpMint, false, true), meta(userLp, false, true), meta(stakeVaultAuth(st.market), false, false), meta(depositPda(st.market, user), false, true), meta(spl.TOKEN_PROGRAM_ID, false, false), meta(web3.SYSVAR_CLOCK_PUBKEY, false, false), meta(SystemProgram.programId, false, false), meta(st.market, false, false)] }),
    stakeWithdraw: (user, st, userLp, dst, lpAmount) => new TransactionInstruction({ programId: S, data: Buffer.concat([Buffer.from([2]), u64(lpAmount)]),
      keys: [meta(user, true, true), meta(pool(st.market), false, true), meta(userLp, false, true), meta(st.lpMint, false, true), meta(st.stakeVault, false, true), meta(dst, false, true), meta(stakeVaultAuth(st.market), false, false), meta(depositPda(st.market, user), false, true), meta(spl.TOKEN_PROGRAM_ID, false, false), meta(web3.SYSVAR_CLOCK_PUBKEY, false, false), meta(st.market, false, false)] }),
    mintNft: (owner, st, portfolio, marketId, nftMint) => {
      const ata = spl.getAssociatedTokenAddressSync(PK(nftMint), PK(owner), false, spl.TOKEN_2022_PROGRAM_ID);
      return new TransactionInstruction({ programId: N, data: Buffer.concat([Buffer.from([0]), u16(0)]),
        keys: [meta(owner, true, true), meta(positionNft(portfolio, marketId), false, true), meta(nftMint, true, true), meta(ata, false, true), meta(portfolio, false, true), meta(mintAuthority(), false, false), meta(spl.TOKEN_2022_PROGRAM_ID, false, false), meta(spl.ASSOCIATED_TOKEN_PROGRAM_ID, false, false), meta(SystemProgram.programId, false, false), meta(extraMetas(nftMint), false, true), meta(registry(st.market), false, false), meta(W, false, false)] });
    },
    burnNft: (owner, st, portfolio, marketId, nftMint, emergency) => {
      const ata = spl.getAssociatedTokenAddressSync(PK(nftMint), PK(owner), false, spl.TOKEN_2022_PROGRAM_ID);
      return new TransactionInstruction({ programId: N, data: Buffer.from([emergency ? 5 : 1]),
        keys: [meta(owner, true, true), meta(positionNft(portfolio, marketId), false, true), meta(nftMint, false, true), meta(ata, false, true), meta(portfolio, false, true), meta(mintAuthority(), false, false), meta(spl.TOKEN_2022_PROGRAM_ID, false, false), meta(extraMetas(nftMint), false, true), meta(registry(st.market), false, false), meta(W, false, false)] });
    },
    readPortfolio(data) {
      if (!data || data.length < 400) return null;
      const b = Buffer.from(data);
      const rd128 = (o, signed) => { let v = b.readBigUInt64LE(o) + (b.readBigUInt64LE(o + 8) << 64n); if (signed && v >= (1n << 127n)) v -= (1n << 128n); return v; };
      return { capital: rd128(OFF.capital).toString(), marketId: Number(b.readBigUInt64LE(OFF.marketId)), basis: rd128(OFF.basis, true).toString() };
    },
  };
}
module.exports = { create, MARKET_LEN, PORTFOLIO_LEN, OFF };
