# Kani Proof Strength Audit — percolator-prog/tests/kani.rs

**Audit date**: 2026-03-21
**Proof count**: 111 (confirmed by `grep -c "^#\[kani::proof\]"`)
**Auditor**: Claude Sonnet 4.6 (automated audit)
**Methodology**: `scripts/audit-proof-strength.md`

Key changes since previous audit (2026-02-24):
- POS_SCALE = 1_000_000 (was 2^64); ADL_ONE = 1_000_000 (was 2^96)
- I256/U256 types removed — all i128/u128 now
- MatchingEngine trait moved to percolator-prog
- KANI_MAX_SCALE = 64, KANI_MAX_QUOTIENT = 16384 (unchanged)
- `gate_active` still exists; `insurance_floor` does not gate trades in practice
- One new proof added (111 vs 110 previously)

---

## Classification Summary

| Classification | Count | Description |
|---|---|---|
| STRONG | 84 | Symbolic inputs exercise key branches; appropriate property asserted; non-vacuous |
| WEAK | 15 | Symbolic collapse (SAT-bounded), missed branches, or weakened assertion |
| UNIT TEST | 5 | Concrete inputs or single execution path — intentional regression/documentation |
| CODE-EQUALS-SPEC | 6 | Proof asserts function equals its own body; regression value only |
| VACUOUS | 1 | Assertion on compile-time constants; cannot fail |

---

## WEAK Proofs by Category

### Category A: Branch Coverage Gaps

| Proof | Line | Issue | Recommendation |
|---|---|---|---|
| `kani_clamp_toward_no_movement_when_dt_zero` | 2915 | Assumes `index > 0` and `cap_e2bps > 0`, excluding the `index == 0` bootstrap branch (which returns `mark` regardless of `dt`). Correctly proves the Bug #9 fix for the non-bootstrap case, but the assumption combination means only the `dt == 0` early-return path is active; the symmetrical `cap == 0` return is not in scope. | Split into separate proofs for `dt = 0` and `cap = 0` as independent gate conditions, or add an `index == 0` witness that confirms the bootstrap branch is also exercised. |
| `kani_clamp_toward_no_movement_when_cap_zero` | 2936 | Mirror of above: `index > 0` and `dt > 0` excludes the bootstrap. `cap == 0` path correctly proved for `index > 0`. | Document or add an `index == 0 && cap == 0` concrete witness. |
| `kani_tradenocpi_auth_failure_rejects` | 732 | Proves only the rejection path (at least one auth fails). The gate check (`gate_active && risk_increase`) is never reached because auth failure causes early return. This proof is subsumed by `kani_tradenocpi_universal_characterization` (line 751); in isolation it lacks gate branch coverage. | Acceptable as documentation; note subsumption relationship in proof comment. |

### Category B: Weak Assertions

| Proof | Line | Issue | Recommendation |
|---|---|---|---|
| `kani_tradecpi_from_ret_gate_active_risk_neutral_accepts` | 2512 | All authorization bools are hardcoded concrete `true`, and `gate_active=true`, `risk_increase=false`. Tests one specific "gate active but risk neutral" scenario rather than the full symbolic space where auth bools vary. The accept-path assertion is valid but the proof does not verify that symbolic auth failures still cause rejection when combined with risk-neutral gate state. | Make `identity_ok`, `pda_ok`, `user_auth_ok`, `lp_auth_ok` symbolic, then add `kani::assume(all pass)` to prove the accept case, and a separate proof with `kani::assume(at least one fails)` for the reject case under gate-active conditions. |

### Category C: Symbolic Collapse (SAT-Bounded)

