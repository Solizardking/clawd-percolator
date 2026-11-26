/-
  Percolator.Matcher
  Minimal, formally verified external passive-LP AMM / Spread Matcher program that
  composes with `percolator-lean` via CPI (`sol_invoke_signed_c`).

  Zero changes to core `percolator-lean`:
  - Owns an LP `Portfolio` via PDA `["lpau", config.key, bump]`.
  - Any trader executes single-signer `Fill` (open or close!) against resting LP liquidity
    at a deterministic, mark-bounded quote (`halfSpreadBps + qty * impactBps`).
  - LP owner updates parameters (`UpdateMatcher`) or withdraws collateral (`WithdrawLp`).
-/
import LeanSVM
import Percolator.Layout

namespace Percolator.Matcher

open LeanSVM
open LeanSVM.Input
open LeanSVM.Math
open LeanSVM.CPI
open Percolator

def MAGIC_MATCHER : UInt32 := 0x4843544D -- "MTCH"
def TAG_LP_AUTH   : UInt32 := 0x7561706C -- "lpau"

namespace MErr
  def paused       : UInt64 := 200
  def qtyTooLarge  : UInt64 := 201
  def slippage     : UInt64 := 202
  def progMismatch : UInt64 := 203
  def pflMismatch  : UInt64 := 204
end MErr

namespace CfgOff
  def size          : UInt64 := 128
  def magic         : UInt64 := 0   -- u32 ("MTCH")
  def pdaBump       : UInt64 := 4   -- u8
  def paused        : UInt64 := 5   -- u8
  def owner         : UInt64 := 8   -- Pubkey (32)
  def percProg      : UInt64 := 40  -- Pubkey (32)
  def lpPortfolio   : UInt64 := 72  -- Pubkey (32)
  def halfSpreadBps : UInt64 := 104 -- u64
  def impactBps     : UInt64 := 112 -- u64 (per 1.0 E6 unit)
  def maxFillQty    : UInt64 := 120 -- u64
end CfgOff

/-! ### Pure Quote Specification & Formal Proofs (0 `sorry`, 0 custom axioms) -/

def clampSpreadBpsNat (halfSpread impact qty maxMove : Nat) : Nat :=
  min (halfSpread + (qty * impact) / 1_000_000) maxMove

def quoteAskNat (markP halfSpread impact qty maxMove : Nat) : Nat :=
  markP + (markP * clampSpreadBpsNat halfSpread impact qty maxMove) / 10_000

def quoteBidNat (markP halfSpread impact qty maxMove : Nat) : Nat :=
  let delta := (markP * clampSpreadBpsNat halfSpread impact qty maxMove) / 10_000
  if markP > delta then markP - delta else 1

theorem quoteAsk_ge_mark (markP halfSpread impact qty maxMove : Nat) :
    quoteAskNat markP halfSpread impact qty maxMove ≥ markP := by
  unfold quoteAskNat; omega

theorem quoteBid_le_mark (markP halfSpread impact qty maxMove : Nat) (hm : markP ≥ 1) :
    quoteBidNat markP halfSpread impact qty maxMove ≤ markP := by
  unfold quoteBidNat; dsimp only; split <;> omega

theorem quoteAsk_within_maxMove (markP halfSpread impact qty maxMove : Nat) :
    quoteAskNat markP halfSpread impact qty maxMove - markP ≤ (markP * maxMove) / 10_000 := by
  unfold quoteAskNat clampSpreadBpsNat
  have hle : min (halfSpread + (qty * impact) / 1_000_000) maxMove ≤ maxMove := Nat.min_le_right _ _
  have hmul : markP * min (halfSpread + (qty * impact) / 1_000_000) maxMove ≤ markP * maxMove :=
    Nat.mul_le_mul_left markP hle
  have hdiv := Nat.div_le_div_right (c := 10_000) hmul
  omega

theorem quoteBid_within_maxMove (markP halfSpread impact qty maxMove : Nat) (hm : markP ≥ 1) :
    markP - quoteBidNat markP halfSpread impact qty maxMove ≤ (markP * maxMove) / 10_000 := by
  unfold quoteBidNat clampSpreadBpsNat; dsimp only
  have hle : min (halfSpread + (qty * impact) / 1_000_000) maxMove ≤ maxMove := Nat.min_le_right _ _
  have hmul : markP * min (halfSpread + (qty * impact) / 1_000_000) maxMove ≤ markP * maxMove :=
    Nat.mul_le_mul_left markP hle
  have hdiv := Nat.div_le_div_right (c := 10_000) hmul
  split <;> omega

/-! ### CPI Signer Helper -/

