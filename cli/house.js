const L=require('./lib');const {web3,spl,conn}=L;const P=require('./portfolio');const fs=require('fs');
(async()=>{const name=process.argv[2]||'clawd';const f=__dirname+'/market-'+name+'.json';const st=JSON.parse(fs.readFileSync(f));const house=L.kp(__dirname+'/../.keys/payer.json');const mint=new web3.PublicKey(st.mint);
 const r=await P.ensurePortfolio(house,st);console.log('house portfolio',r.portfolio.toBase58(),r.sig);
 const ata=await spl.getOrCreateAssociatedTokenAccount(conn,house,mint,house.publicKey);await spl.mintTo(conn,house,mint,ata.address,house,1_000_000_000_000_000n);
 const d=await P.deposit(house,st,r.portfolio,ata.address,100_000_000_000_000n);console.log('house deposit',d);
 st.housePortfolio=r.portfolio.toBase58();fs.writeFileSync(f,JSON.stringify(st,null,1));})().catch(e=>{console.error('ERR',e.message,e.logs||'');process.exit(1)});