| Proof | Line | Issue | Recommendation |
|---|---|---|---|
| `kani_invert_nonzero_computes_correctly` | 1482 | `kani::assume(raw <= 8192)` caps far below the production u64 range. The result `floor(1e12/raw)` is proved correct only for small `raw`. The boundary case `raw == INVERSION_CONSTANT` (expected `Some(1)`) and values just above it (expected `None`) are not covered by this proof. | Add a targeted proof for `raw == INVERSION_CONSTANT as u64` and `raw == INVERSION_CONSTANT as u64 + 1`; extend domain to `raw <= u32::MAX` if SAT allows. |
| `kani_invert_monotonic` | 1544 | `kani::assume(raw1 <= KANI_MAX_QUOTIENT)` (16384). Monotonicity `raw1 > raw2 > 0 => inv1 <= inv2` is proved only for the bounded domain where both inversions succeed. The full-domain monotonicity property (including the transition to `None`) is not proved. | Document that the proof covers the "both-succeed" sub-domain; note the `None` interaction is covered by `kani_invert_result_zero_returns_none`. |
| `kani_base_to_units_conservation` | 1570 | `kani::assume(base <= scale * KANI_MAX_QUOTIENT)`. Conservation `units*scale + dust == base` is a direct consequence of Euclidean division and holds for all u64; the SAT bound is a necessary solver concession, not a conceptual gap. | Add a comment explaining why the bound is necessary and that the mathematical guarantee holds universally. |
| `kani_base_to_units_dust_bound` | 1591 | Same domain bound as conservation. `dust < scale` follows from `%` definition; SAT bound necessary. | Same comment recommendation. |
| `kani_units_roundtrip` | 1618 | `kani::assume(units <= KANI_MAX_QUOTIENT)` (16384) prevents overflow in `units * scale`. The non-overflow boundary is `units <= u64::MAX / scale`; the current bound is far more conservative and leaves a large unproved region. | Document saturation behavior; ideally add a proof with `units <= u32::MAX / scale` for tractability. |
| `kani_base_to_units_monotonic` | 1645 | Same SAT concession as conservation. | Same recommendation. |
| `kani_units_to_base_monotonic_bounded` | 1668 | `kani::assume(units <= KANI_MAX_QUOTIENT)`. Correctly excludes saturation range (documented in comment). The saturation regime (where `units_to_base` returns `u64::MAX` for both inputs) is unproved. | Add a proof that when both `units1 * scale` and `units2 * scale` overflow, both `units_to_base` calls return `u64::MAX`. |
| `kani_sweep_dust_conservation` | 1764 | Same SAT concession as `base_to_units` proofs. | Same recommendation. |
| `kani_sweep_dust_rem_bound` | 1784 | Same SAT concession. | Same recommendation. |
| `kani_withdraw_misaligned_rejects` | 1713 | `kani::assume(q <= KANI_MAX_QUOTIENT)` limits `amount = q * scale + r` to ~1M. Overflow in `q * scale` is not guarded; the proof silently excludes amounts where the constructed value wraps. | Add `kani::assume(q.checked_mul(scale as u64).map_or(false, |v| v <= u64::MAX - r))` to make the no-overflow assumption explicit. |
| `kani_withdraw_aligned_accepts` | 1733 | `kani::assume(units <= KANI_MAX_QUOTIENT)`. Same SAT concession. | Same note. |
| `kani_scale_price_e6_valid_result` | 2759 | `KANI_MAX_SCALE = 64` and `price <= KANI_MAX_QUOTIENT * unit_scale` caps price at ~1M. Production prices can be billions. The correctness assertion (`result == price / unit_scale`) is trivially equivalent to the function body, making this effectively a CODE-EQUALS-SPEC proof in a narrow domain. | Widen to `price <= u32::MAX` if SAT allows; or explicitly reclassify as CODE-EQUALS-SPEC/bounded regression. |
| `kani_scale_price_and_base_to_units_use_same_divisor` | 2817 | Uses `u8` scale (2..16), `u16` multipliers, `u8` position. Scale up to 1_000_000_000 is the real production range. The structural equivalence (both divide by `unit_scale`) is proved only for scale ≤ 16. | Document as bounded verification; note the property is structurally obvious from the function bodies. |
| `kani_scale_price_e6_concrete_example` | 2863 | Same narrow domain (scale 2..16, u8/u16 inputs). The "conservative margin behavior" proof is sound within domain but essentially a bounded integration test. | Reclassify as WEAK Category C. Note that integer truncation can cause up to 1-unit floor differences at exact scale boundaries, as commented. |
| `kani_clamp_toward_movement_bounded_concrete` | 2974 | `index_raw: u8` (10..255), `cap_steps: u8` (1..20), `dt_raw: u8` (1..16), `mark: u64` (unrestricted). The `any_clamp_formula_inputs()` helper used by formula branch proofs uses u16 index (100..1000). Coverage of large index values relies on `kani_clamp_toward_saturation_paths`. | Acceptable but document the coverage split: bounded concrete proof covers small inputs; `kani_clamp_toward_saturation_paths` covers large-value saturation paths. |

### Category D: Trivially True

| Proof | Line | Issue | Recommendation |
|---|---|---|---|
| `kani_invert_overflow_branch_is_dead` | 1527 | The first assertion `kani::assert(INVERSION_CONSTANT <= u64::MAX as u128, ...)` compares two compile-time constants (1e12 vs ~1.8e19) and trivially passes. The symbolic portion correctly proves unreachability of the overflow branch, but the dead branch itself is never executed by any input, so a proof of its unreachability adds no protection against regressions in the logic (as opposed to regressions in the constant). This is VACUOUS in that it cannot fail due to the constant comparison. | Replace the `kani::assert` on constants with a Rust `const` assertion or static check. Retain the symbolic portion (`inverted <= u64::MAX as u128`) as a standalone, properly named "overflow-branch-unreachable" proof. |

---

## UNIT TEST Proofs