def writeMatcherPdaSeeds (sbuf : Ptr) (cfgKeyPtr : Ptr) (bump : UInt8) : SolanaM Ptr := do
  let seedsArr   := sbuf + 8
  let signersArr := sbuf + 56
  storeU32 sbuf 0 TAG_LP_AUTH
  storeU8  sbuf 4 bump
  storeU64 seedsArr 0  sbuf;       storeU64 seedsArr 8  4
  storeU64 seedsArr 16 cfgKeyPtr;  storeU64 seedsArr 24 32
  storeU64 seedsArr 32 (sbuf + 4); storeU64 seedsArr 40 1
  storeU64 signersArr 0 seedsArr
  storeU64 signersArr 8 3
  return signersArr

def zeroMatcherWords (base off nWords : UInt64) : SolanaM Unit := do
  if h : nWords == 0 then return ()
  else
    storeU64 base off 0
    zeroMatcherWords base (off + 8) (nWords - 1)
termination_by nWords.toNat
decreasing_by
  have hne : nWords.toNat ≠ 0 := by
    intro hz
    have h0 : nWords = 0 := UInt64.toNat_inj.mp (by rw [hz]; rfl)
    exact h (by rw [h0]; rfl)
  have h1 : (1 : UInt64).toNat = 1 := rfl
  change (nWords - 1).toNat < nWords.toNat
  rw [UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)), h1]
  omega

def checkCfg (cfg : AccountView) (progId : Ptr) : SolanaM Unit := do
  SolanaM.require (← pubkeyEq (AccountView.ownerPtr cfg) progId) Err.illegalOwner
  SolanaM.require ((← AccountView.dataLen cfg) == CfgOff.size) Err.invalidAccountData
  SolanaM.require ((← loadU32 (AccountView.dataPtr cfg) CfgOff.magic) == MAGIC_MATCHER) PErr.invalidMagic

/-! ### 4 Minimal Instructions -/

/-- Op 0: Initialize `MatcherConfig` and CPI `percolator::InitPortfolio` for the PDA-owned LP portfolio. -/
def handleInitMatcher (input : Ptr) : SolanaM Unit := do
  let (cfg, owner, dex, lpPfl, lpAuthPda, percProg, ix, progId) ← parse6 input
  SolanaM.require (ix.len >= 28) Err.invalidInstructionData
  SolanaM.require (← AccountView.isSigner owner) Err.missingRequiredSignature
  SolanaM.require (← AccountView.isWritable cfg) Err.invalidAccountData
  SolanaM.require (← pubkeyEq (AccountView.ownerPtr cfg) progId) Err.illegalOwner
  SolanaM.require ((← AccountView.dataLen cfg) == CfgOff.size) Err.invalidAccountData
  SolanaM.require ((← loadU32 (AccountView.dataPtr cfg) 0) == 0) Err.accountAlreadyInitialized

  let pdaBump       ← loadU8  ix.ptr 1
  let halfSpreadBps ← loadU64 ix.ptr 4
  let impactBps     ← loadU64 ix.ptr 12
  let maxFillQty    ← loadU64 ix.ptr 20
  SolanaM.require (halfSpreadBps <= BPS_DEN && maxFillQty > 0) PErr.invalidParams

  let cData := AccountView.dataPtr cfg
  zeroMatcherWords cData 0 (CfgOff.size >>> 3)
  storeU32 cData CfgOff.magic MAGIC_MATCHER
  storeU8  cData CfgOff.pdaBump pdaBump
  storeU8  cData CfgOff.paused 0
  pubkeyCopy (cData + CfgOff.owner)       (AccountView.keyPtr owner)
  pubkeyCopy (cData + CfgOff.percProg)    (AccountView.keyPtr percProg)
  pubkeyCopy (cData + CfgOff.lpPortfolio) (AccountView.keyPtr lpPfl)
  storeU64 cData CfgOff.halfSpreadBps halfSpreadBps
  storeU64 cData CfgOff.impactBps     impactBps
  storeU64 cData CfgOff.maxFillQty    maxFillQty

  -- CPI `percolator::InitPortfolio(dex[W], lpPfl[W], lpAuthPda[S])` signed by LP PDA!
  let buf ← stackAlloc 336
  let metas := buf + 40
  let infos := buf + 88
  let data  := buf + 256
  let sbuf  := buf + 264
  storeU8 data 0 IxOp.initPortfolio
  writeMeta metas        dex       1 0
  writeMeta (metas + 16) lpPfl     1 0
  writeMeta (metas + 32) lpAuthPda 0 1
  writeAccountInfo infos         dex
  writeAccountInfo (infos + 56)  lpPfl
  writeAccountInfo (infos + 112) lpAuthPda
  writeInstruction buf (AccountView.keyPtr percProg) metas 3 data 1
  let signersArr ← writeMatcherPdaSeeds sbuf (AccountView.keyPtr cfg) pdaBump
  let rc ← sysInvokeSignedC buf infos 3 signersArr 1
  SolanaM.require (rc == 0) rc

