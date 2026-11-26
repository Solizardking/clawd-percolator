/-
  Percolator.Program
  Unified, zero-separation Lean 4 → Solana sBPF perpetual DEX program (v2).
  Realizable Shared Cross-Margin (full cross-margin capital efficiency on healthy markets)
  + Provable Per-Market Risk Isolation (`cross_margin_isolation_envelope`)
  + Exact Global Stock Equality (`C_tot + I_tot + R_tot + S_tot = V`).

  Written in clean, idiomatic Lean 4 with ZERO manual `@[noinline]` annotations or
  hand-rolled optimizations: `lean-svm` automatically handles smart outlining, constant
  rematerialization, binary-search switch dispatch, loop-weighted register allocation,
  and basic-block value-tracking peephole folding.
-/
import LeanSVM
import Percolator.Layout

namespace Percolator.Program

open LeanSVM
open LeanSVM.Input
open LeanSVM.Math
open LeanSVM.CPI
open Percolator

/-! ### Core Account, PDA, Token-Program & Checked-Mutation Helpers -/

def requireSigner (acc : AccountView) : SolanaM Unit := do
  SolanaM.require (← AccountView.isSigner acc) Err.missingRequiredSignature

def requireWritable (acc : AccountView) : SolanaM Unit := do
  SolanaM.require (← AccountView.isWritable acc) Err.invalidAccountData

def requireKeyEq (a b : Ptr) (err : UInt64) : SolanaM Unit := do
  SolanaM.require (← pubkeyEq a b) err

def requireKeyNe (a b : Ptr) (err : UInt64) : SolanaM Unit := do
  SolanaM.require (!(← pubkeyEq a b)) err

def pubkeyIsZero (p : Ptr) : SolanaM Bool := do
  let w0 ← loadU64 p 0
  let w1 ← loadU64 p 8
  let w2 ← loadU64 p 16
  let w3 ← loadU64 p 24
  return (w0 ||| w1 ||| w2 ||| w3) == 0

/-- Pin every SPL Token CPI strictly to canonical `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`. -/
def checkTokenProg (tokProg : AccountView) : SolanaM Unit := do
  let k := AccountView.keyPtr tokProg
  SolanaM.require ((← loadU64 k 0)  == SPL_TOK_W0) PErr.invalidTokenProg
  SolanaM.require ((← loadU64 k 8)  == SPL_TOK_W1) PErr.invalidTokenProg
  SolanaM.require ((← loadU64 k 16) == SPL_TOK_W2) PErr.invalidTokenProg
  SolanaM.require ((← loadU64 k 24) == SPL_TOK_W3) PErr.invalidTokenProg

/-- Pin SystemProgram CPIs strictly to canonical `11111111111111111111111111111111` (all zeros). -/
def checkSysProg (sysProg : AccountView) : SolanaM Unit := do
  SolanaM.require (← pubkeyIsZero (AccountView.keyPtr sysProg)) PErr.invalidSysProg

def getClockSlot : SolanaM UInt64 := do
  let buf ← stackAlloc 40
  let rc ← sysGetClock buf
  SolanaM.require (rc == 0) Err.invalidAccountData
  loadU64 buf 0

/-- Zero `nWords` consecutive 8-byte words starting at `base + off`.
    Used by every `Init*` handler, `freeLegIfEmpty`, and `handleCloseAccount` so NO account
    can ever retain dirty or cross-type residual bytes across reuse or same-tx revival attempts. -/
def zeroWordsLoop (base off nWords : UInt64) : SolanaM Unit := do
  if h : nWords == 0 then
    return ()
  else
    storeU64 base off 0
    zeroWordsLoop base (off + 8) (nWords - 1)
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

def checkOwnedMagic (acc : AccountView) (progId : Ptr) (exactLen : UInt64) (magic : UInt32) : SolanaM Unit := do
  requireKeyEq (AccountView.ownerPtr acc) progId Err.illegalOwner
  SolanaM.require ((← AccountView.dataLen acc) == exactLen) Err.invalidAccountData
  SolanaM.require ((← loadU32 (AccountView.dataPtr acc) 0) == magic) PErr.invalidMagic

def checkFreshAccount (acc : AccountView) (progId : Ptr) (exactLen : UInt64) : SolanaM Unit := do
  requireWritable acc
  requireKeyEq (AccountView.ownerPtr acc) progId Err.illegalOwner
  SolanaM.require ((← AccountView.dataLen acc) == exactLen) Err.invalidAccountData
  SolanaM.require ((← loadU32 (AccountView.dataPtr acc) 0) == 0) Err.accountAlreadyInitialized

def addU64At (base off delta : UInt64) : SolanaM Unit := do
  storeU64 base off (← checkedAdd64 (← loadU64 base off) delta)

def subU64At (base off delta : UInt64) : SolanaM Unit := do
  storeU64 base off (← checkedSub64 (← loadU64 base off) delta)

def loadOptU8 (ixPtr ixLen off : UInt64) : SolanaM UInt8 := do
  if ixLen > off then loadU8 ixPtr off else return 0

def loadOptU64 (ixPtr ixLen off : UInt64) : SolanaM UInt64 := do
  if ixLen >= off + 8 then loadU64 ixPtr off else return 0

def checkOracleFresh (mData : Ptr) (curSlot : UInt64) : SolanaM Unit := do
  let lastSlot ← loadU64 mData MktOff.lastOracleSlot
  let maxStale ← loadU64 mData MktOff.maxStaleness
  let age := if curSlot >= lastSlot then curSlot - lastSlot else 0
  SolanaM.require (age <= maxStale) PErr.oracleStale

def checkDexVaultAndMint (dexData : Ptr) (mint vaultPda tokProg : AccountView) : SolanaM Unit := do
  checkTokenProg tokProg
  requireKeyEq (AccountView.keyPtr mint)     (dexData + DexOff.quoteMint) PErr.mintMismatch
  requireKeyEq (AccountView.keyPtr vaultPda) (dexData + DexOff.vaultPda)  PErr.vaultMismatch

/-- Sweep any floor-division dust from `m.realizedPool` once `m.totalPosPnl == 0`:
    credits `m.insurance` when LP shares exist, else `dex.totalSurplus`.
    On `STATUS_RESOLVED` markets with `totalInsShares == 0`, also sweeps orphan donated
    `m.insurance` into `dex.totalSurplus` so donated-only markets can cleanly tombstone! -/
def sweepResolvedDust (dData mData : Ptr) : SolanaM Unit := do
  if (← loadU64 mData MktOff.totalPosPnl) == 0 then
    let dust ← loadU64 mData MktOff.realizedPool
    if dust > 0 then
      storeU64 mData MktOff.realizedPool 0
      subU64At dData DexOff.totalResidual dust
      if (← loadU64 mData MktOff.totalInsShares) > 0 then
        addU64At mData MktOff.insurance dust
        addU64At dData DexOff.totalInsurance dust
      else
        addU64At dData DexOff.totalSurplus dust
    if (← loadU8 mData MktOff.status) == STATUS_RESOLVED &&
       (← loadU64 mData MktOff.totalInsShares) == 0 then
      let orphanIns ← loadU64 mData MktOff.insurance
      if orphanIns > 0 then
        storeU64 mData MktOff.insurance 0
        subU64At dData DexOff.totalInsurance orphanIns
        addU64At dData DexOff.totalSurplus orphanIns

/-! ### Portfolio Multi-Leg Slot Helpers (Loop over `MAX_LEGS = 8`) -/

def legPtrAt (pflData idx : UInt64) : Ptr :=
  pflData + PflOff.headerSize + idx * PflOff.legStride

def findLegLoop (pflData mid idx : UInt64) : SolanaM UInt64 := do
  if h : idx >= MAX_LEGS then
    return MAX_LEGS
  else
    let lp := legPtrAt pflData idx
    let act ← loadU8 lp LegOff.active
    if act != 0 then
      let lmid ← loadU64 lp LegOff.marketId
      if lmid == mid then return idx
    findLegLoop pflData mid (idx + 1)
termination_by (MAX_LEGS - idx).toNat
decreasing_by
  have hlt : idx.toNat < MAX_LEGS.toNat := by
    have : ¬ (idx >= MAX_LEGS) := h
    exact Nat.lt_of_not_ge (fun hge => this (UInt64.le_iff_toNat_le.mpr hge))
  have h1 : (1 : UInt64).toNat = 1 := rfl
  have hmax : MAX_LEGS.toNat = 8 := rfl
  have hadd : (idx + 1).toNat = idx.toNat + 1 := by
    rw [UInt64.toNat_add, h1]; exact Nat.mod_eq_of_lt (by omega)
  change (MAX_LEGS - (idx + 1)).toNat < (MAX_LEGS - idx).toNat
  rw [UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)),
      UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)), hadd]
  omega

def findEmptySlotLoop (pflData idx : UInt64) : SolanaM UInt64 := do
  if h : idx >= MAX_LEGS then
    return MAX_LEGS
  else
    if (← loadU8 (legPtrAt pflData idx) LegOff.active) == 0 then return idx
    findEmptySlotLoop pflData (idx + 1)
termination_by (MAX_LEGS - idx).toNat
decreasing_by
  have hlt : idx.toNat < MAX_LEGS.toNat := by
    have : ¬ (idx >= MAX_LEGS) := h
    exact Nat.lt_of_not_ge (fun hge => this (UInt64.le_iff_toNat_le.mpr hge))
  have h1 : (1 : UInt64).toNat = 1 := rfl
  have hmax : MAX_LEGS.toNat = 8 := rfl
  have hadd : (idx + 1).toNat = idx.toNat + 1 := by
    rw [UInt64.toNat_add, h1]; exact Nat.mod_eq_of_lt (by omega)
  change (MAX_LEGS - (idx + 1)).toNat < (MAX_LEGS - idx).toNat
  rw [UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)),
      UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)), hadd]
  omega

/-- Verify every active open leg in `pflData` was refreshed in `curSlot` (same-slot freshness!). -/
def checkAllLegsFreshLoop (pflData curSlot idx : UInt64) : SolanaM Unit := do
  if h : idx >= MAX_LEGS then
    return ()
  else
    let lp := legPtrAt pflData idx
    let act ← loadU8 lp LegOff.active
    if act != 0 then
      let side ← loadU8 lp LegOff.side
      if side != SIDE_FLAT then
        let rSlot ← loadU64 lp LegOff.lastRefreshSlot
        SolanaM.require (rSlot == curSlot) PErr.stalePortfolioLeg
    checkAllLegsFreshLoop pflData curSlot (idx + 1)
termination_by (MAX_LEGS - idx).toNat
decreasing_by
  have hlt : idx.toNat < MAX_LEGS.toNat := by
    have : ¬ (idx >= MAX_LEGS) := h
    exact Nat.lt_of_not_ge (fun hge => this (UInt64.le_iff_toNat_le.mpr hge))
  have h1 : (1 : UInt64).toNat = 1 := rfl
  have hmax : MAX_LEGS.toNat = 8 := rfl
  have hadd : (idx + 1).toNat = idx.toNat + 1 := by
    rw [UInt64.toNat_add, h1]; exact Nat.mod_eq_of_lt (by omega)
  change (MAX_LEGS - (idx + 1)).toNat < (MAX_LEGS - idx).toNat
  rw [UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)),
      UInt64.toNat_sub_of_le _ _ (UInt64.le_iff_toNat_le.mpr (by omega)), hadd]
  omega