| Proof | Line | Reason |
|---|---|---|
| `kani_min_abs_boundary_rejected` (X) | 1334 | All inputs are concrete literals: `exec_size = i128::MIN`, `req_size = i128::MIN + 1`. Single-path regression test proving the old `.abs()` panic is fixed. Valuable but not a symbolic proof. |
| `kani_tradecpi_from_ret_forced_acceptance` (AJ) | 2566 | All authorization bools concrete `true`, gate concrete `false`, `exec_size = 0` with `PARTIAL_OK`. Proves one specific happy-path execution; non-vacuity witness only. |
| `kani_tradecpi_from_ret_accept_uses_exec_size` (V companion) | 1243 | Shape hardcoded valid; all auth bools concrete `true`; gate concrete `false`. Symbolic only on `exec_size` and `req_size` dimensions. Functions as unit test of `cpi_trade_size` on the accept path. |
| `kani_tradecpi_from_ret_req_id_is_nonce_plus_one` (AF) | 2283 | Shape hardcoded via `valid_shape()`; all auth bools concrete `true`; gate concrete `false`. Only `old_nonce`, `lp_account_id`, `oracle_price_e6`, `req_size` are symbolic. Borderline unit test of nonce binding on a single forced-accept path. |
| `kani_clamp_toward_formula_concrete` (partial) | 3038 | Symbolic portion uses `any_clamp_formula_inputs()` with tight bounds (index 100..1000, cap 1%..5%, dt 1..20, mark ≤ 2000) plus `kani::assume(mark < lo)`. Non-vacuity witness is concrete. Effectively bounded integration test of one formula branch. |

---

## CODE-EQUALS-SPEC Proofs

These proofs assert that a function's output equals the literal expression of its body. They protect against regression if the body is edited but do not verify the body is correct against an independent specification.

| Proof | Line | Function | Notes |
|---|---|---|---|
| `kani_matcher_shape_universal` (E) | 386 | `matcher_shape_ok` | Body IS `prog_exec && !ctx_exec && ctx_owned && ctx_len`. Fully symbolic; regression value only. |
| `kani_lp_pda_shape_universal` (R) | 954 | `lp_pda_shape_ok` | Body IS `is_system_owned && data_len_zero && lamports_zero`. Fully symbolic. |
| `kani_oracle_feed_id_universal` (S) | 975 | `oracle_feed_id_ok` | Body IS `expected == provided`. Fully symbolic. |
| `kani_slab_shape_universal` (S) | 987 | `slab_shape_ok` | Body IS `owned_by_program && correct_len`. Fully symbolic. |
| `kani_len_ok_universal` (Q) | 933 | `len_ok` | Body IS `actual >= need`. Fully symbolic. |
| `kani_accumulate_dust_saturates` (AD) | 1828 | `accumulate_dust` | Body IS `old.saturating_add(added)`. Fully symbolic; guards against regression if implementation is replaced. |

---

## STRONG Proofs (84)

### Tier 1: Universal Characterization (12 proofs — highest verification value)

These proofs fully characterize the function-under-test for all symbolic inputs, proving both acceptance and rejection conditions as well as output field values.

**`kani_decide_trade_cpi_universal` (L, line 600)** — The flagship proof. Fully symbolic across all 9 gate inputs. Proves: Accept iff `matcher_shape_ok && identity_ok && pda_ok && abi_ok && user_auth_ok && lp_auth_ok && !(gate_active && risk_increase)`. On Accept, verifies `new_nonce == nonce_on_success(old_nonce)` and `chosen_size == exec_size`. Subsumes all 6 AE individual-gate proofs.

**`kani_tradenocpi_universal_characterization` (M, line 751)** — Full characterization of `decide_trade_nocpi` across all 4 symbolic boolean inputs.

**`kani_decide_single_owner_universal` (T, line 1006)** — Single boolean, fully characterized (both arms).

**`kani_decide_crank_universal` (T, line 1019)** — Three branches (permissionless / self-crank-ok / self-crank-fail) all symbolically covered by symbolic `permissionless`, `idx_exists`, `stored`, `signer`.

**`kani_decide_admin_universal` (T, line 1038)** — Full characterization including burned admin (`admin == [0;32]`) case. Both arms proved for all symbolic inputs.

**`kani_decide_keeper_crank_with_panic_universal` (Y, line 1443)** — Full characterization of 3-gate function: symbolic `allow_panic` (all u8 values), `admin`, `signer`, and crank inputs. The equivalence with the spec `if allow_panic != 0 && !admin_ok ... else decide_crank(...)` is proved without restricting any dimension.

**`kani_abi_ok_equals_validate` (U, line 1060)** — Proves `verify::abi_ok == validate_matcher_return.is_ok()` for all symbolic inputs. Critical coupling proof: ensures the wrapper function and real validator are mechanically identical.

**`kani_tradecpi_variants_consistent_valid_shape` (AF, line 2139)** — Proves `decide_trade_cpi` and `decide_trade_cpi_from_ret` produce identical outcomes when ABI validity is computed via `abi_ok(ret, ...)` consistently. All auth bools and gate bools are symbolic.

**`kani_withdraw_insurance_vault_result_characterization` (line 3214)** — Full characterization: `Some(vault - ins)` iff `ins <= vault`; `None` iff `ins > vault`. Both arms proved for all symbolic u128 inputs.