/-- Op 1: LP owner updates quote parameters or pauses quoting. -/
def handleUpdateMatcher (input : Ptr) : SolanaM Unit := do
  let (cfg, owner, ix, progId) ← parse2 input
  SolanaM.require (ix.len >= 28) Err.invalidInstructionData
  SolanaM.require (← AccountView.isSigner owner) Err.missingRequiredSignature
  SolanaM.require (← AccountView.isWritable cfg) Err.invalidAccountData
  checkCfg cfg progId
  let cData := AccountView.dataPtr cfg
  SolanaM.require (← pubkeyEq (AccountView.keyPtr owner) (cData + CfgOff.owner)) PErr.ownerMismatch
  let paused        ← loadU8  ix.ptr 1
  let halfSpreadBps ← loadU64 ix.ptr 4
  let impactBps     ← loadU64 ix.ptr 12
  let maxFillQty    ← loadU64 ix.ptr 20
  SolanaM.require (halfSpreadBps <= BPS_DEN && maxFillQty > 0) PErr.invalidParams
  storeU8  cData CfgOff.paused        paused
  storeU64 cData CfgOff.halfSpreadBps halfSpreadBps
  storeU64 cData CfgOff.impactBps     impactBps
  storeU64 cData CfgOff.maxFillQty    maxFillQty

/-- Op 2: Permissionless single-signer taker `Fill` (open OR close) against passive LP portfolio! -/
def handleFill (input : Ptr) : SolanaM Unit := do
  let (cfg, dex, mkt, lpPfl, lpAuthPda, takerPfl, takerOwner, percProg, ix, progId) ← parse8 input
  SolanaM.require (ix.len >= 19) Err.invalidInstructionData
  SolanaM.require (← AccountView.isSigner takerOwner) Err.missingRequiredSignature
  checkCfg cfg progId
  let cData := AccountView.dataPtr cfg
  SolanaM.require ((← loadU8 cData CfgOff.paused) == 0) MErr.paused
  SolanaM.require (← pubkeyEq (AccountView.keyPtr percProg) (cData + CfgOff.percProg))    MErr.progMismatch
  SolanaM.require (← pubkeyEq (AccountView.keyPtr lpPfl)    (cData + CfgOff.lpPortfolio)) MErr.pflMismatch

  -- Validate `mkt` belongs to `percolatorProg` before reading `markPrice` & `maxMoveBps`
  SolanaM.require (← pubkeyEq (AccountView.ownerPtr mkt) (AccountView.keyPtr percProg)) Err.illegalOwner
  SolanaM.require ((← AccountView.dataLen mkt) >= MktOff.size) Err.accountDataTooSmall
  let mData := AccountView.dataPtr mkt
  SolanaM.require ((← loadU32 mData MktOff.magic) == MAGIC_MARKET) PErr.invalidMagic

  let mode        ← loadU8  ix.ptr 1
  let takerIsLong ← loadU8  ix.ptr 2
  let qty         ← loadU64 ix.ptr 3
  let limitPrice  ← loadU64 ix.ptr 11
  SolanaM.require (qty > 0 && qty <= (← loadU64 cData CfgOff.maxFillQty)) MErr.qtyTooLarge

  let markP   ← loadU64 mData MktOff.markPrice
  let maxMove ← loadU64 mData MktOff.maxMoveBps
  let halfSp  ← loadU64 cData CfgOff.halfSpreadBps
  let impBps  ← loadU64 cData CfgOff.impactBps
  let imp     ← mulDiv64 qty impBps POS_SCALE
  let rawBps  ← checkedAdd64 halfSp imp
  let effBps  := if rawBps > maxMove then maxMove else rawBps
  let delta   ← mulDiv64 markP effBps BPS_DEN

  let execPrice ← if takerIsLong != 0 then do
      let ask ← checkedAdd64 markP delta
      if limitPrice != 0 then SolanaM.require (ask <= limitPrice) MErr.slippage
      pure ask
    else do
      let bid := if markP > delta then markP - delta else 1
      if limitPrice != 0 then SolanaM.require (bid >= limitPrice) MErr.slippage
      pure bid

  let pflL   := if takerIsLong != 0 then takerPfl   else lpPfl
  let ownerL := if takerIsLong != 0 then takerOwner else lpAuthPda
  let pflS   := if takerIsLong != 0 then lpPfl      else takerPfl
  let ownerS := if takerIsLong != 0 then lpAuthPda  else takerOwner

  -- CPI `percolator::Trade(dex[W], mkt[W], pflL[W], ownerL[S], pflS[W], ownerS[S])`
  let buf ← stackAlloc 568
  let metas := buf + 40
  let infos := buf + 136
  let data  := buf + 472
  let sbuf  := buf + 496
  storeU8  data 0  IxOp.trade
  storeU8  data 1  mode
  storeU64 data 2  qty
  storeU64 data 10 execPrice
  writeMeta metas        dex    1 0
  writeMeta (metas + 16) mkt    1 0
  writeMeta (metas + 32) pflL   1 0
  writeMeta (metas + 48) ownerL 0 1
  writeMeta (metas + 64) pflS   1 0
  writeMeta (metas + 80) ownerS 0 1
  writeAccountInfo infos         dex
  writeAccountInfo (infos + 56)  mkt
  writeAccountInfo (infos + 112) pflL
  writeAccountInfo (infos + 168) ownerL
  writeAccountInfo (infos + 224) pflS
  writeAccountInfo (infos + 280) ownerS
  writeInstruction buf (AccountView.keyPtr percProg) metas 6 data 18
  let bump ← loadU8 cData CfgOff.pdaBump
  let signersArr ← writeMatcherPdaSeeds sbuf (AccountView.keyPtr cfg) bump
  let rc ← sysInvokeSignedC buf infos 6 signersArr 1
  SolanaM.require (rc == 0) rc

