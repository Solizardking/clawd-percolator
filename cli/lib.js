const web3=require('@solana/web3.js');const spl=require('@solana/spl-token');const fs=require('fs');
const {Connection,Keypair,PublicKey,SystemProgram,Transaction,TransactionInstruction,ComputeBudgetProgram,sendAndConfirmTransaction}=web3;
const RPC=process.env.RPC||'https://rpc.clawdchain.trade';
const conn=new Connection(RPC,'confirmed');
const ids=Object.fromEntries(fs.readFileSync(__dirname+'/../ids.txt','utf8').trim().split(' ').map(s=>s.split('=')));
const PROGRAM=new PublicKey(ids.W);
const MARKET_LEN=3163, PORTFOLIO_LEN=9539;
const kp=f=>Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(f))));
const u16=n=>{const b=Buffer.alloc(2);b.writeUInt16LE(n);return b};
const u64=n=>{const b=Buffer.alloc(8);b.writeBigUInt64LE(BigInt(n));return b};
const u128=n=>{n=BigInt(n);const b=Buffer.alloc(16);b.writeBigUInt64LE(n&((1n<<64n)-1n));b.writeBigUInt64LE(n>>64n,8);return b};
const i128=n=>{n=BigInt(n);if(n<0n)n=(1n<<128n)+n;return u128(n)};
function encInitMarket(p){return Buffer.concat([Buffer.from([0]),u16(p.max_portfolio_assets),u64(p.h_min),u64(p.h_max),u64(p.initial_price),u128(p.min_nonzero_mm_req),u128(p.min_nonzero_im_req),u64(p.maintenance_margin_bps),u64(p.initial_margin_bps),u64(p.max_trading_fee_bps),u64(p.trade_fee_base_bps),u64(p.liquidation_fee_bps),u128(p.liquidation_fee_cap),u128(p.min_liquidation_abs),u64(p.max_price_move_bps_per_slot),u64(p.max_accrual_dt_slots),u64(p.max_abs_funding_e9_per_slot),u64(p.min_funding_lifetime_slots),u64(p.max_account_b_settlement_chunks),u64(p.max_bankrupt_close_chunks),u64(p.max_bankrupt_close_lifetime_slots),u128(p.public_b_chunk_atoms),u128(p.maintenance_fee_per_slot)])}
const encInitPortfolio=()=>Buffer.from([1]);
const encDeposit=a=>Buffer.concat([Buffer.from([3]),u128(a)]);
const encWithdraw=a=>Buffer.concat([Buffer.from([4]),u128(a)]);
const encTrade=(asset,size,price,fee)=>Buffer.concat([Buffer.from([6]),u16(asset),i128(size),u64(price),u64(fee)]);
const budget=()=>[ComputeBudgetProgram.requestHeapFrame({bytes:128*1024}),ComputeBudgetProgram.setComputeUnitLimit({units:1_400_000})];
const ix=(keys,data)=>new TransactionInstruction({programId:PROGRAM,keys,data});
const vaultAuthority=m=>PublicKey.findProgramAddressSync([Buffer.from('vault'),m.toBuffer()],PROGRAM)[0];
async function send(ixs,signers){const tx=new Transaction().add(...ixs);return sendAndConfirmTransaction(conn,tx,signers,{commitment:'confirmed',skipPreflight:false})}
module.exports={web3,spl,conn,PROGRAM,MARKET_LEN,PORTFOLIO_LEN,kp,encInitMarket,encInitPortfolio,encDeposit,encWithdraw,encTrade,budget,ix,vaultAuthority,send,ids};