def findOrAllocLegSlot (pflData mktKey : Ptr) (mid : UInt64) : SolanaM UInt64 := do
  let existing ← findLegLoop pflData mid 0
  if existing < MAX_LEGS then
    requireKeyEq mktKey (legPtrAt pflData existing + LegOff.market) PErr.marketMismatch
    return existing
  let empty ← findEmptySlotLoop pflData 0
  SolanaM.require (empty < MAX_LEGS) PErr.portfolioFull
  let lp := legPtrAt pflData empty
  storeU8  lp LegOff.active 1
  storeU8  lp LegOff.side SIDE_FLAT
  storeU64 lp LegOff.marketId mid
  pubkeyCopy (lp + LegOff.market) mktKey
  storeU64 lp LegOff.rawBasis 0
  storeU64 lp LegOff.entryK K_ZERO
  storeU64 lp LegOff.posPnl 0
  storeU64 lp LegOff.cachedMm 0
  storeU64 lp LegOff.cachedIm 0
  storeU64 lp LegOff.lastRefreshSlot 0
  storeU64 lp LegOff.reservedMm 0
  let cnt ← loadU8 pflData PflOff.legCount
  storeU8 pflData PflOff.legCount (cnt + 1)
  return empty

def freeLegIfEmpty (pflData slotIdx : UInt64) : SolanaM Unit := do
  let lp := legPtrAt pflData slotIdx
  let act ← loadU8 lp LegOff.active
  if act != 0 then
    let side ← loadU8 lp LegOff.side
    let rawB ← loadU64 lp LegOff.rawBasis
    let pnl  ← loadU64 lp LegOff.posPnl
    if side == SIDE_FLAT && rawB == 0 && pnl == 0 then
      zeroWordsLoop lp 0 (PflOff.legStride >>> 3)
      let cnt ← loadU8 pflData PflOff.legCount
      if cnt > 0 then storeU8 pflData PflOff.legCount (cnt - 1)

/-! ### Exact Verified Economic Kernels (Matching `Percolator.Spec` & `Percolator.Proofs`) -/

def deriveAdlU64 (oi rawBasis : UInt64) : SolanaM UInt64 := do
  if rawBasis == 0 || oi >= rawBasis then return ADL_ONE
  let q ← mulDiv64 ADL_ONE oi rawBasis
  return if q == 0 then 1 else q

def haircutPayoutU64 (p realizedPool posPnlTot : UInt64) : SolanaM UInt64 := do
  if posPnlTot == 0 || p == 0 || realizedPool == 0 then return 0
  mulDiv64 p (if realizedPool < posPnlTot then realizedPool else posPnlTot) posPnlTot

def proportionalRawRedU64 (qty rawLeg rawSide oiSide : UInt64) : SolanaM UInt64 := do
  if oiSide == 0 || rawSide == 0 then return rawLeg
  if oiSide == rawSide then
    return if qty < rawLeg then qty else rawLeg
  let ceilR ← mulDiv64Ceil qty rawSide oiSide
  let r1 := if ceilR == 0 then 1 else ceilR
  return if r1 < rawLeg then r1 else rawLeg

/-- Raw-basis minted when opening `qty` effective OI on a side with `(oiSide, rawSide)`.
    Rejects 100%-wiped-out sides (`oiSide == 0 && rawSide > 0`); on healthy sides returns `qty`;
    on partially-ADL'd sides (`0 < oiSide < rawSide`) returns `ceil(qty * rawSide / oiSide)` so
    `(oi + qty) / (raw + rawAdd) <= oi / raw` (monotone non-inflating, re-enabling post-ADL opens!). -/
def postAdlRawAddU64 (qty oiSide rawSide : UInt64) : SolanaM UInt64 := do
  if rawSide == 0 then return qty
  SolanaM.require (oiSide > 0) PErr.adlHaircutActive
  if oiSide == rawSide then return qty
  let r ← mulDiv64Ceil qty rawSide oiSide
  return if r < qty then qty else r

/-- Ceiling proportional OI removal for liquidating `rawClose` rawBasis on `(oiSide, rawSide)`.
    Guarantees `oiSide - eff <= rawSide - rawClose` ALWAYS (even for 1-atom post-ADL dust!). -/
def proportionalOiForRawU64 (rawClose oiSide rawSide : UInt64) : SolanaM UInt64 := do
  if oiSide == 0 || rawSide == 0 then return 0
  if oiSide == rawSide then
    return if rawClose < oiSide then rawClose else oiSide
  let c ← mulDiv64Ceil rawClose oiSide rawSide
  let c1 := if c == 0 then 1 else c
  return if c1 < oiSide then c1 else oiSide

def computeMarginReq (effQty markPrice bps : UInt64) : SolanaM UInt64 := do
  if effQty == 0 then return 0
  mulDiv64Ceil (← mulDiv64Ceil effQty markPrice POS_SCALE) bps BPS_DEN

def computeMarginWithFloor (effQty markPrice bps floor : UInt64) : SolanaM UInt64 := do
  if effQty == 0 then return 0
  let r ← computeMarginReq effQty markPrice bps
  return if r < floor then floor else r

def legEffectiveQty (mktData lp : Ptr) : SolanaM UInt64 := do
  let side ← loadU8 lp LegOff.side
  let rawB ← loadU64 lp LegOff.rawBasis
  if side == SIDE_FLAT || rawB == 0 then return 0
  let adl ← loadU64 mktData (if side == SIDE_LONG then MktOff.adlLong else MktOff.adlShort)
  mulDiv64 rawB adl ADL_ONE

/-- Refresh `cachedMm` / `cachedIm` / `reservedMm` on `slotIdx` and maintain exact
    `pfl.totalMm`, `pfl.totalIm`, and unconditional `pfl.totalReservedMm <= pfl.capital`!
    Anti-shielding rule: a leg may ONLY grow `reservedMm` above `oldRes` when `capital >= newTotMm`,
    and ALWAYS bounded by `capital - otherRes` so an untrusted market can NEVER inflate its
    margin reservation to starve another market's `MM`! -/
def syncLegMargins (mktData pflData : Ptr) (slotIdx curSlot : UInt64) : SolanaM Unit := do
  let lp := legPtrAt pflData slotIdx
  let oldMm  ← loadU64 lp LegOff.cachedMm
  let oldIm  ← loadU64 lp LegOff.cachedIm
  let oldRes ← loadU64 lp LegOff.reservedMm
  let eff ← legEffectiveQty mktData lp
  let markP ← loadU64 mktData MktOff.markPrice
  let minMm ← loadU64 mktData MktOff.minMm
  let newMm ← computeMarginWithFloor eff markP (← loadU64 mktData MktOff.mmBps) minMm
  let newIm ← computeMarginWithFloor eff markP (← loadU64 mktData MktOff.imBps) minMm
  let totMm  ← loadU64 pflData PflOff.totalMm
  let totIm  ← loadU64 pflData PflOff.totalIm
  let totRes ← loadU64 pflData PflOff.totalReservedMm
  let baseMm  := if totMm  >= oldMm  then totMm  - oldMm  else 0
  let baseIm  := if totIm  >= oldIm  then totIm  - oldIm  else 0
  let baseRes := if totRes >= oldRes then totRes - oldRes else 0
  let newTotMm ← checkedAdd64 baseMm newMm
  let newTotIm ← checkedAdd64 baseIm newIm
  let cap ← loadU64 pflData PflOff.capital
  let availRes := if cap >= baseRes then cap - baseRes else 0
  let capTarget :=
    if cap >= newTotMm then newMm
    else if oldRes < newMm then oldRes else newMm
  let newRes := if capTarget < availRes then capTarget else availRes
  storeU64 pflData PflOff.totalMm         newTotMm
  storeU64 pflData PflOff.totalIm         newTotIm
  storeU64 pflData PflOff.totalReservedMm (← checkedAdd64 baseRes newRes)
  storeU64 lp LegOff.cachedMm        newMm
  storeU64 lp LegOff.cachedIm        newIm
  storeU64 lp LegOff.reservedMm      newRes
  storeU64 lp LegOff.lastRefreshSlot curSlot

/-- Settle `entryK → kSide` with per-leg loss containment (`capAvailableForLeg = capital - otherRes`),
    optionally realize backed `posPnl` directly into shared `pfl.capital`, and sync cached margins.
    Returns any unbacked loss `shortfall`. -/
def settleAndRefreshLeg (dexData mktData pflData : Ptr) (slotIdx curSlot : UInt64)
    (forceHaircut : Bool) : SolanaM UInt64 := do
  let lp := legPtrAt pflData slotIdx
  let side ← loadU8 lp LegOff.side
  let rawB ← loadU64 lp LegOff.rawBasis
  let mut posPnl ← loadU64 lp LegOff.posPnl
  let mut shortfall : UInt64 := 0

  if side != SIDE_FLAT && rawB > 0 then
    let kSide ← loadU64 mktData (if side == SIDE_LONG then MktOff.kLong else MktOff.kShort)
    let entryK ← loadU64 lp LegOff.entryK
    if kSide > entryK then
      let gain ← mulDiv64 rawB (kSide - entryK) POS_SCALE
      posPnl ← checkedAdd64 posPnl gain
      addU64At mktData MktOff.totalPosPnl gain
    else if entryK > kSide then
      let loss ← mulDiv64Ceil rawB (entryK - kSide) POS_SCALE
      let pnlBurned := if loss < posPnl then loss else posPnl
      posPnl := posPnl - pnlBurned
      if pnlBurned > 0 then subU64At mktData MktOff.totalPosPnl pnlBurned
      let rem1 := loss - pnlBurned
      -- Per-leg loss containment (`cross_margin_isolation_envelope` + anti-shielding):
      -- NEVER debit below `otherRes = totalReservedMm - leg.reservedMm`!
      let cap ← loadU64 pflData PflOff.capital
      let totRes ← loadU64 pflData PflOff.totalReservedMm
      let legRes ← loadU64 lp LegOff.reservedMm
      let otherRes := if totRes >= legRes then totRes - legRes else 0
      let availCap := if cap >= otherRes then cap - otherRes else 0
      let capDeb := if rem1 < availCap then rem1 else availCap
      if capDeb > 0 then
        storeU64 pflData PflOff.capital (cap - capDeb)
        subU64At dexData DexOff.totalCapital capDeb
        addU64At mktData MktOff.realizedPool capDeb
        addU64At dexData DexOff.totalResidual capDeb
      shortfall := rem1 - capDeb
    storeU64 lp LegOff.entryK kSide

  -- Instant backed realization into shared `pfl.capital` (Hyperliquid cross-margin efficiency!):
  -- 1. If `R_m >= P^+_m` (healthy market), realize 100% (`pay = posPnl`) automatically!
  -- 2. If `R_m < P^+_m` and `forceHaircut`, realize pro-rata `haircutPayout` (monotone ratio!).
  if posPnl > 0 then
    let warmup ← loadU64 mktData MktOff.warmupSlots
    if warmup == 0 then
      let rPool  ← loadU64 mktData MktOff.realizedPool
      let totPnl ← loadU64 mktData MktOff.totalPosPnl
      if (rPool >= totPnl && totPnl > 0) || forceHaircut then
        let pay ← haircutPayoutU64 posPnl rPool totPnl
        if pay > 0 then
          subU64At mktData MktOff.realizedPool pay
          subU64At dexData DexOff.totalResidual pay
          addU64At pflData PflOff.capital pay
          addU64At dexData DexOff.totalCapital pay
        subU64At mktData MktOff.totalPosPnl posPnl
        posPnl := 0

  storeU64 lp LegOff.posPnl posPnl
  syncLegMargins mktData pflData slotIdx curSlot
  return shortfall