/-- Op 3: LP owner withdraws collateral from PDA-owned LP portfolio via `percolator::WithdrawCollateral`. -/
def handleWithdrawLp (input : Ptr) : SolanaM Unit := do
  let (n, cfg, ix, progId) ← parseIxOnly input
  SolanaM.require (n >= 10) Err.notEnoughAccountKeys
  SolanaM.require (ix.len >= 9) Err.invalidInstructionData
  let owner     ← AccountView.nextCursor cfg
  let dex       ← AccountView.nextCursor owner
  let lpPfl     ← AccountView.nextCursor dex
  let lpAuthPda ← AccountView.nextCursor lpPfl
  let destTok   ← AccountView.nextCursor lpAuthPda
  let mint      ← AccountView.nextCursor destTok
  let vaultPda  ← AccountView.nextCursor mint
  let tokProg   ← AccountView.nextCursor vaultPda
  let percProg  ← AccountView.nextCursor tokProg

  SolanaM.require (← AccountView.isSigner owner) Err.missingRequiredSignature
  checkCfg cfg progId
  let cData := AccountView.dataPtr cfg
  SolanaM.require (← pubkeyEq (AccountView.keyPtr owner)    (cData + CfgOff.owner))       PErr.ownerMismatch
  SolanaM.require (← pubkeyEq (AccountView.keyPtr percProg) (cData + CfgOff.percProg))    MErr.progMismatch
  SolanaM.require (← pubkeyEq (AccountView.keyPtr lpPfl)    (cData + CfgOff.lpPortfolio)) MErr.pflMismatch

  let amount ← loadU64 ix.ptr 1
  let buf ← stackAlloc 632
  let metas := buf + 40
  let infos := buf + 152
  let data  := buf + 544
  let sbuf  := buf + 560
  storeU8  data 0 IxOp.withdrawCollateral
  storeU64 data 1 amount
  writeMeta metas        dex       1 0
  writeMeta (metas + 16) lpPfl     1 0
  writeMeta (metas + 32) lpAuthPda 0 1
  writeMeta (metas + 48) destTok   1 0
  writeMeta (metas + 64) mint      0 0
  writeMeta (metas + 80) vaultPda  1 0
  writeMeta (metas + 96) tokProg   0 0
  writeAccountInfo infos         dex
  writeAccountInfo (infos + 56)  lpPfl
  writeAccountInfo (infos + 112) lpAuthPda
  writeAccountInfo (infos + 168) destTok
  writeAccountInfo (infos + 224) mint
  writeAccountInfo (infos + 280) vaultPda
  writeAccountInfo (infos + 336) tokProg
  writeInstruction buf (AccountView.keyPtr percProg) metas 7 data 9
  let bump ← loadU8 cData CfgOff.pdaBump
  let signersArr ← writeMatcherPdaSeeds sbuf (AccountView.keyPtr cfg) bump
  let rc ← sysInvokeSignedC buf infos 7 signersArr 1
  SolanaM.require (rc == 0) rc

def dispatch (input : Ptr) : SolanaM Unit := do
  let (_, _, ix, _) ← parseIxOnly input
  SolanaM.require (ix.len >= 1) Err.invalidInstructionData
  let op ← loadU8 ix.ptr 0
  if op == 0 then handleInitMatcher input
  else if op == 1 then handleUpdateMatcher input
  else if op == 2 then handleFill input
  else if op == 3 then handleWithdrawLp input
  else SolanaM.fail PErr.invalidInstruction

def entrypoint (input : Ptr) (s : SvmState) : UInt64 × SvmState :=
  SolanaM.runEntry (dispatch input) s

end Percolator.Matcher
