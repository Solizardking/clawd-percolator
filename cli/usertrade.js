const L=require('./lib');const {web3,spl,conn}=L;const P=require('./portfolio');const fs=require('fs');
(async()=>{const st=JSON.parse(fs.readFileSync(__dirname+'/market-clawd.json'));const house=L.kp(__dirname+'/../.keys/payer.json');const mint=new web3.PublicKey(st.mint);
 const u=web3.Keypair.generate();console.log('user',u.publicKey.toBase58());
 await L.send([web3.SystemProgram.transfer({fromPubkey:house.publicKey,toPubkey:u.publicKey,lamports:1_000_000_000})],[house]);
 const ata=await spl.getOrCreateAssociatedTokenAccount(conn,house,mint,u.publicKey);await spl.mintTo(conn,house,mint,ata.address,house,1_000_000_000n);
 const r=await P.ensurePortfolio(u,st);console.log('user portfolio',r.portfolio.toBase58());
 console.log('deposit',await P.deposit(u,st,r.portfolio,ata.address,1_000_000_000n));
 const sig=await P.trade(u,r.portfolio,house,new web3.PublicKey(st.housePortfolio),st,5_000_000n,st.initPrice,100);console.log('TRADE',sig);
})().catch(e=>{console.error('ERR',e.message,(e.logs||[]).slice(-8).join('\n'));process.exit(1)});