/-! ### Instruction Handlers (All 19 Unified Instructions, Auto-Outlined by `lean-svm`) -/

def handleInitDex (input : Ptr) : SolanaM Unit := do
  let (dex, admin, mint, vaultPda, feeVault, tokProg, sysProg, ix, progId) ← parse7 input
  SolanaM.require (ix.len >= 20) Err.invalidInstructionData
  requireSigner admin; requireWritable vaultPda
  checkFreshAccount dex progId DexOff.size
  checkTokenProg tokProg
  checkSysProg sysProg
  let vaultBump    ← loadU8  ix.ptr 1
  let decimals     ← loadU8  ix.ptr 2
  let createFee    ← loadU64 ix.ptr 4
  let rentLamports ← loadU64 ix.ptr 12
  SolanaM.require (createFee <= MAX_CREATE_FEE && rentLamports >= 2_039_280) PErr.invalidParams

  -- Create & initialize the `DexVault` PDA token account on-chain (works on real clusters &
  -- immune to 1-lamport pre-funding griefing; runtime enforces PDA seeds ["dvlt", dex, bump]!).
  let curLam ← AccountView.lamports vaultPda
  if curLam < rentLamports then
    systemTransfer sysProg admin vaultPda (rentLamports - curLam)
  systemAllocateSigned sysProg vaultPda 165 TAG_DEX_VAULT (AccountView.keyPtr dex) vaultBump
  systemAssignSigned   sysProg vaultPda (AccountView.keyPtr tokProg) TAG_DEX_VAULT (AccountView.keyPtr dex) vaultBump
  tokenInitializeAccount3 tokProg vaultPda mint (AccountView.keyPtr vaultPda)

  let dData := AccountView.dataPtr dex
  zeroWordsLoop dData 0 (DexOff.size >>> 3)
  storeU32 dData DexOff.magic MAGIC_DEX
  storeU8  dData DexOff.version 2
  storeU8  dData DexOff.vaultBump vaultBump
  storeU8  dData DexOff.decimals decimals
  pubkeyCopy (dData + DexOff.admin)     (AccountView.keyPtr admin)
  pubkeyCopy (dData + DexOff.quoteMint) (AccountView.keyPtr mint)
  pubkeyCopy (dData + DexOff.vaultPda)  (AccountView.keyPtr vaultPda)
  pubkeyCopy (dData + DexOff.feeVault)  (AccountView.keyPtr feeVault)
  storeU64 dData DexOff.nextMarketId 1
  storeU64 dData DexOff.nextPortfolioId 1
  storeU64 dData DexOff.createFee createFee

def handleUpdateCreateFee (input : Ptr) : SolanaM Unit := do
  let (n, dex, ix, progId) ← parseIxOnly input
  SolanaM.require (n >= 2) Err.notEnoughAccountKeys
  SolanaM.require (ix.len >= 9) Err.invalidInstructionData
  let admin ← AccountView.nextCursor dex
  requireSigner admin; requireWritable dex
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  let dData := AccountView.dataPtr dex
  requireKeyEq (AccountView.keyPtr admin) (dData + DexOff.admin) PErr.unauthorized
  let newFee ← loadU64 ix.ptr 1
  SolanaM.require (newFee <= MAX_CREATE_FEE) PErr.invalidParams
  storeU64 dData DexOff.createFee newFee
  if n >= 3 then
    let newAdmin ← AccountView.nextCursor admin
    SolanaM.require (!(← pubkeyIsZero (AccountView.keyPtr newAdmin))) PErr.invalidParams
    pubkeyCopy (dData + DexOff.admin) (AccountView.keyPtr newAdmin)

def handleCreateMarket (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, mint, oracleAuth, creator, creatorTok, feeVaultTok, tokProg, ix, progId) ← parse8 input
  SolanaM.require (ix.len >= 92) Err.invalidInstructionData
  requireSigner creator; requireWritable dex
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkFreshAccount mkt progId MktOff.size
  checkTokenProg tokProg

  let dData := AccountView.dataPtr dex
  requireKeyEq (AccountView.keyPtr mint) (dData + DexOff.quoteMint) PErr.mintMismatch

  let initPrice     ← loadU64 ix.ptr 4
  let exitWindow    ← loadU64 ix.ptr 12
  let maxStaleness  ← loadU64 ix.ptr 20
  let warmupSlots   ← loadU64 ix.ptr 28
  let imBps         ← loadU64 ix.ptr 36
  let mmBps         ← loadU64 ix.ptr 44
  let liqFeeBps     ← loadU64 ix.ptr 52
  let tradeFeeBps   ← loadU64 ix.ptr 60
  let maxMoveBps    ← loadU64 ix.ptr 68
  let maxInsDrawBps ← loadU64 ix.ptr 76
  let maxOi         ← loadU64 ix.ptr 84
  let minMm         ← if ix.len >= 100 then loadU64 ix.ptr 92 else pure 0

  SolanaM.require (initPrice > 0 && initPrice <= MAX_PRICE) PErr.invalidParams
  SolanaM.require (exitWindow >= MIN_EXIT_WINDOW && exitWindow <= MAX_EXIT_WINDOW &&
                   maxStaleness >= 1 && maxStaleness <= MAX_STALENESS && warmupSlots == 0) PErr.invalidParams
  SolanaM.require (mmBps > 0 && mmBps < imBps && imBps <= BPS_DEN) PErr.invalidParams
  SolanaM.require ((← checkedAdd64 maxMoveBps liqFeeBps) <= mmBps) PErr.invalidParams
  SolanaM.require (tradeFeeBps <= 1000 && maxInsDrawBps <= BPS_DEN && maxOi > 0 && maxOi <= MAX_OI) PErr.invalidParams

  let createFee ← loadU64 dData DexOff.createFee
  if createFee > 0 then
    requireKeyEq (AccountView.keyPtr feeVaultTok) (dData + DexOff.feeVault) PErr.vaultMismatch
    let dec ← loadU8 dData DexOff.decimals
    tokenTransferChecked tokProg creatorTok mint feeVaultTok creator createFee dec

  let mid ← loadU64 dData DexOff.nextMarketId
  storeU64 dData DexOff.nextMarketId (← checkedAdd64 mid 1)

  let mData := AccountView.dataPtr mkt
  zeroWordsLoop mData 0 (MktOff.size >>> 3)
  storeU32 mData MktOff.magic MAGIC_MARKET
  storeU8  mData MktOff.status STATUS_ACTIVE
  storeU64 mData MktOff.marketId mid
  pubkeyCopy (mData + MktOff.dex)         (AccountView.keyPtr dex)
  pubkeyCopy (mData + MktOff.oracleAuth)  (AccountView.keyPtr oracleAuth)
  pubkeyCopy (mData + MktOff.marketAdmin) (AccountView.keyPtr creator)
  storeU64 mData MktOff.markPrice initPrice
  storeU64 mData MktOff.lastOracleSlot (← getClockSlot)
  storeU64 mData MktOff.exitWindow exitWindow
  storeU64 mData MktOff.maxStaleness maxStaleness
  storeU64 mData MktOff.warmupSlots warmupSlots
  storeU64 mData MktOff.imBps imBps
  storeU64 mData MktOff.mmBps mmBps
  storeU64 mData MktOff.liqFeeBps liqFeeBps
  storeU64 mData MktOff.tradeFeeBps tradeFeeBps
  storeU64 mData MktOff.maxMoveBps maxMoveBps
  storeU64 mData MktOff.maxInsDrawBps maxInsDrawBps
  storeU64 mData MktOff.maxOi maxOi
  storeU64 mData MktOff.adlLong ADL_ONE
  storeU64 mData MktOff.adlShort ADL_ONE
  storeU64 mData MktOff.kLong K_ZERO
  storeU64 mData MktOff.kShort K_ZERO
  storeU64 mData MktOff.minMm minMm

/-- Burn (`n == 2` or zero key) or transfer `marketAdmin` (`role == 0`), or rotate `oracleAuth` (`role == 1`). -/
def handleUpdateMarketAuth (input : Ptr) : SolanaM Unit := do
  let (n, mkt, ix, progId) ← parseIxOnly input
  SolanaM.require (n >= 2) Err.notEnoughAccountKeys
  let admin ← AccountView.nextCursor mkt
  requireSigner admin; requireWritable mkt
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  let mData := AccountView.dataPtr mkt
  SolanaM.require ((← loadU8 mData MktOff.status) != STATUS_TOMBSTONED) PErr.invalidStatus
  SolanaM.require (!(← pubkeyIsZero (mData + MktOff.marketAdmin))) PErr.unauthorized
  requireKeyEq (AccountView.keyPtr admin) (mData + MktOff.marketAdmin) PErr.unauthorized
  let role ← if ix.len >= 2 then loadU8 ix.ptr 1 else pure 0
  if role == 0 then
    if n >= 3 then
      let newAdmin ← AccountView.nextCursor admin
      pubkeyCopy (mData + MktOff.marketAdmin) (AccountView.keyPtr newAdmin)
    else
      storeU64 mData (MktOff.marketAdmin + 0)  0
      storeU64 mData (MktOff.marketAdmin + 8)  0
      storeU64 mData (MktOff.marketAdmin + 16) 0
      storeU64 mData (MktOff.marketAdmin + 24) 0
  else
    SolanaM.require (n >= 3) Err.notEnoughAccountKeys
    let newOracle ← AccountView.nextCursor admin
    SolanaM.require (!(← pubkeyIsZero (AccountView.keyPtr newOracle))) PErr.invalidParams
    pubkeyCopy (mData + MktOff.oracleAuth) (AccountView.keyPtr newOracle)

