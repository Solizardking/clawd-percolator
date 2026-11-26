/-
  Percolator.Layout
  Shared constants, pinned canonical SPL Token Program ID, error codes, and exact
  zero-copy byte layouts for `percolator-lean` v2 (Realizable Shared Cross-Margin
  with Provable Per-Market Risk Isolation).

  Single unified codebase: zero engine/program separation.
-/
import LeanSVM

namespace Percolator

/-! ### Protocol Scales & Hard Bounds -/

def POS_SCALE        : UInt64 := 1_000_000          -- 10^6 (size & price fixed-point scale)
def ADL_ONE          : UInt64 := 1_000_000_000      -- 10^9 (derived side ADL index scale)
def BPS_DEN          : UInt64 := 10_000             -- basis points denominator
def K_ZERO           : UInt64 := 0x4000000000000000 -- 2^62 unbiased zero for cumulative K_side
def MAX_PRICE        : UInt64 := 1_000_000_000_000      -- $1,000,000 at E6
def MAX_OI           : UInt64 := 10_000_000_000_000     -- 10,000,000 units at E6
def MAX_RAW          : UInt64 := 10_000_000_000_000_000 -- 1000x MAX_OI post-ADL rawBasis cap
def MAX_CREATE_FEE   : UInt64 := 10_000_000_000     -- 10,000 quote units at E6
def MIN_EXIT_WINDOW  : UInt64 := 10                 -- minimum wind-down exit window (slots)
def MAX_EXIT_WINDOW  : UInt64 := 216_000            -- maximum wind-down exit window (~24h at 400ms/slot)
def MAX_STALENESS    : UInt64 := 9_000              -- maximum oracle staleness (~1h at 400ms/slot)
def MAX_LEGS         : UInt64 := 8                  -- max cross-margin legs per Portfolio

/-! ### Pinned Canonical SPL Token Program ID (`TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`) -/

def SPL_TOK_W0 : UInt64 := 0x93A165D7E1F6DD06
def SPL_TOK_W1 : UInt64 := 0xAC79EBCE46E1CBD9
def SPL_TOK_W2 : UInt64 := 0x91375B5FED85B41C
def SPL_TOK_W3 : UInt64 := 0xA900FF7E85F58C3A

/-! ### Account Type Magics (4-byte little-endian ASCII) -/

def MAGIC_DEX        : UInt32 := 0x58454450         -- "PDEX"
def MAGIC_MARKET     : UInt32 := 0x50544B4D         -- "MKTP"
def MAGIC_PORTFOLIO  : UInt32 := 0x4F4C4650         -- "PFLO"
def MAGIC_INSURANCE  : UInt32 := 0x52534E49         -- "INSR"
def MAGIC_CLOSED     : UInt32 := 0x44454144         -- "DEAD" (permanent tombstone; blocks same-tx revival!)

/-! ### PDA Seed Tag (4-byte little-endian ASCII: "dvlt") -/

def TAG_DEX_VAULT    : UInt32 := 0x746C7664         -- "dvlt"

/-! ### Market Lifecycle Statuses -/

def STATUS_ACTIVE     : UInt8 := 0
def STATUS_WINDDOWN   : UInt8 := 1
def STATUS_RESOLVED   : UInt8 := 2
def STATUS_TOMBSTONED : UInt8 := 3

/-! ### Position Sides -/

def SIDE_FLAT  : UInt8 := 0
def SIDE_LONG  : UInt8 := 1
def SIDE_SHORT : UInt8 := 2

/-! ### Program Error Codes (custom >= 100) -/

namespace PErr
  def invalidMagic       : UInt64 := 100
  def invalidStatus      : UInt64 := 101
  def unauthorized       : UInt64 := 102
  def invalidParams      : UInt64 := 103
  def oracleClamp        : UInt64 := 104
  def oracleNotStale     : UInt64 := 105
  def oracleStale        : UInt64 := 106
  def insufficientMargin : UInt64 := 107
  def insufficientFunds  : UInt64 := 108
  def aliasedAccounts    : UInt64 := 109
  def marketMismatch     : UInt64 := 110
  def ownerMismatch      : UInt64 := 111
  def mintMismatch       : UInt64 := 112
  def vaultMismatch      : UInt64 := 113
  def oiLimitExceeded    : UInt64 := 114
  def adlHaircutActive   : UInt64 := 115
  def positionHealthy    : UInt64 := 116
  def exitWindowActive   : UInt64 := 117
  def nonzeroOi          : UInt64 := 118
  def nonzeroState       : UInt64 := 119
  def invalidTokenProg   : UInt64 := 120
  def invalidSide        : UInt64 := 121
  def stalePortfolioLeg  : UInt64 := 122
  def invalidInstruction : UInt64 := 123
  def portfolioFull      : UInt64 := 124
  def legNotFound        : UInt64 := 125
  def oracleSameSlot     : UInt64 := 126
  def dexMismatch        : UInt64 := 127
  def invalidVaultState  : UInt64 := 128
  def invalidSysProg     : UInt64 := 129
  def execPriceClamp     : UInt64 := 130
