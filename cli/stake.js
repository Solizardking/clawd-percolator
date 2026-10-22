const L=require('./lib');const {web3,spl,conn}=L;const fs=require('fs');
const STAKE=new web3.PublicKey(L.ids.S);const PK=x=>new web3.PublicKey(x);
const pda=(s,...k)=>web3.PublicKey.findProgramAddressSync([Buffer.from(s),...k.map(x=>x.toBuffer())],STAKE)[0];
const u64=n=>{const b=Buffer.alloc(8);b.writeBigUInt64LE(BigInt(n));return b};
const m=(pubkey,isSigner,isWritable)=>({pubkey,isSigner,isWritable});
function poolKeys(st){const slab=PK(st.market);const pool=pda('stake_pool',slab);const va=pda('vault_auth',pool);return {slab,pool,va}}
async function initPool(st,admin){const {slab,pool,va}=poolKeys(st);const lp=web3.Keypair.generate(),vault=web3.Keypair.generate();
 const rm=await conn.getMinimumBalanceForRentExemption(82),ra=await conn.getMinimumBalanceForRentExemption(165);
 const ixs=[web3.SystemProgram.createAccount({fromPubkey:admin.publicKey,newAccountPubkey:lp.publicKey,lamports:rm,space:82,programId:spl.TOKEN_PROGRAM_ID}),
  web3.SystemProgram.createAccount({fromPubkey:admin.publicKey,newAccountPubkey:vault.publicKey,lamports:ra,space:165,programId:spl.TOKEN_PROGRAM_ID}),...L.budget(),
  new web3.TransactionInstruction({programId:STAKE,data:Buffer.concat([Buffer.from([0]),u64(10),u64(0)]),keys:[m(admin.publicKey,true,true),m(slab,false,true),m(pool,false,true),m(lp.publicKey,false,true),m(vault.publicKey,false,true),m(va,false,false),m(PK(st.mint),false,false),m(L.PROGRAM,false,false),m(spl.TOKEN_PROGRAM_ID,false,false),m(web3.SystemProgram.programId,false,false),m(web3.SYSVAR_RENT_PUBKEY,false,false)]})];
 const sig=await L.send(ixs,[admin,lp,vault]);return {sig,pool:pool.toBase58(),lpMint:lp.publicKey.toBase58(),stakeVault:vault.publicKey.toBase58()}}
function depositIx(st,user,src,userLp,amount){const {slab,pool,va}=poolKeys(st);const dep=pda('stake_deposit',pool,user);
 return new web3.TransactionInstruction({programId:STAKE,data:Buffer.concat([Buffer.from([1]),u64(amount)]),keys:[m(user,true,true),m(pool,false,true),m(src,false,true),m(PK(st.stakeVault),false,true),m(PK(st.lpMint),false,true),m(userLp,false,true),m(va,false,false),m(dep,false,true),m(spl.TOKEN_PROGRAM_ID,false,false),m(web3.SYSVAR_CLOCK_PUBKEY,false,false),m(web3.SystemProgram.programId,false,false),m(slab,false,false)]})}
function withdrawIx(st,user,userLp,dst,lpAmount){const {slab,pool,va}=poolKeys(st);const dep=pda('stake_deposit',pool,user);
 return new web3.TransactionInstruction({programId:STAKE,data:Buffer.concat([Buffer.from([2]),u64(lpAmount)]),keys:[m(user,true,true),m(pool,false,true),m(userLp,false,true),m(PK(st.lpMint),false,true),m(PK(st.stakeVault),false,true),m(dst,false,true),m(va,false,false),m(dep,false,true),m(spl.TOKEN_PROGRAM_ID,false,false),m(web3.SYSVAR_CLOCK_PUBKEY,false,false),m(slab,false,false)]})}
module.exports={initPool,depositIx,withdrawIx,poolKeys,STAKE};
if(require.main===module)(async()=>{const f=__dirname+'/market-clawd.json';const st=JSON.parse(fs.readFileSync(f));const house=L.kp(__dirname+'/../.keys/payer.json');
 const r=await initPool(st,house);console.log('initPool',JSON.stringify(r));Object.assign(st,r);fs.writeFileSync(f,JSON.stringify(st,null,1));
 const ata=spl.getAssociatedTokenAddressSync(PK(st.mint),house.publicKey);const lpAta=await spl.getOrCreateAssociatedTokenAccount(conn,house,PK(st.lpMint),house.publicKey);
 console.log('stake deposit',await L.send([...L.budget(),depositIx(st,house.publicKey,ata,lpAta.address,1_000_000_000)],[house]));
})().catch(e=>{console.error('ERR',e.message,(e.logs||[]).slice(-10).join('\n'));process.exit(1)});
