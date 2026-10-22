const L=require('./lib');const {web3,spl,conn}=L;
async function ensurePortfolio(owner,st,payer){payer=payer||owner;const p=web3.Keypair.generate();const rent=await conn.getMinimumBalanceForRentExemption(L.PORTFOLIO_LEN);const M=new web3.PublicKey(st.market);
 const sig=await L.send([web3.SystemProgram.createAccount({fromPubkey:payer.publicKey,newAccountPubkey:p.publicKey,lamports:rent,space:L.PORTFOLIO_LEN,programId:L.PROGRAM}),...L.budget(),
  L.ix([{pubkey:owner.publicKey,isSigner:true,isWritable:true},{pubkey:M,isSigner:false,isWritable:true},{pubkey:p.publicKey,isSigner:false,isWritable:true}],L.encInitPortfolio())],payer===owner?[owner,p]:[payer,owner,p]);
 return {portfolio:p.publicKey,sig}}
async function deposit(owner,st,portfolio,source,amount){const M=new web3.PublicKey(st.market);
 return L.send([...L.budget(),L.ix([{pubkey:owner.publicKey,isSigner:true,isWritable:true},{pubkey:M,isSigner:false,isWritable:true},{pubkey:portfolio,isSigner:false,isWritable:true},{pubkey:source,isSigner:false,isWritable:true},{pubkey:new web3.PublicKey(st.vault),isSigner:false,isWritable:true},{pubkey:spl.TOKEN_PROGRAM_ID,isSigner:false,isWritable:false}],L.encDeposit(amount))],[owner])}
async function trade(a,pa,b,pb,st,size,price,fee){const M=new web3.PublicKey(st.market);
 return L.send([...L.budget(),L.ix([{pubkey:a.publicKey,isSigner:true,isWritable:true},{pubkey:b.publicKey,isSigner:true,isWritable:true},{pubkey:M,isSigner:false,isWritable:true},{pubkey:pa,isSigner:false,isWritable:true},{pubkey:pb,isSigner:false,isWritable:true}],L.encTrade(0,size,price,fee))],[a,b])}
module.exports={ensurePortfolio,deposit,trade};
