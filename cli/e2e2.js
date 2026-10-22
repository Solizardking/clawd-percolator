const web3=require('@solana/web3.js');const spl=require('@solana/spl-token');const fs=require('fs');const {create}=require('./perc-core');
const ids0=Object.fromEntries(fs.readFileSync(__dirname+'/../ids.txt','utf8').trim().split(' ').map(s=>s.split('=')));
const C=create({wrapper:ids0.W,stake:ids0.S,nft:ids0.N});const conn=new web3.Connection('https://rpc.clawdchain.trade','confirmed');
const kp=f=>web3.Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(f))));
const send=(ixs,s)=>web3.sendAndConfirmTransaction(conn,new web3.Transaction().add(...ixs),s,{commitment:'confirmed'});
(async()=>{const st=JSON.parse(fs.readFileSync(__dirname+'/m2-clawd.json'));const house=kp(__dirname+'/../.keys/payer.json');const mint=new web3.PublicKey(st.mint);
 const u=web3.Keypair.generate();await send([web3.SystemProgram.transfer({fromPubkey:house.publicKey,toPubkey:u.publicKey,lamports:1e9})],[house]);
 const ata=await spl.getOrCreateAssociatedTokenAccount(conn,house,mint,u.publicKey);await spl.mintTo(conn,house,mint,ata.address,house,1_000_000_000n);
 const p=web3.Keypair.generate();await send([C.createProgramAccount(u.publicKey,p.publicKey,await conn.getMinimumBalanceForRentExemption(C.PORTFOLIO_LEN),C.PORTFOLIO_LEN),...C.budget(),C.initPortfolio(u.publicKey,st.market,p.publicKey)],[u,p]);
 await send([...C.budget(),C.deposit(u.publicKey,st,p.publicKey,ata.address,500_000_000n)],[u]);
 console.log('trade',await send([...C.budget(),C.trade(u.publicKey,p.publicKey,house.publicKey,st.housePortfolio,st.market,2_000_000n,st.price,100)],[u,house]));
 const pf=C.readPortfolio((await conn.getAccountInfo(p.publicKey)).data);console.log('portfolio',JSON.stringify(pf));
 try{const nm=web3.Keypair.generate();console.log('nft mint',await send([...C.budget(),C.mintNft(u.publicKey,st,p.publicKey,pf.marketId,nm.publicKey)],[u,nm]),nm.publicKey.toBase58());
  console.log('nft burn',await send([...C.budget(),C.burnNft(u.publicKey,st,p.publicKey,pf.marketId,nm.publicKey)],[u]));}catch(e){console.log('NFT ERR',e.message,(e.logs||[]).slice(-6).join('\n'))}
 const lp=spl.getAssociatedTokenAddressSync(new web3.PublicKey(st.lpMint),u.publicKey);
 console.log('stake dep',await send([spl.createAssociatedTokenAccountIdempotentInstruction(u.publicKey,lp,u.publicKey,new web3.PublicKey(st.lpMint)),...C.budget(),C.stakeDeposit(u.publicKey,st,ata.address,lp,100_000_000n)],[u]));
 const bal=(await conn.getTokenAccountBalance(lp)).value.amount;console.log('lp',bal);
 await new Promise(r=>setTimeout(r,6000));
 try{console.log('stake wd',await send([...C.budget(),C.stakeWithdraw(u.publicKey,st,lp,ata.address,BigInt(bal))],[u]))}catch(e){console.log('WD ERR',(e.logs||[]).slice(-4).join('\n')||e.message)}
})().catch(e=>{console.error('ERR',e.message,(e.logs||[]).slice(-8).join('\n'));process.exit(1)});