def handleUpdateOracle (input : Ptr) : SolanaM Unit := do
  let (mkt, oracleAuth, ix, progId) ← parse2 input
  SolanaM.require (ix.len >= 18) Err.invalidInstructionData
  requireSigner oracleAuth; requireWritable mkt
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  let mData := AccountView.dataPtr mkt
  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_ACTIVE) PErr.invalidStatus
  requireKeyEq (AccountView.keyPtr oracleAuth) (mData + MktOff.oracleAuth) PErr.unauthorized

  -- Per-slot rate limit: strictly 1 oracle step per slot (enforces `oracle_clamp_zero_bad_debt`!)
  let curSlot ← getClockSlot
  let lastSlot ← loadU64 mData MktOff.lastOracleSlot
  SolanaM.require (curSlot > lastSlot) PErr.oracleSameSlot

  let newPrice ← loadU64 ix.ptr 1
  let fundSign ← loadU8  ix.ptr 9
  let fundE6   ← loadU64 ix.ptr 10
  SolanaM.require (newPrice > 0 && newPrice <= MAX_PRICE) PErr.invalidParams

  let oldPrice ← loadU64 mData MktOff.markPrice
  let diff := if newPrice >= oldPrice then newPrice - oldPrice else oldPrice - newPrice
  let maxAllowed ← mulDiv64 oldPrice (← loadU64 mData MktOff.maxMoveBps) BPS_DEN
  SolanaM.require ((← checkedAdd64 diff fundE6) <= maxAllowed) PErr.oracleClamp

  let adlL ← loadU64 mData MktOff.adlLong
  let adlS ← loadU64 mData MktOff.adlShort
  if newPrice > oldPrice then
    addU64At mData MktOff.kLong  (← mulDiv64     adlL diff ADL_ONE)
    subU64At mData MktOff.kShort (← mulDiv64Ceil adlS diff ADL_ONE)
  else if oldPrice > newPrice then
    subU64At mData MktOff.kLong  (← mulDiv64Ceil adlL diff ADL_ONE)
    addU64At mData MktOff.kShort (← mulDiv64     adlS diff ADL_ONE)

  if fundE6 > 0 then
    if fundSign == 0 then
      subU64At mData MktOff.kLong  (← mulDiv64Ceil adlL fundE6 ADL_ONE)
      addU64At mData MktOff.kShort (← mulDiv64     adlS fundE6 ADL_ONE)
    else
      subU64At mData MktOff.kShort (← mulDiv64Ceil adlS fundE6 ADL_ONE)
      addU64At mData MktOff.kLong  (← mulDiv64     adlL fundE6 ADL_ONE)

  storeU64 mData MktOff.markPrice newPrice
  storeU64 mData MktOff.lastOracleSlot curSlot
  log64 EvtTag.oracle (← loadU64 mData MktOff.marketId) newPrice fundSign.toUInt64 fundE6

def handleInitPortfolio (input : Ptr) : SolanaM Unit := do
  let (dex, pfl, owner, _, progId) ← parse3 input
  requireSigner owner; requireWritable dex
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkFreshAccount pfl progId PflOff.size
  let dData := AccountView.dataPtr dex
  let pid ← loadU64 dData DexOff.nextPortfolioId
  storeU64 dData DexOff.nextPortfolioId (← checkedAdd64 pid 1)
  let fData := AccountView.dataPtr pfl
  zeroWordsLoop fData 0 (PflOff.size >>> 3)
  storeU32 fData PflOff.magic MAGIC_PORTFOLIO
  storeU8  fData PflOff.legCount 0
  storeU64 fData PflOff.portfolioId pid
  storeU64 fData PflOff.episode 1
  pubkeyCopy (fData + PflOff.dex)   (AccountView.keyPtr dex)
  pubkeyCopy (fData + PflOff.owner) (AccountView.keyPtr owner)

def handleDepositCollateral (input : Ptr) : SolanaM Unit := do
  let (dex, pfl, owner, ownerTok, mint, vaultPda, tokProg, ix, progId) ← parse7 input
  SolanaM.require (ix.len >= 9) Err.invalidInstructionData
  requireSigner owner; requireWritable dex; requireWritable pfl
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic pfl progId PflOff.size MAGIC_PORTFOLIO
  let dData := AccountView.dataPtr dex
  let fData := AccountView.dataPtr pfl
  requireKeyEq (AccountView.keyPtr dex) (fData + PflOff.dex) PErr.dexMismatch
  checkDexVaultAndMint dData mint vaultPda tokProg

  let amount ← loadU64 ix.ptr 1
  SolanaM.require (amount > 0) PErr.invalidParams
  let dec ← loadU8 dData DexOff.decimals
  tokenTransferChecked tokProg ownerTok mint vaultPda owner amount dec

  addU64At dData DexOff.vaultBalance amount
  addU64At dData DexOff.totalCapital amount
  addU64At fData PflOff.capital      amount
  log64 EvtTag.deposit (← loadU64 fData PflOff.portfolioId) amount (← loadU64 fData PflOff.capital) 0

def handleWithdrawCollateral (input : Ptr) : SolanaM Unit := do
  let (dex, pfl, owner, destTok, mint, vaultPda, tokProg, ix, progId) ← parse7 input
  SolanaM.require (ix.len >= 9) Err.invalidInstructionData
  requireSigner owner; requireWritable dex; requireWritable pfl
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic pfl progId PflOff.size MAGIC_PORTFOLIO
  let dData := AccountView.dataPtr dex
  let fData := AccountView.dataPtr pfl
  requireKeyEq (AccountView.keyPtr dex)   (fData + PflOff.dex)   PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr owner) (fData + PflOff.owner) PErr.ownerMismatch
  checkDexVaultAndMint dData mint vaultPda tokProg

  let amount ← loadU64 ix.ptr 1
  SolanaM.require (amount > 0) PErr.invalidParams

  -- Same-slot freshness: every active open leg must have been refreshed in `curSlot`!
  -- Flat portfolios (0 open legs) pass unconditionally!
  let curSlot ← getClockSlot
  checkAllLegsFreshLoop fData curSlot 0

  let cap ← loadU64 fData PflOff.capital
  let remCap ← checkedSub64 cap amount PErr.insufficientFunds
  SolanaM.require (remCap >= (← loadU64 fData PflOff.totalIm)) PErr.insufficientMargin

  storeU64 fData PflOff.capital remCap
  subU64At dData DexOff.totalCapital amount
  subU64At dData DexOff.vaultBalance amount
  let dec  ← loadU8 dData DexOff.decimals
  let bump ← loadU8 dData DexOff.vaultBump
  tokenTransferCheckedSigned tokProg vaultPda mint destTok vaultPda amount dec
    TAG_DEX_VAULT (AccountView.keyPtr dex) bump
  log64 EvtTag.withdraw (← loadU64 fData PflOff.portfolioId) amount remCap 0

def handleDepositInsurance (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, receipt, lp, lpTok, mint, vaultPda, tokProg, ix, progId) ← parse8 input
  SolanaM.require (ix.len >= 9) Err.invalidInstructionData
  requireSigner lp; requireWritable dex; requireWritable mkt
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex) PErr.dexMismatch
  checkDexVaultAndMint dData mint vaultPda tokProg

  let st ← loadU8 mData MktOff.status
  SolanaM.require (st == STATUS_ACTIVE || st == STATUS_WINDDOWN) PErr.invalidStatus
  let amount ← loadU64 ix.ptr 1
  SolanaM.require (amount > 0) PErr.invalidParams
  let donate ← if ix.len >= 10 then loadU8 ix.ptr 9 else pure 0

  let dec ← loadU8 dData DexOff.decimals
  if donate != 0 then
    -- Permissionless permanent insurance donation (0 LP shares minted!)
    tokenTransferChecked tokProg lpTok mint vaultPda lp amount dec
    addU64At dData DexOff.vaultBalance   amount
    addU64At dData DexOff.totalInsurance amount
    addU64At mData MktOff.insurance      amount
  else
    requireWritable receipt
    requireKeyEq (AccountView.ownerPtr receipt) progId Err.illegalOwner
    SolanaM.require ((← AccountView.dataLen receipt) == InsOff.size) Err.invalidAccountData
    let rData := AccountView.dataPtr receipt
    let mid ← loadU64 mData MktOff.marketId
    if (← loadU32 rData InsOff.magic) == 0 then
      zeroWordsLoop rData 0 (InsOff.size >>> 3)
      storeU32 rData InsOff.magic MAGIC_INSURANCE
      storeU64 rData InsOff.marketId mid
      pubkeyCopy (rData + InsOff.owner)  (AccountView.keyPtr lp)
      pubkeyCopy (rData + InsOff.market) (AccountView.keyPtr mkt)
    else
      SolanaM.require ((← loadU32 rData InsOff.magic) == MAGIC_INSURANCE) PErr.invalidMagic
      requireKeyEq (AccountView.keyPtr lp)  (rData + InsOff.owner)  PErr.ownerMismatch
      requireKeyEq (AccountView.keyPtr mkt) (rData + InsOff.market) PErr.marketMismatch
      SolanaM.require ((← loadU64 rData InsOff.marketId) == mid) PErr.marketMismatch

    let totSh ← loadU64 mData MktOff.totalInsShares
    let ins   ← loadU64 mData MktOff.insurance
    let minted ← if totSh == 0 || ins == 0 then pure amount else mulDiv64 amount totSh ins
    SolanaM.require (minted > 0) PErr.invalidParams

    tokenTransferChecked tokProg lpTok mint vaultPda lp amount dec

    addU64At dData DexOff.vaultBalance   amount
    addU64At dData DexOff.totalInsurance amount
    addU64At mData MktOff.insurance      amount
    addU64At mData MktOff.totalInsShares minted
    addU64At rData InsOff.shares         minted

def handleWithdrawInsurance (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, receipt, lp, destTok, mint, vaultPda, tokProg, ix, progId) ← parse8 input
  SolanaM.require (ix.len >= 9) Err.invalidInstructionData
  requireSigner lp; requireWritable dex; requireWritable mkt; requireWritable receipt
  checkOwnedMagic dex     progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt     progId MktOff.size MAGIC_MARKET
  checkOwnedMagic receipt progId InsOff.size MAGIC_INSURANCE
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  let rData := AccountView.dataPtr receipt
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex)    PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr lp)  (rData + InsOff.owner)  PErr.ownerMismatch
  requireKeyEq (AccountView.keyPtr mkt) (rData + InsOff.market) PErr.marketMismatch
  SolanaM.require ((← loadU64 rData InsOff.marketId) == (← loadU64 mData MktOff.marketId)) PErr.marketMismatch
  checkDexVaultAndMint dData mint vaultPda tokProg

  SolanaM.require ((← loadU8 mData MktOff.status) != STATUS_TOMBSTONED) PErr.invalidStatus
  -- Insurance CANNOT be withdrawn while ANY OI or post-ADL rawBasis exists on this market!
  SolanaM.require ((← loadU64 mData MktOff.oiLong) == 0 && (← loadU64 mData MktOff.oiShort) == 0 &&
                   (← loadU64 mData MktOff.rawBasisLong) == 0 && (← loadU64 mData MktOff.rawBasisShort) == 0) PErr.nonzeroOi

  let burnShares ← loadU64 ix.ptr 1
  SolanaM.require (burnShares > 0) PErr.invalidParams
  let curSh ← loadU64 rData InsOff.shares
  let totSh ← loadU64 mData MktOff.totalInsShares
  SolanaM.require (curSh <= totSh) PErr.nonzeroState
  SolanaM.require (curSh >= burnShares) PErr.insufficientFunds
  let ins   ← loadU64 mData MktOff.insurance
  let payout ← mulDiv64 burnShares ins totSh

  storeU64 rData InsOff.shares (curSh - burnShares)
  subU64At mData MktOff.totalInsShares burnShares
  if payout > 0 then
    subU64At mData MktOff.insurance      payout
    subU64At dData DexOff.totalInsurance payout
    subU64At dData DexOff.vaultBalance   payout
    let dec  ← loadU8 dData DexOff.decimals
    let bump ← loadU8 dData DexOff.vaultBump
    tokenTransferCheckedSigned tokProg vaultPda mint destTok vaultPda payout dec
      TAG_DEX_VAULT (AccountView.keyPtr dex) bump

