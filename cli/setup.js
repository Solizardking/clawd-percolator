const L=require('./lib');const {web3,spl,conn}=L;const fs=require('fs');
(async()=>{const house=L.kp(__dirname+'/../.keys/payer.json');
const name=process.argv[2]||'clawd';
const mint=await spl.createMint(conn,house,house.publicKey,null,6);console.log('mint',mint.toBase58());
const market=web3.Keypair.generate();
const rent=await conn.getMinimumBalanceForRentExemption(L.MARKET_LEN);
const P={max_portfolio_assets:1,h_min:0,h_max:10,initial_price:100_000_000,min_nonzero_mm_req:1,min_nonzero_im_req:2,maintenance_margin_bps:10000,initial_margin_bps:10000,max_trading_fee_bps:1000,trade_fee_base_bps:0,liquidation_fee_bps:0,liquidation_fee_cap:0,min_liquidation_abs:0,max_price_move_bps_per_slot:100,max_accrual_dt_slots:100,max_abs_funding_e9_per_slot:0,min_funding_lifetime_slots:100,max_account_b_settlement_chunks:1,max_bankrupt_close_chunks:1,max_bankrupt_close_lifetime_slots:100,public_b_chunk_atoms:10_000_000_000_000_000n,maintenance_fee_per_slot:0};
let sig=await L.send([web3.SystemProgram.createAccount({fromPubkey:house.publicKey,newAccountPubkey:market.publicKey,lamports:rent,space:L.MARKET_LEN,programId:L.PROGRAM}),...L.budget(),
 L.ix([{pubkey:house.publicKey,isSigner:true,isWritable:true},{pubkey:market.publicKey,isSigner:false,isWritable:true},{pubkey:mint,isSigner:false,isWritable:false}],L.encInitMarket(P))],[house,market]);
console.log('init market',market.publicKey.toBase58(),sig);
const va=L.vaultAuthority(market.publicKey);const vault=await spl.getOrCreateAssociatedTokenAccount(conn,house,mint,va,true);console.log('vault',vault.address.toBase58());
const st={name,market:market.publicKey.toBase58(),mint:mint.toBase58(),vault:vault.address.toBase58(),house:house.publicKey.toBase58(),initPrice:P.initial_price,initSig:sig};
fs.writeFileSync(__dirname+'/market-'+name+'.json',JSON.stringify(st,null,1));console.log(JSON.stringify(st));
})().catch(e=>{console.error('ERR',e.message,e.logs||'');process.exit(1)});