end PErr

/-! ### Exact Byte Offsets: `Dex` (200 bytes) -/

namespace DexOff
  def size            : UInt64 := 200
  def magic           : UInt64 := 0   -- u32 ("PDEX")
  def version         : UInt64 := 4   -- u8  (2)
  def vaultBump       : UInt64 := 5   -- u8
  def decimals        : UInt64 := 6   -- u8
  def admin           : UInt64 := 8   -- Pubkey (32)
  def quoteMint       : UInt64 := 40  -- Pubkey (32)
  def vaultPda        : UInt64 := 72  -- Pubkey (32)
  def feeVault        : UInt64 := 104 -- Pubkey (32)
  def nextMarketId    : UInt64 := 136 -- u64
  def nextPortfolioId : UInt64 := 144 -- u64
  def createFee       : UInt64 := 152 -- u64
  def vaultBalance    : UInt64 := 160 -- u64 (V)
  def totalCapital    : UInt64 := 168 -- u64 (C_tot)
  def totalInsurance  : UInt64 := 176 -- u64 (I_tot)
  def totalResidual   : UInt64 := 184 -- u64 (R_tot)
  def totalSurplus    : UInt64 := 192 -- u64 (S_tot)
end DexOff

/-! ### Exact Byte Offsets: `Market` (344 bytes — Per-Market Isolated Risk Domain) -/

namespace MktOff
  def size           : UInt64 := 352
  def magic          : UInt64 := 0   -- u32 ("MKTP")
  def status         : UInt64 := 4   -- u8
  def marketId       : UInt64 := 8   -- u64
  def dex            : UInt64 := 16  -- Pubkey (32)
  def oracleAuth     : UInt64 := 48  -- Pubkey (32)
  def marketAdmin    : UInt64 := 80  -- Pubkey (32) (0 = burned)
  def markPrice      : UInt64 := 112 -- u64 (E6)
  def lastOracleSlot : UInt64 := 120 -- u64
  def windDownSlot   : UInt64 := 128 -- u64
  def exitWindow     : UInt64 := 136 -- u64
  def maxStaleness   : UInt64 := 144 -- u64
  def warmupSlots    : UInt64 := 152 -- u64 (0 = instant backed realization!)
  def imBps          : UInt64 := 160 -- u64
  def mmBps          : UInt64 := 168 -- u64
  def liqFeeBps      : UInt64 := 176 -- u64
  def tradeFeeBps    : UInt64 := 184 -- u64
  def maxMoveBps     : UInt64 := 192 -- u64
  def maxInsDrawBps  : UInt64 := 200 -- u64
  def maxOi          : UInt64 := 208 -- u64
  def realizedPool   : UInt64 := 216 -- u64 (R_m: isolated market-m loser backing!)
  def insurance      : UInt64 := 224 -- u64 (I_m: isolated market-m insurance!)
  def feeSurplus     : UInt64 := 232 -- u64 (S_m: rounding residue sink!)
  def totalPosPnl    : UInt64 := 240 -- u64 (P^+_m)
  def totalInsShares : UInt64 := 248 -- u64
  def oiLong         : UInt64 := 256 -- u64
  def oiShort        : UInt64 := 264 -- u64
  def rawBasisLong   : UInt64 := 272 -- u64
  def rawBasisShort  : UInt64 := 280 -- u64
  def adlLong        : UInt64 := 288 -- u64
  def adlShort       : UInt64 := 296 -- u64
  def kLong          : UInt64 := 304 -- u64 (biased by K_ZERO)
  def kShort         : UInt64 := 312 -- u64 (biased by K_ZERO)
  def resolvedNum    : UInt64 := 320 -- u64
  def resolvedDen    : UInt64 := 328 -- u64
  def activeLegs     : UInt64 := 336 -- u64
  def minMm          : UInt64 := 344 -- u64 (minimum nonzero MM floor; prevents dust griefing)
end MktOff

/-! ### Exact Byte Offsets: `Portfolio` (952 bytes = 120B header + 8 × 104B `Leg` slots) -/