def handleRefreshLeg (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, pfl, ix, progId) ← parse3 input
  requireWritable dex; requireWritable mkt; requireWritable pfl
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  checkOwnedMagic pfl progId PflOff.size MAGIC_PORTFOLIO
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  let fData := AccountView.dataPtr pfl
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex) PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex) (fData + PflOff.dex) PErr.dexMismatch

  let st ← loadU8 mData MktOff.status
  SolanaM.require (st == STATUS_ACTIVE || st == STATUS_WINDDOWN) PErr.invalidStatus
  let curSlot ← getClockSlot
  if st == STATUS_ACTIVE then checkOracleFresh mData curSlot

  let mid ← loadU64 mData MktOff.marketId
  let slotIdx ← findLegLoop fData mid 0
  SolanaM.require (slotIdx < MAX_LEGS) PErr.legNotFound
  requireKeyEq (AccountView.keyPtr mkt) (legPtrAt fData slotIdx + LegOff.market) PErr.marketMismatch

  let forceHaircut ← if ix.len >= 2 then do pure ((← loadU8 ix.ptr 1) != 0) else pure false
  let _ ← settleAndRefreshLeg dData mData fData slotIdx curSlot forceHaircut
  freeLegIfEmpty fData slotIdx

/-- Apply optional bounded `execPrice` spread transfer directly between Long & Short `capital`.
    Preserves `C_tot` identically and allows external CLOB/AMM/matcher CPIs with real spreads! -/
def applyExecSpread (mData fLData fSData : Ptr) (qty execPrice : UInt64) : SolanaM Unit := do
  if execPrice == 0 then return ()
  let markP ← loadU64 mData MktOff.markPrice
  if execPrice == markP then return ()
  let diff := if execPrice > markP then execPrice - markP else markP - execPrice
  let maxSlip ← mulDiv64 markP (← loadU64 mData MktOff.maxMoveBps) BPS_DEN
  SolanaM.require (diff <= maxSlip) PErr.execPriceClamp
  let spread ← mulDiv64Ceil qty diff POS_SCALE
  if spread > 0 then
    if execPrice > markP then
      let cL ← loadU64 fLData PflOff.capital
      SolanaM.require (cL >= spread) PErr.insufficientFunds
      storeU64 fLData PflOff.capital (cL - spread)
      addU64At fSData PflOff.capital spread
    else
      let cS ← loadU64 fSData PflOff.capital
      SolanaM.require (cS >= spread) PErr.insufficientFunds
      storeU64 fSData PflOff.capital (cS - spread)
      addU64At fLData PflOff.capital spread

def handleTrade (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, pflL, ownerL, pflS, ownerS, ix, progId) ← parse6 input
  SolanaM.require (ix.len >= 10) Err.invalidInstructionData
  requireSigner ownerL; requireSigner ownerS
  requireWritable dex; requireWritable mkt; requireWritable pflL; requireWritable pflS
  requireKeyNe (AccountView.keyPtr pflL) (AccountView.keyPtr pflS) PErr.aliasedAccounts
  checkOwnedMagic dex  progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt  progId MktOff.size MAGIC_MARKET
  checkOwnedMagic pflL progId PflOff.size MAGIC_PORTFOLIO
  checkOwnedMagic pflS progId PflOff.size MAGIC_PORTFOLIO

  let dData  := AccountView.dataPtr dex
  let mData  := AccountView.dataPtr mkt
  let fLData := AccountView.dataPtr pflL
  let fSData := AccountView.dataPtr pflS
  requireKeyEq (AccountView.keyPtr dex)    (mData  + MktOff.dex)   PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex)    (fLData + PflOff.dex)   PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex)    (fSData + PflOff.dex)   PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr ownerL) (fLData + PflOff.owner) PErr.ownerMismatch
  requireKeyEq (AccountView.keyPtr ownerS) (fSData + PflOff.owner) PErr.ownerMismatch

  let mode      ← loadU8  ix.ptr 1
  let qty       ← loadU64 ix.ptr 2
  let execPrice ← if ix.len >= 18 then loadU64 ix.ptr 10 else pure 0
  SolanaM.require (qty > 0) PErr.invalidParams

  let st ← loadU8 mData MktOff.status
  let curSlot ← getClockSlot
  let mid ← loadU64 mData MktOff.marketId

  if mode == 0 then
    -- Open / Increase bilateral trade (supports post-ADL recovery when `oiSide > 0`!)
    SolanaM.require (st == STATUS_ACTIVE) PErr.invalidStatus
    checkOracleFresh mData curSlot

    let slotL ← findOrAllocLegSlot fLData (AccountView.keyPtr mkt) mid
    let slotS ← findOrAllocLegSlot fSData (AccountView.keyPtr mkt) mid
    let lpL := legPtrAt fLData slotL
    let lpS := legPtrAt fSData slotS
    let sL ← loadU8 lpL LegOff.side
    let sS ← loadU8 lpS LegOff.side
    SolanaM.require (sL == SIDE_FLAT || sL == SIDE_LONG)  PErr.invalidSide
    SolanaM.require (sS == SIDE_FLAT || sS == SIDE_SHORT) PErr.invalidSide

    let _ ← settleAndRefreshLeg dData mData fLData slotL curSlot false
    let _ ← settleAndRefreshLeg dData mData fSData slotS curSlot false
    applyExecSpread mData fLData fSData qty execPrice

    let oiL ← loadU64 mData MktOff.oiLong
    let oiS ← loadU64 mData MktOff.oiShort
    let rL  ← loadU64 mData MktOff.rawBasisLong
    let rS  ← loadU64 mData MktOff.rawBasisShort
    let maxOi ← loadU64 mData MktOff.maxOi
    let oiL' ← checkedAdd64 oiL qty
    let oiS' ← checkedAdd64 oiS qty
    SolanaM.require (oiL' <= maxOi && oiS' <= maxOi) PErr.oiLimitExceeded

    let rawAddL ← postAdlRawAddU64 qty oiL rL
    let rawAddS ← postAdlRawAddU64 qty oiS rS
    let rL' ← checkedAdd64 rL rawAddL
    let rS' ← checkedAdd64 rS rawAddS
    SolanaM.require (rL' <= MAX_RAW && rS' <= MAX_RAW && oiL' <= rL' && oiS' <= rS') PErr.oiLimitExceeded

    let markP ← loadU64 mData MktOff.markPrice
    let fee ← computeMarginReq qty markP (← loadU64 mData MktOff.tradeFeeBps)
    if fee > 0 then
      let cL ← loadU64 fLData PflOff.capital
      let cS ← loadU64 fSData PflOff.capital
      SolanaM.require (cL >= fee && cS >= fee) PErr.insufficientFunds
      storeU64 fLData PflOff.capital (cL - fee)
      storeU64 fSData PflOff.capital (cS - fee)
      let fee2 ← checkedAdd64 fee fee
      subU64At dData DexOff.totalCapital   fee2
      addU64At mData MktOff.insurance      fee2
      addU64At dData DexOff.totalInsurance fee2

    if sL == SIDE_FLAT then
      storeU8  lpL LegOff.side SIDE_LONG
      storeU64 lpL LegOff.entryK (← loadU64 mData MktOff.kLong)
      addU64At mData MktOff.activeLegs 1
    if sS == SIDE_FLAT then
      storeU8  lpS LegOff.side SIDE_SHORT
      storeU64 lpS LegOff.entryK (← loadU64 mData MktOff.kShort)
      addU64At mData MktOff.activeLegs 1

    addU64At lpL LegOff.rawBasis rawAddL
    addU64At lpS LegOff.rawBasis rawAddS
    storeU64 mData MktOff.oiLong oiL'
    storeU64 mData MktOff.oiShort oiS'
    storeU64 mData MktOff.rawBasisLong  rL'
    storeU64 mData MktOff.rawBasisShort rS'
    storeU64 mData MktOff.adlLong  (← deriveAdlU64 oiL' rL')
    storeU64 mData MktOff.adlShort (← deriveAdlU64 oiS' rS')

    syncLegMargins mData fLData slotL curSlot
    syncLegMargins mData fSData slotS curSlot

    -- Enforce same-slot freshness on ALL open legs of both portfolios + cross-margin IM check!
    checkAllLegsFreshLoop fLData curSlot 0
    checkAllLegsFreshLoop fSData curSlot 0
    SolanaM.require ((← loadU64 fLData PflOff.capital) >= (← loadU64 fLData PflOff.totalIm)) PErr.insufficientMargin
    SolanaM.require ((← loadU64 fSData PflOff.capital) >= (← loadU64 fSData PflOff.totalIm)) PErr.insufficientMargin
  else if mode == 1 then
    -- Mode 1: Bilateral risk-reducing close (works in Active & WindDown, post-ADL proportional!)
    SolanaM.require (st == STATUS_ACTIVE || st == STATUS_WINDDOWN) PErr.invalidStatus
    if st == STATUS_ACTIVE then checkOracleFresh mData curSlot

    let slotL ← findLegLoop fLData mid 0
    let slotS ← findLegLoop fSData mid 0
    SolanaM.require (slotL < MAX_LEGS && slotS < MAX_LEGS) PErr.legNotFound
    let lpL := legPtrAt fLData slotL
    let lpS := legPtrAt fSData slotS
    requireKeyEq (AccountView.keyPtr mkt) (lpL + LegOff.market) PErr.marketMismatch
    requireKeyEq (AccountView.keyPtr mkt) (lpS + LegOff.market) PErr.marketMismatch
    SolanaM.require ((← loadU8 lpL LegOff.side) == SIDE_LONG && (← loadU8 lpS LegOff.side) == SIDE_SHORT) PErr.invalidSide

    let _ ← settleAndRefreshLeg dData mData fLData slotL curSlot false
    let _ ← settleAndRefreshLeg dData mData fSData slotS curSlot false
    applyExecSpread mData fLData fSData qty execPrice

    let oiL ← loadU64 mData MktOff.oiLong
    let oiS ← loadU64 mData MktOff.oiShort
    SolanaM.require (qty <= oiL && qty <= oiS) PErr.invalidParams

    let rLegL ← loadU64 lpL LegOff.rawBasis
    let rLegS ← loadU64 lpS LegOff.rawBasis
    let rMktL ← loadU64 mData MktOff.rawBasisLong
    let rMktS ← loadU64 mData MktOff.rawBasisShort
    let redL ← proportionalRawRedU64 qty rLegL rMktL oiL
    let redS ← proportionalRawRedU64 qty rLegS rMktS oiS
    SolanaM.require (redL > 0 && redS > 0 && rLegL >= redL && rLegS >= redS &&
                     rMktL >= redL && rMktS >= redS) PErr.invalidParams

    let oiL' := oiL - qty
    let oiS' := oiS - qty
    let rMktL' := rMktL - redL
    let rMktS' := rMktS - redS
    SolanaM.require (oiL' <= rMktL' && oiS' <= rMktS') PErr.invalidParams

    storeU64 mData MktOff.oiLong oiL'
    storeU64 mData MktOff.oiShort oiS'
    storeU64 mData MktOff.rawBasisLong rMktL'
    storeU64 mData MktOff.rawBasisShort rMktS'
    storeU64 mData MktOff.adlLong  (← deriveAdlU64 oiL' rMktL')
    storeU64 mData MktOff.adlShort (← deriveAdlU64 oiS' rMktS')

    let rLegL' := rLegL - redL
    let rLegS' := rLegS - redS
    storeU64 lpL LegOff.rawBasis rLegL'
    storeU64 lpS LegOff.rawBasis rLegS'
    if rLegL' == 0 then
      storeU8 lpL LegOff.side SIDE_FLAT
      subU64At mData MktOff.activeLegs 1
    if rLegS' == 0 then
      storeU8 lpS LegOff.side SIDE_FLAT
      subU64At mData MktOff.activeLegs 1

    syncLegMargins mData fLData slotL curSlot
    syncLegMargins mData fSData slotS curSlot
    -- Paying an `execPrice` spread during reduce can NEVER drop payer below `newTotalMm`!
    let markP ← loadU64 mData MktOff.markPrice
    if execPrice != 0 && execPrice != markP then
      if execPrice > markP then
        SolanaM.require ((← loadU64 fLData PflOff.capital) >= (← loadU64 fLData PflOff.totalMm)) PErr.insufficientMargin
      else
        SolanaM.require ((← loadU64 fSData PflOff.capital) >= (← loadU64 fSData PflOff.totalMm)) PErr.insufficientMargin
    freeLegIfEmpty fLData slotL
    freeLegIfEmpty fSData slotS
  else if mode == 2 || mode == 3 then
    -- Mode 2 (Long→Long) / Mode 3 (Short→Short): Position Hand-Off (`pflL` seller → `pflS` buyer).
    -- Leaves `oiLong`, `oiShort`, `rawBasisLong`, `rawBasisShort`, `adlLong`, `adlShort` BIT-FOR-BIT INVARIANT!
    -- Works in Active & WindDown and even during active partial ADL!
    SolanaM.require (st == STATUS_ACTIVE || st == STATUS_WINDDOWN) PErr.invalidStatus
    if st == STATUS_ACTIVE then checkOracleFresh mData curSlot
    let targetSide := if mode == 2 then SIDE_LONG else SIDE_SHORT
    let oiSide ← loadU64 mData (if mode == 2 then MktOff.oiLong else MktOff.oiShort)
    let rMktSide ← loadU64 mData (if mode == 2 then MktOff.rawBasisLong else MktOff.rawBasisShort)
    SolanaM.require (oiSide > 0 && qty <= oiSide) PErr.invalidParams

    let slotSell ← findLegLoop fLData mid 0
    SolanaM.require (slotSell < MAX_LEGS) PErr.legNotFound
    let lpSell := legPtrAt fLData slotSell
    requireKeyEq (AccountView.keyPtr mkt) (lpSell + LegOff.market) PErr.marketMismatch
    SolanaM.require ((← loadU8 lpSell LegOff.side) == targetSide) PErr.invalidSide

    let slotBuy ← findOrAllocLegSlot fSData (AccountView.keyPtr mkt) mid
    let lpBuy := legPtrAt fSData slotBuy
    let sBuy ← loadU8 lpBuy LegOff.side
    SolanaM.require (sBuy == SIDE_FLAT || sBuy == targetSide) PErr.invalidSide

    let _ ← settleAndRefreshLeg dData mData fLData slotSell curSlot false
    let _ ← settleAndRefreshLeg dData mData fSData slotBuy  curSlot false
    if mode == 2 then
      applyExecSpread mData fSData fLData qty execPrice
    else
      applyExecSpread mData fLData fSData qty execPrice

    let rSell ← loadU64 lpSell LegOff.rawBasis
    let rawXfer ← proportionalRawRedU64 qty rSell rMktSide oiSide
    SolanaM.require (rawXfer > 0 && rSell >= rawXfer) PErr.invalidParams

    if sBuy == SIDE_FLAT then
      storeU8  lpBuy LegOff.side targetSide
      storeU64 lpBuy LegOff.entryK (← loadU64 mData (if mode == 2 then MktOff.kLong else MktOff.kShort))
      addU64At mData MktOff.activeLegs 1
    addU64At lpBuy LegOff.rawBasis rawXfer

    let rSell' := rSell - rawXfer
    storeU64 lpSell LegOff.rawBasis rSell'
    if rSell' == 0 then
      storeU8 lpSell LegOff.side SIDE_FLAT
      subU64At mData MktOff.activeLegs 1

    syncLegMargins mData fLData slotSell curSlot
    syncLegMargins mData fSData slotBuy  curSlot

    -- Buyer MUST satisfy full cross-margin IM with all legs fresh!
    checkAllLegsFreshLoop fSData curSlot 0
    SolanaM.require ((← loadU64 fSData PflOff.capital) >= (← loadU64 fSData PflOff.totalIm)) PErr.insufficientMargin
    -- If seller paid an `execPrice` spread, seller MUST remain above `totalMm`!
    let markP ← loadU64 mData MktOff.markPrice
    if execPrice != 0 && execPrice != markP then
      let sellerPaid := (mode == 2 && execPrice < markP) || (mode == 3 && execPrice > markP)
      if sellerPaid then
        SolanaM.require ((← loadU64 fLData PflOff.capital) >= (← loadU64 fLData PflOff.totalMm)) PErr.insufficientMargin
    freeLegIfEmpty fLData slotSell
  else
    SolanaM.fail PErr.invalidParams
  log64 EvtTag.trade ((mid <<< 8) ||| mode.toUInt64) qty execPrice (← loadU64 mData MktOff.markPrice)