**`kani_base_to_units_scale_zero` (line 1607)** — Fully symbolic `base: u64`; proves `(base, 0)` for all inputs.

**`kani_units_to_base_scale_zero` (line 1635)** — Fully symbolic `units: u64`; proves identity return.

**`kani_scale_price_e6_identity_for_scale_leq_1` (line 2788)** — Fully symbolic `price: u64` and `unit_scale: u32` with `unit_scale <= 1`. Proves `Some(price)` for all inputs in this range.

### Tier 2: Critical Security Properties — ABI Rejection (8 proofs)

Each proof forces one specific validation gate to fail while leaving all remaining inputs fully symbolic. Together they cover all 8 rejection gates of `validate_matcher_return`.

- `kani_matcher_rejects_wrong_abi_version` (134): `ret.abi_version != MATCHER_ABI_VERSION`, all other inputs symbolic.
- `kani_matcher_rejects_missing_valid_flag` (149): `(flags & FLAG_VALID) == 0`, all other inputs symbolic.
- `kani_matcher_rejects_rejected_flag` (165): `flags |= FLAG_REJECTED`, all other inputs symbolic.
- `kani_matcher_rejects_nonzero_reserved` (182): `reserved != 0`, all other inputs symbolic.
- `kani_matcher_rejects_zero_exec_price` (200): `exec_price_e6 = 0`, all other inputs symbolic.
- `kani_matcher_zero_size_requires_partial_ok` (218): `exec_size = 0`, no `PARTIAL_OK` flag.
- `kani_matcher_rejects_exec_size_exceeds_req` (240): `|exec_size| > |req_size|`, all other inputs symbolic.
- `kani_matcher_rejects_sign_mismatch` (264): `exec_size.signum() != req_size.signum()`, all other inputs symbolic.

### Tier 3: Authorization and Binding (14 proofs)

Owner (`kani_owner_mismatch_rejected` 291, `kani_owner_match_accepted` 301), admin (`kani_admin_mismatch_rejected` 313, `kani_admin_match_accepted` 324, `kani_admin_burned_disables_ops` 333), CPI identity (`kani_matcher_identity_mismatch_rejected` 349, `kani_matcher_identity_match_accepted` 366), PDA (`kani_pda_mismatch_rejected` 411, `kani_pda_match_accepted` 424), nonce (`kani_nonce_unchanged_on_failure` 436, `kani_nonce_advances_on_success` 445), CPI exec_size binding (`kani_cpi_uses_exec_size` 463), per-instruction auth (`kani_single_owner_mismatch_rejected` 524, `kani_single_owner_match_accepted` 537, `kani_trade_rejects_user_mismatch` 548, `kani_trade_rejects_lp_mismatch` 562).

All are fully symbolic and non-vacuous; they cover the rejection AND acceptance paths of each elementary check.

### Tier 4: Gate Activation Logic (3 proofs)

`kani_gate_inactive_when_threshold_zero` (482), `kani_gate_inactive_when_balance_exceeds` (493), `kani_gate_active_when_conditions_met` (506): cover all 3 branches of `gate_active(threshold, balance)` for fully symbolic inputs.

### Tier 5: Nonce Transition and Decision Coupling (7 proofs)

`kani_tradecpi_reject_nonce_unchanged` (645), `kani_tradecpi_accept_increments_nonce` (678): restrict shape to invalid/valid respectively with symbolic remainder; prove unconditional rejection/acceptance and nonce behavior.

`kani_tradecpi_any_reject_nonce_unchanged` (808), `kani_tradecpi_any_accept_increments_nonce` (871): unconstrained symbolic inputs; prove `decision_nonce` correctly maps both outcomes. Include concrete non-vacuity witnesses.

`kani_tradecpi_from_ret_any_reject_nonce_unchanged` (1098), `kani_tradecpi_from_ret_any_accept_increments_nonce` (1172): same structure for `decide_trade_cpi_from_ret` variant.

`kani_tradecpi_variants_consistent_invalid_shape` (2213): proves both variants reject on invalid shape.

### Tier 6: Gate Kill-Switch and Panic Guard (3 proofs)

`kani_universal_gate_risk_increase_rejects` (2341): proves `gate_active && risk_increase` causes rejection in `decide_trade_cpi` regardless of all other inputs (symbolic shape forced valid, other bools fully symbolic).

`kani_universal_gate_risk_increase_rejects_from_ret` (2455): same for `decide_trade_cpi_from_ret` with ABI-valid `ret` constructed symbolically to get past the ABI check.

`kani_universal_panic_requires_admin` (2416): proves `allow_panic != 0 && !admin_ok` rejects unconditionally for all other symbolic inputs.

### Tier 7: Gate Ordering Documentation (6 proofs)

AE section — `kani_universal_shape_fail_rejects` (1892), `kani_universal_pda_fail_rejects` (1934), `kani_universal_user_auth_fail_rejects` (1974), `kani_universal_lp_auth_fail_rejects` (2014), `kani_universal_identity_fail_rejects` (2054), `kani_universal_abi_fail_rejects` (2094): each proves one gate causes rejection when the gate fails, with all inputs before that gate forced to pass and later inputs fully symbolic. Subsumed by `kani_decide_trade_cpi_universal` but useful as gate-ordering documentation.