namespace PflOff
  def headerSize      : UInt64 := 120
  def legStride       : UInt64 := 104
  def size            : UInt64 := 952  -- 120 + 8 * 104
  def magic           : UInt64 := 0    -- u32 ("PFLO")
  def legCount        : UInt64 := 4    -- u8 (0..8)
  def portfolioId     : UInt64 := 8    -- u64 (monotonic)
  def episode         : UInt64 := 16   -- u64
  def dex             : UInt64 := 24   -- Pubkey (32)
  def owner           : UInt64 := 56   -- Pubkey (32)
  def capital         : UInt64 := 88   -- u64 (shared cross-margin collateral!)
  def totalMm         : UInt64 := 96   -- u64 (sum of active legs' cachedMm)
  def totalIm         : UInt64 := 104  -- u64 (sum of active legs' cachedIm)
  def totalReservedMm : UInt64 := 112  -- u64 (sum of active legs' reservedMm; ALWAYS <= capital!)
end PflOff

namespace LegOff
  def active          : UInt64 := 0   -- u8 (0=empty, 1=active)
  def side            : UInt64 := 1   -- u8 (0=flat, 1=long, 2=short)
  def marketId        : UInt64 := 8   -- u64
  def market          : UInt64 := 16  -- Pubkey (32)
  def rawBasis        : UInt64 := 48  -- u64
  def entryK          : UInt64 := 56  -- u64
  def posPnl          : UInt64 := 64  -- u64 (unclaimed positive PnL on marketId's R_m)
  def cachedMm        : UInt64 := 72  -- u64 (latest MM requirement for this leg)
  def cachedIm        : UInt64 := 80  -- u64 (latest IM requirement for this leg)
  def lastRefreshSlot : UInt64 := 88  -- u64 (same-slot refresh witness!)
  def reservedMm      : UInt64 := 96  -- u64 (capital-backed MM reservation; prevents shielding!)
end LegOff

/-! ### Exact Byte Offsets: `InsReceipt` (88 bytes — Per-Market Isolated Insurance LP Shares) -/

namespace InsOff
  def size      : UInt64 := 88
  def magic     : UInt64 := 0   -- u32 ("INSR")
  def marketId  : UInt64 := 8   -- u64
  def owner     : UInt64 := 16  -- Pubkey (32)
  def market    : UInt64 := 48  -- Pubkey (32)
  def shares    : UInt64 := 80  -- u64
end InsOff

/-! ### Instruction Opcodes (20 minimal unified instructions) -/

namespace IxOp
  def initDex            : UInt8 := 0
  def updateCreateFee    : UInt8 := 1
  def createMarket       : UInt8 := 2
  def updateMarketAuth   : UInt8 := 3
  def updateOracle       : UInt8 := 4
  def initPortfolio      : UInt8 := 5
  def depositCollateral  : UInt8 := 6
  def withdrawCollateral : UInt8 := 7
  def depositInsurance   : UInt8 := 8
  def withdrawInsurance  : UInt8 := 9
  def refreshLeg         : UInt8 := 10
  def trade              : UInt8 := 11
  def liquidate          : UInt8 := 12
  def clearDeadLeg       : UInt8 := 13
  def startWindDown      : UInt8 := 14
  def forceClosePair     : UInt8 := 15
  def resolveMarket      : UInt8 := 16
  def claimResolved      : UInt8 := 17
  def tombstoneMarket    : UInt8 := 18
  def closeAccount       : UInt8 := 19
end IxOp

/-! ### Structured `sol_log_64_` Event Tags (0 heap, 0 `.rodata`, ~100 CU)
    Format: `sol_log_64_(tag, arg1, arg2, arg3, arg4)`
    - `1` (`oracle`)     : `(1, marketId, newPrice, fundSign, fundE6)`
    - `2` (`trade`)      : `(2, (marketId << 8) | mode, qty, execPrice, markPrice)`
    - `3` (`liquidate`)  : `(3, (marketId << 8) | mode, rawClose, liqFee, shortfall)`
    - `4` (`deposit`)    : `(4, portfolioId, amount, newCapital, 0)`
    - `5` (`withdraw`)   : `(5, portfolioId, amount, newCapital, 0)`
    - `6` (`forceClose`) : `(6, marketId, closedQty, remainingOi, slot)`
    - `7` (`resolve`)    : `(7, marketId, resolvedNum, resolvedDen, 0)` -/

namespace EvtTag
  def oracle     : UInt64 := 1
  def trade      : UInt64 := 2
  def liquidate  : UInt64 := 3
  def deposit    : UInt64 := 4
  def withdraw   : UInt64 := 5
  def forceClose : UInt64 := 6
  def resolve    : UInt64 := 7
end EvtTag

end Percolator