/-- Fair, transparent $O(1)$ liquidation supporting:
    - Partial OR full liquidation (`reqQty = 0` → full leg; `reqQty > 0` → partial `min(reqQty, rawB)`),
    - Gradual deficit-scaled liquidation fee (`min(maxFee, max(maxFee/4, deficit))`, strictly above `otherMm`),
    - `mode = 1`: Transparent HLP/counterparty position takeover (9 accs, **ZERO ADL!**),
    - `mode = 0`: Proportional OI-reduction + capped `I_m` draw + derived ADL fallback (7 accs). -/
def handleLiquidate (input : Ptr) : SolanaM Unit := do
  let (n, dex, ix, progId) ← parseIxOnly input
  SolanaM.require (n >= 7) Err.notEnoughAccountKeys
  let mkt      ← AccountView.nextCursor dex
  let pfl      ← AccountView.nextCursor mkt
  let liqTok   ← AccountView.nextCursor pfl
  let mint     ← AccountView.nextCursor liqTok
  let vaultPda ← AccountView.nextCursor mint
  let tokProg  ← AccountView.nextCursor vaultPda

  let mode   ← loadOptU8  ix.ptr ix.len 1
  let reqQty ← loadOptU64 ix.ptr ix.len 2

  requireWritable dex; requireWritable mkt; requireWritable pfl
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  checkOwnedMagic pfl progId PflOff.size MAGIC_PORTFOLIO
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  let fData := AccountView.dataPtr pfl
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex) PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex) (fData + PflOff.dex) PErr.dexMismatch
  checkDexVaultAndMint dData mint vaultPda tokProg

  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_ACTIVE) PErr.invalidStatus
  let curSlot ← getClockSlot
  checkOracleFresh mData curSlot

  let mid ← loadU64 mData MktOff.marketId
  let slotIdx ← findLegLoop fData mid 0
  SolanaM.require (slotIdx < MAX_LEGS) PErr.legNotFound
  let lp := legPtrAt fData slotIdx
  requireKeyEq (AccountView.keyPtr mkt) (lp + LegOff.market) PErr.marketMismatch
  let side ← loadU8 lp LegOff.side
  let rawB ← loadU64 lp LegOff.rawBasis
  SolanaM.require (side != SIDE_FLAT && rawB > 0) PErr.invalidSide

  let rawClose := if reqQty == 0 || reqQty >= rawB then rawB else reqQty
  let remRaw   := rawB - rawClose
  let shortfall ← settleAndRefreshLeg dData mData fData slotIdx curSlot false
  -- A1 Fix: require ALL open legs on victim to be refreshed in `curSlot` BEFORE evaluating
  -- portfolio solvency, preventing selective single-leg liquidation of a hedged cross-margin spread!
  checkAllLegsFreshLoop fData curSlot 0

  let cap    ← loadU64 fData PflOff.capital
  let posPnl ← loadU64 lp LegOff.posPnl
  let totMm  ← loadU64 fData PflOff.totalMm
  let eq ← checkedAdd64 cap posPnl
  SolanaM.require (shortfall > 0 || eq < totMm) PErr.positionHealthy

  -- Fair gradual deficit-scaled liquidation fee strictly above `otherMm` (anti-reverse-contagion!)
  let markP ← loadU64 mData MktOff.markPrice
  let adlSide ← loadU64 mData (if side == SIDE_LONG then MktOff.adlLong else MktOff.adlShort)
  let closeEff ← mulDiv64 rawClose adlSide ADL_ONE
  let notional ← mulDiv64Ceil closeEff markP POS_SCALE
  let maxFee ← mulDiv64 notional (← loadU64 mData MktOff.liqFeeBps) BPS_DEN
  let deficit := if totMm > eq then totMm - eq else 0
  let feeFloor := maxFee >>> 2
  let targetFee :=
    if shortfall > 0 then maxFee
    else
      let g := if deficit > feeFloor then deficit else feeFloor
      if g < maxFee then g else maxFee
  let legRes ← loadU64 lp LegOff.reservedMm
  let totRes ← loadU64 fData PflOff.totalReservedMm
  let otherRes := if totRes >= legRes then totRes - legRes else 0
  let availForFee := if cap >= otherRes then cap - otherRes else 0
  let liqFee := if targetFee < availForFee then targetFee else availForFee
  if liqFee > 0 then
    storeU64 fData PflOff.capital (cap - liqFee)
    subU64At dData DexOff.totalCapital liqFee
    subU64At dData DexOff.vaultBalance liqFee
    let dec  ← loadU8 dData DexOff.decimals
    let bump ← loadU8 dData DexOff.vaultBump
    tokenTransferCheckedSigned tokProg vaultPda mint liqTok vaultPda liqFee dec
      TAG_DEX_VAULT (AccountView.keyPtr dex) bump

  -- Capped draw from THIS market's isolated insurance `I_m` into `R_m`
  if shortfall > 0 then
    let ins ← loadU64 mData MktOff.insurance
    let maxDraw ← mulDiv64 ins (← loadU64 mData MktOff.maxInsDrawBps) BPS_DEN
    let draw1 := if shortfall < maxDraw then shortfall else maxDraw
    let insDraw := if draw1 < ins then draw1 else ins
    if insDraw > 0 then
      subU64At mData MktOff.insurance      insDraw
      subU64At dData DexOff.totalInsurance insDraw
      addU64At mData MktOff.realizedPool   insDraw
      addU64At dData DexOff.totalResidual  insDraw

  -- Prevent partial liquidation from leaving a dust leg below `minMm`!
  if remRaw > 0 then
    let minMm ← loadU64 mData MktOff.minMm
    let remEff ← mulDiv64 remRaw adlSide ADL_ONE
    let remNot ← mulDiv64Ceil remEff markP POS_SCALE
    SolanaM.require (remNot >= minMm) PErr.invalidParams

  if mode == 1 then
    -- Tier 1/2 Transparent HLP / Counterparty Takeover (**ZERO ADL!**)
    SolanaM.require (n >= 9) Err.notEnoughAccountKeys
    let liqPfl   ← AccountView.nextCursor tokProg
    let liqOwner ← AccountView.nextCursor liqPfl
    requireSigner liqOwner; requireWritable liqPfl
    requireKeyNe (AccountView.keyPtr pfl) (AccountView.keyPtr liqPfl) PErr.aliasedAccounts
    checkOwnedMagic liqPfl progId PflOff.size MAGIC_PORTFOLIO
    let lData := AccountView.dataPtr liqPfl
    requireKeyEq (AccountView.keyPtr dex)      (lData + PflOff.dex)   PErr.dexMismatch
    requireKeyEq (AccountView.keyPtr liqOwner) (lData + PflOff.owner) PErr.ownerMismatch
    -- A2 Fix: Takeover preserves `oiSide`, `rawBasisSide`, and `adlSide` bit-for-bit, so it works
    -- even during active partial ADL! Only reject 100%-wiped-out dead legs (`oiSide == 0`).
    let oiSide ← loadU64 mData (if side == SIDE_LONG then MktOff.oiLong else MktOff.oiShort)
    SolanaM.require (oiSide > 0) PErr.adlHaircutActive

    let liqSlot ← findOrAllocLegSlot lData (AccountView.keyPtr mkt) mid
    let liqLp := legPtrAt lData liqSlot
    let liqSide ← loadU8 liqLp LegOff.side
    SolanaM.require (liqSide == SIDE_FLAT || liqSide == side) PErr.invalidSide
    let _ ← settleAndRefreshLeg dData mData lData liqSlot curSlot false
    if liqSide == SIDE_FLAT then
      storeU8  liqLp LegOff.side side
      storeU64 liqLp LegOff.entryK (← loadU64 mData (if side == SIDE_LONG then MktOff.kLong else MktOff.kShort))
      addU64At mData MktOff.activeLegs 1
    addU64At liqLp LegOff.rawBasis rawClose
    syncLegMargins mData lData liqSlot curSlot
    checkAllLegsFreshLoop lData curSlot 0
    SolanaM.require ((← loadU64 lData PflOff.capital) >= (← loadU64 lData PflOff.totalIm)) PErr.insufficientMargin
  else
    -- Tier 3 Fallback: Proportional OI reduction (`ceil` guarantees `oi' <= rSide'`!) + derived ADL
    let oiL ← loadU64 mData MktOff.oiLong
    let oiS ← loadU64 mData MktOff.oiShort
    let rL  ← loadU64 mData MktOff.rawBasisLong
    let rS  ← loadU64 mData MktOff.rawBasisShort
    let minOi := if oiL < oiS then oiL else oiS
    let oiSide  := if side == SIDE_LONG then oiL else oiS
    let rawSide := if side == SIDE_LONG then rL  else rS
    let effProp ← proportionalOiForRawU64 rawClose oiSide rawSide
    let eff := if effProp < minOi then effProp else minOi
    let oiL' := oiL - eff
    let oiS' := oiS - eff
    let rL' ← checkedSub64 rL (if side == SIDE_LONG  then rawClose else 0)
    let rS' ← checkedSub64 rS (if side == SIDE_SHORT then rawClose else 0)
    SolanaM.require (oiL' <= rL' && oiS' <= rS') PErr.invalidParams
    storeU64 mData MktOff.oiLong oiL'
    storeU64 mData MktOff.oiShort oiS'
    storeU64 mData MktOff.rawBasisLong rL'
    storeU64 mData MktOff.rawBasisShort rS'
    storeU64 mData MktOff.adlLong  (← deriveAdlU64 oiL' rL')
    storeU64 mData MktOff.adlShort (← deriveAdlU64 oiS' rS')

  -- Update or flatten victim's leg & sync cached margins
  storeU64 lp LegOff.rawBasis remRaw
  if remRaw == 0 then
    storeU8 lp LegOff.side SIDE_FLAT
    subU64At mData MktOff.activeLegs 1
  syncLegMargins mData fData slotIdx curSlot
  freeLegIfEmpty fData slotIdx
  log64 EvtTag.liquidate ((mid <<< 8) ||| mode.toUInt64) rawClose liqFee shortfall