### Tier 8: Math Properties (31 proofs — selective listing of strongest)

**Oracle inversion**: `kani_invert_zero_returns_raw` (1472) — fully symbolic u64, proves pass-through; `kani_invert_zero_raw_returns_none` (1502) — symbolic nonzero `invert`, symbolic zero `raw`, proves `None`; `kani_invert_result_zero_returns_none` (1512) — proves all `raw > INVERSION_CONSTANT` return `None`.

**Scale=0 policies**: `kani_scale_zero_policy_no_dust` (1844), `kani_scale_zero_policy_sweep_complete` (1855), `kani_scale_zero_policy_end_to_end` (1866) — unbounded symbolic inputs; the end-to-end proof composes three functions with a symbolic `old_dust` variable.

**InitMarket scale bounds**: `kani_init_market_scale_rejects_overflow` (2621), `kani_init_market_scale_valid_range` (2636) — fully symbolic u32; test both sides of `MAX_UNIT_SCALE`.

**WithdrawInsurance**: `kani_withdraw_insurance_vault_correct` (3179), `kani_withdraw_insurance_vault_overflow` (3197) — fully symbolic u128; cover both branches.

**Clamp toward (Bug #9)**: `kani_clamp_toward_bootstrap_when_index_zero` (2956) — symbolic `mark`, `cap`, `dt`; `kani_clamp_toward_formula_within_bounds` (3064), `kani_clamp_toward_formula_above_hi` (3092) — bounded symbolic with non-vacuity witnesses; `kani_clamp_toward_saturation_paths` (3120) — large-value saturation with symbolic `index_offset`, `mark`, `cap_steps`, `dt`; `inductive_clamp_within_bounds` (3262) — unbounded symbolic proof that `mark.clamp(lo, hi)` stays in `[lo, hi]`.

**Acceptance proofs**: `kani_matcher_zero_size_with_partial_ok_accepted` (774), `kani_matcher_accepts_minimal_valid_nonzero_exec` (1370), `kani_matcher_accepts_exec_size_equal_req_size` (1395), `kani_matcher_accepts_partial_fill_with_flag` (1415) — prove the ACCEPTANCE path of `validate_matcher_return` for symbolic inputs.

**Unit conversion (scale=0 edge cases)**: `kani_base_to_units_monotonic_scale_zero` (1692), `kani_withdraw_scale_zero_always_aligned` (1750), `kani_sweep_dust_below_threshold` (1800), `kani_sweep_dust_scale_zero` (1815), `kani_units_roundtrip_exact_when_no_dust` (2397) — all unbounded symbolic; test the fast paths that don't involve division.

---

## Cross-Cutting Observations

### 1. Universal Characterization Coverage is Excellent
The suite contains 12 Tier 1 universal characterization proofs. Every security-critical decision function — `decide_trade_cpi`, `decide_trade_nocpi`, `decide_keeper_crank_with_panic`, `decide_admin_op`, `decide_crank`, `decide_single_owner_op` — is fully characterized. The `kani_abi_ok_equals_validate` proof is the strongest coupling proof in the file: it mechanically ties `verify::abi_ok` to the real `validate_matcher_return` without any abstraction gap.

### 2. SAT-Bounded Division Proofs are a Necessary Gap
15 proofs use `KANI_MAX_SCALE = 64` and/or `KANI_MAX_QUOTIENT = 16384` to bound division-related SAT queries. Production prices can be in the billions (e.g., 100_000_000_000 for BTC in e6 format). The bounded domain (max ~1M) does not cover the production range. This is the most significant structural gap in the suite.

The mathematical properties being proved (Euclidean division conservation, monotonicity) are provably correct for all inputs because they are definitional properties of integer division. The SAT bounds are necessary to keep proof runtimes tractable, not because the properties break down outside the bounds. However, this relies on the reader trusting the mathematical argument rather than the Kani proof.

**Recommendation**: For each bounded division proof, add a code comment explaining: "this property follows from the definition of floor-division and holds for all u64 inputs; the assume bound is required for SAT tractability only."

### 3. `kani_invert_overflow_branch_is_dead` is VACUOUS
The proof's first statement asserts `INVERSION_CONSTANT <= u64::MAX as u128` — a comparison between two compile-time constants (1,000,000,000,000 and 18,446,744,073,709,551,615). This cannot fail under any model. The subsequent symbolic portion correctly proves the overflow branch is unreachable, but since the branch itself is dead code, the proof adds no protection against logic regressions. Replace the `kani::assert` on constants with a Rust `const` assertion at module level.

### 4. `decide_trade_cpi_from_ret` Lacks a Tier 1 Universal Characterization
`decide_trade_cpi` has `kani_decide_trade_cpi_universal` which fully characterizes it for all symbolic inputs. `decide_trade_cpi_from_ret` has strong nonce-transition proofs (V section) and the consistency proof (AF section), but no single proof that characterizes the full accept/reject condition across all symbolic inputs.

**Recommendation**: Add a `kani_decide_trade_cpi_from_ret_universal` proof analogous to `kani_decide_trade_cpi_universal`. The specification is: Accept iff `matcher_shape_ok(shape) && identity_ok && pda_ok && user_auth_ok && lp_auth_ok && abi_ok(ret, lp_id, oracle, req_size, nonce_on_success(old_nonce)) && !(gate_is_active && risk_increase)`.

### 5. Non-Vacuity Discipline is Consistently Applied
All proofs that assert properties on the Accept path include concrete non-vacuity witnesses: blocks that call the function with known-accepting inputs and assert the outcome. This practice is applied consistently across sections L, P, V, AF, AI, AJ, and all formula branch proofs. This is a best practice that should be maintained for all future proofs.

### 6. Missing Coverage: `compute_premium_funding_bps_per_slot`
The `oracle::compute_premium_funding_bps_per_slot` function has no Kani proof. It contains 6 branches: zero-input early return, premium computation, premium clamping, k-multiplier scaling, per-slot division, and policy clamping. It is a pure arithmetic function with multiple saturation and clamping paths that would benefit from symbolic proof.

**Recommendation**: Add proofs for: (a) any zero input returns 0, (b) output magnitude is bounded by `max_bps_per_slot`, (c) sign matches `mark - index` direction, (d) premium clamp is applied before k-multiplier.

### 7. Missing Coverage: `clamp_oracle_price`
`oracle::clamp_oracle_price` (the circuit breaker) has 3 branches: disabled (`max_change_e2bps == 0`), first-time (`last_price == 0`), and normal clamping. No Kani proof exists for this function. Given its role as a price manipulation circuit breaker, the absence is notable.

**Recommendation**: Add a universal characterization proof with symbolic `last_price`, `raw_price`, `max_change_e2bps`.

### 8. `kani_scale_price_e6_concrete_example` is Misclassified in the Codebase
The proof name says "concrete example" but uses bounded symbolic inputs. It is better described as "bounded symbolic proof of margin conservatism." The narrow domain (scale 2..16, u8/u16 multipliers) makes it resemble a bounded integration test. Rename or reclassify to accurately reflect its scope.

### 9. `kani_tradecpi_from_ret_gate_active_risk_neutral_accepts` Has Unexplored Auth Interaction
By hardcoding all auth bools to `true`, this proof cannot demonstrate whether the gate-active + risk-neutral case interacts correctly with partial auth failure. In production, gate-active + risk-neutral + identity failure should still reject. The companion `kani_universal_gate_risk_increase_rejects_from_ret` correctly forces ABI-valid inputs to reach the gate — a similar technique should be applied here but with symbolic auth bools.

### 10. Summary of Top Priorities for Strengthening
1. Add `kani_decide_trade_cpi_from_ret_universal` (Tier 1 gap).
2. Add `clamp_oracle_price` universal characterization.
3. Add `compute_premium_funding_bps_per_slot` property proofs.
4. Replace `kani_invert_overflow_branch_is_dead` `kani::assert` on constants with a `const` assertion.
5. Add overflow guard to `kani_withdraw_misaligned_rejects`.
6. Document SAT bounds in all Category C proofs with a standard comment explaining mathematical universality.

---

## Per-Proof Classification Table

| # | Proof Name | Line | Section | Classification |
|---|---|---|---|---|
| 1 | `kani_matcher_rejects_wrong_abi_version` | 134 | A | STRONG |
| 2 | `kani_matcher_rejects_missing_valid_flag` | 149 | A | STRONG |
| 3 | `kani_matcher_rejects_rejected_flag` | 165 | A | STRONG |
| 4 | `kani_matcher_rejects_nonzero_reserved` | 182 | A | STRONG |
| 5 | `kani_matcher_rejects_zero_exec_price` | 200 | A | STRONG |
| 6 | `kani_matcher_zero_size_requires_partial_ok` | 218 | A | STRONG |
| 7 | `kani_matcher_rejects_exec_size_exceeds_req` | 240 | A | STRONG |
| 8 | `kani_matcher_rejects_sign_mismatch` | 264 | A | STRONG |
| 9 | `kani_owner_mismatch_rejected` | 291 | B | STRONG |
| 10 | `kani_owner_match_accepted` | 301 | B | STRONG |
| 11 | `kani_admin_mismatch_rejected` | 313 | C | STRONG |
| 12 | `kani_admin_match_accepted` | 324 | C | STRONG |
| 13 | `kani_admin_burned_disables_ops` | 333 | C | STRONG |
| 14 | `kani_matcher_identity_mismatch_rejected` | 349 | D | STRONG |
| 15 | `kani_matcher_identity_match_accepted` | 366 | D | STRONG |
| 16 | `kani_matcher_shape_universal` | 386 | E | CODE-EQUALS-SPEC |
| 17 | `kani_pda_mismatch_rejected` | 411 | F | STRONG |
| 18 | `kani_pda_match_accepted` | 424 | F | STRONG |
| 19 | `kani_nonce_unchanged_on_failure` | 436 | G | STRONG |
| 20 | `kani_nonce_advances_on_success` | 445 | G | STRONG |
| 21 | `kani_cpi_uses_exec_size` | 463 | H | STRONG |
| 22 | `kani_gate_inactive_when_threshold_zero` | 482 | I | STRONG |
| 23 | `kani_gate_inactive_when_balance_exceeds` | 493 | I | STRONG |
| 24 | `kani_gate_active_when_conditions_met` | 506 | I | STRONG |
| 25 | `kani_single_owner_mismatch_rejected` | 524 | J | STRONG |
| 26 | `kani_single_owner_match_accepted` | 537 | J | STRONG |
| 27 | `kani_trade_rejects_user_mismatch` | 548 | J | STRONG |
| 28 | `kani_trade_rejects_lp_mismatch` | 562 | J | STRONG |
| 29 | `kani_decide_trade_cpi_universal` | 600 | L | STRONG (Tier 1) |
| 30 | `kani_tradecpi_reject_nonce_unchanged` | 645 | L | STRONG |
| 31 | `kani_tradecpi_accept_increments_nonce` | 678 | L | STRONG |
| 32 | `kani_tradenocpi_auth_failure_rejects` | 732 | M | WEAK (Cat A) |
| 33 | `kani_tradenocpi_universal_characterization` | 751 | M | STRONG (Tier 1) |
| 34 | `kani_matcher_zero_size_with_partial_ok_accepted` | 774 | N | STRONG |
| 35 | `kani_tradecpi_any_reject_nonce_unchanged` | 808 | P | STRONG |
| 36 | `kani_tradecpi_any_accept_increments_nonce` | 871 | P | STRONG |
| 37 | `kani_len_ok_universal` | 933 | Q | CODE-EQUALS-SPEC |
| 38 | `kani_lp_pda_shape_universal` | 954 | R | CODE-EQUALS-SPEC |
| 39 | `kani_oracle_feed_id_universal` | 975 | S | CODE-EQUALS-SPEC |
| 40 | `kani_slab_shape_universal` | 987 | S | CODE-EQUALS-SPEC |
| 41 | `kani_decide_single_owner_universal` | 1006 | T | STRONG (Tier 1) |
| 42 | `kani_decide_crank_universal` | 1019 | T | STRONG (Tier 1) |
| 43 | `kani_decide_admin_universal` | 1038 | T | STRONG (Tier 1) |
| 44 | `kani_abi_ok_equals_validate` | 1060 | U | STRONG (Tier 1) |
| 45 | `kani_tradecpi_from_ret_any_reject_nonce_unchanged` | 1098 | V | STRONG |
| 46 | `kani_tradecpi_from_ret_any_accept_increments_nonce` | 1172 | V | STRONG |
| 47 | `kani_tradecpi_from_ret_accept_uses_exec_size` | 1243 | V | UNIT TEST |
| 48 | `kani_min_abs_boundary_rejected` | 1334 | X | UNIT TEST |
| 49 | `kani_matcher_accepts_minimal_valid_nonzero_exec` | 1370 | Y | STRONG |
| 50 | `kani_matcher_accepts_exec_size_equal_req_size` | 1395 | Y | STRONG |
| 51 | `kani_matcher_accepts_partial_fill_with_flag` | 1415 | Y | STRONG |
| 52 | `kani_decide_keeper_crank_with_panic_universal` | 1443 | Y | STRONG (Tier 1) |
| 53 | `kani_invert_zero_returns_raw` | 1472 | AA | STRONG |
| 54 | `kani_invert_nonzero_computes_correctly` | 1482 | AA | WEAK (Cat C) |
| 55 | `kani_invert_zero_raw_returns_none` | 1502 | AA | STRONG |
| 56 | `kani_invert_result_zero_returns_none` | 1512 | AA | STRONG |
| 57 | `kani_invert_overflow_branch_is_dead` | 1527 | AA | VACUOUS |
| 58 | `kani_invert_monotonic` | 1544 | AA | WEAK (Cat C) |
| 59 | `kani_base_to_units_conservation` | 1570 | AB | WEAK (Cat C) |
| 60 | `kani_base_to_units_dust_bound` | 1591 | AB | WEAK (Cat C) |
| 61 | `kani_base_to_units_scale_zero` | 1607 | AB | STRONG (Tier 1) |
| 62 | `kani_units_roundtrip` | 1618 | AB | WEAK (Cat C) |
| 63 | `kani_units_to_base_scale_zero` | 1635 | AB | STRONG (Tier 1) |
| 64 | `kani_base_to_units_monotonic` | 1645 | AB | WEAK (Cat C) |
| 65 | `kani_units_to_base_monotonic_bounded` | 1668 | AB | WEAK (Cat C) |
| 66 | `kani_base_to_units_monotonic_scale_zero` | 1692 | AB | STRONG |
| 67 | `kani_withdraw_misaligned_rejects` | 1713 | AC | WEAK (Cat C) |
| 68 | `kani_withdraw_aligned_accepts` | 1733 | AC | WEAK (Cat C) |
| 69 | `kani_withdraw_scale_zero_always_aligned` | 1750 | AC | STRONG |
| 70 | `kani_sweep_dust_conservation` | 1764 | AD | WEAK (Cat C) |
| 71 | `kani_sweep_dust_rem_bound` | 1784 | AD | WEAK (Cat C) |
| 72 | `kani_sweep_dust_below_threshold` | 1800 | AD | STRONG |
| 73 | `kani_sweep_dust_scale_zero` | 1815 | AD | STRONG |
| 74 | `kani_accumulate_dust_saturates` | 1828 | AD | CODE-EQUALS-SPEC |
| 75 | `kani_scale_zero_policy_no_dust` | 1844 | AD | STRONG |
| 76 | `kani_scale_zero_policy_sweep_complete` | 1855 | AD | STRONG |
| 77 | `kani_scale_zero_policy_end_to_end` | 1866 | AD | STRONG |
| 78 | `kani_universal_shape_fail_rejects` | 1892 | AE | STRONG |
| 79 | `kani_universal_pda_fail_rejects` | 1934 | AE | STRONG |
| 80 | `kani_universal_user_auth_fail_rejects` | 1974 | AE | STRONG |
| 81 | `kani_universal_lp_auth_fail_rejects` | 2014 | AE | STRONG |
| 82 | `kani_universal_identity_fail_rejects` | 2054 | AE | STRONG |
| 83 | `kani_universal_abi_fail_rejects` | 2094 | AE | STRONG |
| 84 | `kani_tradecpi_variants_consistent_valid_shape` | 2139 | AF | STRONG (Tier 1) |
| 85 | `kani_tradecpi_variants_consistent_invalid_shape` | 2213 | AF | STRONG |
| 86 | `kani_tradecpi_from_ret_req_id_is_nonce_plus_one` | 2283 | AF | UNIT TEST |
| 87 | `kani_universal_gate_risk_increase_rejects` | 2341 | AG | STRONG |
| 88 | `kani_units_roundtrip_exact_when_no_dust` | 2397 | AH | STRONG |
| 89 | `kani_universal_panic_requires_admin` | 2416 | AH | STRONG |
| 90 | `kani_universal_gate_risk_increase_rejects_from_ret` | 2455 | AI | STRONG |
| 91 | `kani_tradecpi_from_ret_gate_active_risk_neutral_accepts` | 2512 | AI | WEAK (Cat B) |
| 92 | `kani_tradecpi_from_ret_forced_acceptance` | 2566 | AJ | UNIT TEST |
| 93 | `kani_init_market_scale_rejects_overflow` | 2621 | AK | STRONG |
| 94 | `kani_init_market_scale_valid_range` | 2636 | AK | STRONG |
| 95 | `kani_scale_price_e6_zero_result_rejected` | 2739 | Bug | STRONG |
| 96 | `kani_scale_price_e6_valid_result` | 2759 | Bug | WEAK (Cat C) |
| 97 | `kani_scale_price_e6_identity_for_scale_leq_1` | 2788 | Bug | STRONG (Tier 1) |
| 98 | `kani_scale_price_and_base_to_units_use_same_divisor` | 2817 | Bug | WEAK (Cat C) |
| 99 | `kani_scale_price_e6_concrete_example` | 2863 | Bug | WEAK (Cat C) |
| 100 | `kani_clamp_toward_no_movement_when_dt_zero` | 2915 | Bug #9 | WEAK (Cat A) |
| 101 | `kani_clamp_toward_no_movement_when_cap_zero` | 2936 | Bug #9 | WEAK (Cat A) |
| 102 | `kani_clamp_toward_bootstrap_when_index_zero` | 2956 | Bug #9 | STRONG |
| 103 | `kani_clamp_toward_movement_bounded_concrete` | 2974 | Bug #9 | WEAK (Cat C) |
| 104 | `kani_clamp_toward_formula_concrete` | 3038 | Bug #9 | UNIT TEST |
| 105 | `kani_clamp_toward_formula_within_bounds` | 3064 | Bug #9 | STRONG |
| 106 | `kani_clamp_toward_formula_above_hi` | 3092 | Bug #9 | STRONG |
| 107 | `kani_clamp_toward_saturation_paths` | 3120 | Bug #9 | STRONG |
| 108 | `kani_withdraw_insurance_vault_correct` | 3179 | WI | STRONG |
| 109 | `kani_withdraw_insurance_vault_overflow` | 3197 | WI | STRONG |
| 110 | `kani_withdraw_insurance_vault_result_characterization` | 3214 | WI | STRONG (Tier 1) |
| 111 | `inductive_clamp_within_bounds` | 3262 | Inductive | STRONG |