def handleClearDeadLeg (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, pfl, _, progId) ← parse3 input
  requireWritable dex; requireWritable mkt; requireWritable pfl
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  checkOwnedMagic pfl progId PflOff.size MAGIC_PORTFOLIO
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  let fData := AccountView.dataPtr pfl
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex) PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex) (fData + PflOff.dex) PErr.dexMismatch

  let st ← loadU8 mData MktOff.status
  SolanaM.require (st == STATUS_ACTIVE || st == STATUS_WINDDOWN) PErr.invalidStatus
  let curSlot ← getClockSlot
  let mid ← loadU64 mData MktOff.marketId
  let slotIdx ← findLegLoop fData mid 0
  SolanaM.require (slotIdx < MAX_LEGS) PErr.legNotFound
  let lp := legPtrAt fData slotIdx
  requireKeyEq (AccountView.keyPtr mkt) (lp + LegOff.market) PErr.marketMismatch

  let side ← loadU8 lp LegOff.side
  let rawB ← loadU64 lp LegOff.rawBasis
  SolanaM.require (side != SIDE_FLAT && rawB > 0) PErr.invalidSide
  let _ ← settleAndRefreshLeg dData mData fData slotIdx curSlot false

  if side == SIDE_LONG then
    SolanaM.require ((← loadU64 mData MktOff.oiLong) == 0) PErr.nonzeroOi
    let rL' ← checkedSub64 (← loadU64 mData MktOff.rawBasisLong) rawB
    storeU64 mData MktOff.rawBasisLong rL'
    storeU64 mData MktOff.adlLong (← deriveAdlU64 0 rL')
  else
    SolanaM.require ((← loadU64 mData MktOff.oiShort) == 0) PErr.nonzeroOi
    let rS' ← checkedSub64 (← loadU64 mData MktOff.rawBasisShort) rawB
    storeU64 mData MktOff.rawBasisShort rS'
    storeU64 mData MktOff.adlShort (← deriveAdlU64 0 rS')

  storeU8  lp LegOff.side SIDE_FLAT
  storeU64 lp LegOff.rawBasis 0
  subU64At mData MktOff.activeLegs 1
  syncLegMargins mData fData slotIdx curSlot
  freeLegIfEmpty fData slotIdx

def handleStartWindDown (input : Ptr) : SolanaM Unit := do
  let (mkt, caller, _, progId) ← parse2 input
  requireSigner caller; requireWritable mkt
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  let mData := AccountView.dataPtr mkt
  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_ACTIVE) PErr.invalidStatus

  let curSlot ← getClockSlot
  let isAdmin ← do
    if (← pubkeyIsZero (mData + MktOff.marketAdmin)) then pure false
    else pubkeyEq (AccountView.keyPtr caller) (mData + MktOff.marketAdmin)
  if !isAdmin then
    let lastSlot ← loadU64 mData MktOff.lastOracleSlot
    let maxStale ← loadU64 mData MktOff.maxStaleness
    let age := if curSlot >= lastSlot then curSlot - lastSlot else 0
    SolanaM.require (age > maxStale) PErr.oracleNotStale

  storeU8  mData MktOff.status STATUS_WINDDOWN
  storeU64 mData MktOff.windDownSlot curSlot

def handleForceClosePair (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, pflL, pflS, _, progId) ← parse4 input
  requireWritable dex; requireWritable mkt; requireWritable pflL; requireWritable pflS
  requireKeyNe (AccountView.keyPtr pflL) (AccountView.keyPtr pflS) PErr.aliasedAccounts
  checkOwnedMagic dex  progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt  progId MktOff.size MAGIC_MARKET
  checkOwnedMagic pflL progId PflOff.size MAGIC_PORTFOLIO
  checkOwnedMagic pflS progId PflOff.size MAGIC_PORTFOLIO
  let dData  := AccountView.dataPtr dex
  let mData  := AccountView.dataPtr mkt
  let fLData := AccountView.dataPtr pflL
  let fSData := AccountView.dataPtr pflS
  requireKeyEq (AccountView.keyPtr dex) (mData  + MktOff.dex) PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex) (fLData + PflOff.dex) PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex) (fSData + PflOff.dex) PErr.dexMismatch

  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_WINDDOWN) PErr.invalidStatus
  let curSlot ← getClockSlot
  let unlockSlot ← checkedAdd64 (← loadU64 mData MktOff.windDownSlot) (← loadU64 mData MktOff.exitWindow)
  SolanaM.require (curSlot >= unlockSlot) PErr.exitWindowActive

  let mid ← loadU64 mData MktOff.marketId
  let slotL ← findLegLoop fLData mid 0
  let slotS ← findLegLoop fSData mid 0
  SolanaM.require (slotL < MAX_LEGS && slotS < MAX_LEGS) PErr.legNotFound
  let lpL := legPtrAt fLData slotL
  let lpS := legPtrAt fSData slotS
  requireKeyEq (AccountView.keyPtr mkt) (lpL + LegOff.market) PErr.marketMismatch
  requireKeyEq (AccountView.keyPtr mkt) (lpS + LegOff.market) PErr.marketMismatch
  SolanaM.require ((← loadU8 lpL LegOff.side) == SIDE_LONG && (← loadU8 lpS LegOff.side) == SIDE_SHORT) PErr.invalidSide

  let _ ← settleAndRefreshLeg dData mData fLData slotL curSlot false
  let _ ← settleAndRefreshLeg dData mData fSData slotS curSlot false

  let rLegL ← loadU64 lpL LegOff.rawBasis
  let rLegS ← loadU64 lpS LegOff.rawBasis
  SolanaM.require (rLegL > 0 && rLegS > 0) PErr.invalidParams

  let eL0 ← legEffectiveQty mData lpL
  let eS0 ← legEffectiveQty mData lpS
  let effL := if eL0 == 0 then 1 else eL0
  let effS := if eS0 == 0 then 1 else eS0
  let oiL ← loadU64 mData MktOff.oiLong
  let oiS ← loadU64 mData MktOff.oiShort
  let q1 := if effL < effS then effL else effS
  let q2 := if oiL < oiS then oiL else oiS
  let q  := if q1 < q2 then q1 else q2
  SolanaM.require (q > 0) PErr.nonzeroOi

  let rMktL ← loadU64 mData MktOff.rawBasisLong
  let rMktS ← loadU64 mData MktOff.rawBasisShort
  let redL ← proportionalRawRedU64 q rLegL rMktL oiL
  let redS ← proportionalRawRedU64 q rLegS rMktS oiS
  SolanaM.require (redL > 0 && redS > 0 && rLegL >= redL && rLegS >= redS &&
                   rMktL >= redL && rMktS >= redS) PErr.invalidParams

  let oiL' := oiL - q
  let oiS' := oiS - q
  let rMktL' := rMktL - redL
  let rMktS' := rMktS - redS
  SolanaM.require (oiL' <= rMktL' && oiS' <= rMktS') PErr.invalidParams

  storeU64 mData MktOff.oiLong oiL'
  storeU64 mData MktOff.oiShort oiS'
  storeU64 mData MktOff.rawBasisLong rMktL'
  storeU64 mData MktOff.rawBasisShort rMktS'
  storeU64 mData MktOff.adlLong  (← deriveAdlU64 oiL' rMktL')
  storeU64 mData MktOff.adlShort (← deriveAdlU64 oiS' rMktS')

  let rLegL' := rLegL - redL
  let rLegS' := rLegS - redS
  storeU64 lpL LegOff.rawBasis rLegL'
  storeU64 lpS LegOff.rawBasis rLegS'
  if rLegL' == 0 then
    storeU8 lpL LegOff.side SIDE_FLAT
    subU64At mData MktOff.activeLegs 1
  if rLegS' == 0 then
    storeU8 lpS LegOff.side SIDE_FLAT
    subU64At mData MktOff.activeLegs 1

  syncLegMargins mData fLData slotL curSlot
  syncLegMargins mData fSData slotS curSlot
  freeLegIfEmpty fLData slotL
  freeLegIfEmpty fSData slotS
  log64 EvtTag.forceClose mid q oiL' curSlot

def handleResolveMarket (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, _, progId) ← parse2 input
  requireWritable dex; requireWritable mkt
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex) PErr.dexMismatch
  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_WINDDOWN) PErr.invalidStatus
  SolanaM.require ((← loadU64 mData MktOff.oiLong) == 0 && (← loadU64 mData MktOff.oiShort) == 0 &&
                   (← loadU64 mData MktOff.rawBasisLong) == 0 && (← loadU64 mData MktOff.rawBasisShort) == 0 &&
                   (← loadU64 mData MktOff.activeLegs) == 0) PErr.nonzeroOi

  let rPool  ← loadU64 mData MktOff.realizedPool
  let totPnl ← loadU64 mData MktOff.totalPosPnl
  let resNum := if rPool < totPnl then rPool else totPnl
  storeU64 mData MktOff.resolvedNum resNum
  storeU64 mData MktOff.resolvedDen totPnl
  storeU8  mData MktOff.status STATUS_RESOLVED
  sweepResolvedDust dData mData
  log64 EvtTag.resolve (← loadU64 mData MktOff.marketId) resNum totPnl 0

def handleClaimResolved (input : Ptr) : SolanaM Unit := do
  let (dex, mkt, pfl, _, progId) ← parse3 input
  requireWritable dex; requireWritable mkt; requireWritable pfl
  checkOwnedMagic dex progId DexOff.size MAGIC_DEX
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  checkOwnedMagic pfl progId PflOff.size MAGIC_PORTFOLIO
  let dData := AccountView.dataPtr dex
  let mData := AccountView.dataPtr mkt
  let fData := AccountView.dataPtr pfl
  requireKeyEq (AccountView.keyPtr dex) (mData + MktOff.dex) PErr.dexMismatch
  requireKeyEq (AccountView.keyPtr dex) (fData + PflOff.dex) PErr.dexMismatch

  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_RESOLVED) PErr.invalidStatus
  let mid ← loadU64 mData MktOff.marketId
  let slotIdx ← findLegLoop fData mid 0
  SolanaM.require (slotIdx < MAX_LEGS) PErr.legNotFound
  let lp := legPtrAt fData slotIdx
  requireKeyEq (AccountView.keyPtr mkt) (lp + LegOff.market) PErr.marketMismatch
  SolanaM.require ((← loadU8 lp LegOff.side) == SIDE_FLAT && (← loadU64 lp LegOff.rawBasis) == 0) PErr.nonzeroOi

  let posPnl ← loadU64 lp LegOff.posPnl
  let resNum ← loadU64 mData MktOff.resolvedNum
  let resDen ← loadU64 mData MktOff.resolvedDen
  let pay ← if resDen == 0 || posPnl == 0 then pure 0 else mulDiv64 posPnl resNum resDen
  if pay > 0 then
    subU64At mData MktOff.realizedPool pay
    subU64At dData DexOff.totalResidual pay
    addU64At fData PflOff.capital pay
    addU64At dData DexOff.totalCapital pay
  if posPnl > 0 then subU64At mData MktOff.totalPosPnl posPnl

  storeU64 lp LegOff.posPnl 0
  storeU8  lp LegOff.active 0
  let cnt ← loadU8 fData PflOff.legCount
  if cnt > 0 then storeU8 fData PflOff.legCount (cnt - 1)
  sweepResolvedDust dData mData

def handleTombstoneMarket (input : Ptr) : SolanaM Unit := do
  let (mkt, _, progId) ← parse1 input
  requireWritable mkt
  checkOwnedMagic mkt progId MktOff.size MAGIC_MARKET
  let mData := AccountView.dataPtr mkt
  SolanaM.require ((← loadU8 mData MktOff.status) == STATUS_RESOLVED) PErr.invalidStatus
  SolanaM.require ((← loadU64 mData MktOff.oiLong) == 0 && (← loadU64 mData MktOff.oiShort) == 0 &&
                   (← loadU64 mData MktOff.rawBasisLong) == 0 && (← loadU64 mData MktOff.rawBasisShort) == 0 &&
                   (← loadU64 mData MktOff.insurance) == 0 && (← loadU64 mData MktOff.realizedPool) == 0 &&
                   (← loadU64 mData MktOff.activeLegs) == 0 && (← loadU64 mData MktOff.totalPosPnl) == 0) PErr.nonzeroState
  storeU8 mData MktOff.status STATUS_TOMBSTONED

/-- Unified rent reclaim for empty `Portfolio`, empty `InsReceipt`, or `Tombstoned` `Market`.
    Zeroes ALL account bytes AND writes permanent `MAGIC_CLOSED` ("DEAD", ≠ 0) before draining
    lamports so a closed account can NEVER be revived (neither as initialized nor as fresh `magic == 0`)
    even if rent lamports are refunded in the same transaction! -/
def handleCloseAccount (input : Ptr) : SolanaM Unit := do
  let (acc, authority, dest, _, progId) ← parse3 input
  requireSigner authority; requireWritable acc; requireWritable dest
  requireKeyNe (AccountView.keyPtr acc) (AccountView.keyPtr dest) PErr.aliasedAccounts
  requireKeyEq (AccountView.ownerPtr acc) progId Err.illegalOwner
  let dlen ← AccountView.dataLen acc
  let data := AccountView.dataPtr acc
  let magic ← loadU32 data 0
  let closeWords ←
    if magic == MAGIC_PORTFOLIO then do
      SolanaM.require (dlen == PflOff.size) Err.invalidAccountData
      requireKeyEq (AccountView.keyPtr authority) (data + PflOff.owner) PErr.ownerMismatch
      SolanaM.require ((← loadU64 data PflOff.capital) == 0 && (← loadU8 data PflOff.legCount) == 0 &&
                       (← loadU64 data PflOff.totalMm) == 0 && (← loadU64 data PflOff.totalIm) == 0 &&
                       (← loadU64 data PflOff.totalReservedMm) == 0) PErr.nonzeroState
      pure (PflOff.size >>> 3)
    else if magic == MAGIC_INSURANCE then do
      SolanaM.require (dlen == InsOff.size) Err.invalidAccountData
      requireKeyEq (AccountView.keyPtr authority) (data + InsOff.owner) PErr.ownerMismatch
      SolanaM.require ((← loadU64 data InsOff.shares) == 0) PErr.nonzeroState
      pure (InsOff.size >>> 3)
    else if magic == MAGIC_MARKET then do
      SolanaM.require (dlen == MktOff.size) Err.invalidAccountData
      SolanaM.require ((← loadU8 data MktOff.status) == STATUS_TOMBSTONED) PErr.invalidStatus
      if !(← pubkeyIsZero (data + MktOff.marketAdmin)) then
        requireKeyEq (AccountView.keyPtr authority) (data + MktOff.marketAdmin) PErr.unauthorized
      pure (MktOff.size >>> 3)
    else
      SolanaM.fail PErr.invalidMagic

  zeroWordsLoop data 0 closeWords
  storeU32 data 0 MAGIC_CLOSED
  let accLam  ← AccountView.lamports acc
  let destLam ← AccountView.lamports dest
  AccountView.setLamports dest (← checkedAdd64 destLam accLam)
  AccountView.setLamports acc 0

/-! ### Top-Level Solana Entrypoint (`lean-svm` lowers opcode dispatch to O(log2 20) binary search!) -/

def dispatch (input : Ptr) : SolanaM Unit := do
  let (_, _, ix, _) ← parseIxOnly input
  SolanaM.require (ix.len >= 1) Err.invalidInstructionData
  let op ← loadU8 ix.ptr 0
  if op == IxOp.initDex                 then handleInitDex input
  else if op == IxOp.updateCreateFee    then handleUpdateCreateFee input
  else if op == IxOp.createMarket       then handleCreateMarket input
  else if op == IxOp.updateMarketAuth   then handleUpdateMarketAuth input
  else if op == IxOp.updateOracle       then handleUpdateOracle input
  else if op == IxOp.initPortfolio      then handleInitPortfolio input
  else if op == IxOp.depositCollateral  then handleDepositCollateral input
  else if op == IxOp.withdrawCollateral then handleWithdrawCollateral input
  else if op == IxOp.depositInsurance   then handleDepositInsurance input
  else if op == IxOp.withdrawInsurance  then handleWithdrawInsurance input
  else if op == IxOp.refreshLeg         then handleRefreshLeg input
  else if op == IxOp.trade              then handleTrade input
  else if op == IxOp.liquidate          then handleLiquidate input
  else if op == IxOp.clearDeadLeg       then handleClearDeadLeg input
  else if op == IxOp.startWindDown      then handleStartWindDown input
  else if op == IxOp.forceClosePair     then handleForceClosePair input
  else if op == IxOp.resolveMarket      then handleResolveMarket input
  else if op == IxOp.claimResolved      then handleClaimResolved input
  else if op == IxOp.tombstoneMarket    then handleTombstoneMarket input
  else if op == IxOp.closeAccount       then handleCloseAccount input
  else SolanaM.fail PErr.invalidInstruction

def entrypoint (input : Ptr) (s : SvmState) : UInt64 × SvmState :=
  SolanaM.runEntry (dispatch input) s

end Percolator.Program
