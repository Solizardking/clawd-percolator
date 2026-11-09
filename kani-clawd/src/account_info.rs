//! Symbolic `solana_program::AccountInfo` (feature `solana-account-info`).
//!
//! [`SymbolicAccount`] owns the backing storage (key, owner, lamports, data)
//! for one account, generated from [`AccountConstraints`]. Call
//! [`SymbolicAccount::info`] to borrow a real `AccountInfo<'_>` and pass it to
//! unmodified program code. [`LamportSnapshot`] records total lamports so a
//! harness can assert conservation across an instruction.

use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

/// Constraints for generating a symbolic account. `None` = unconstrained.
#[derive(Clone, Copy, Debug)]
pub struct AccountConstraints {
    pub key: Option<Pubkey>,
    pub owner: Option<Pubkey>,
    pub is_signer: Option<bool>,
    pub is_writable: Option<bool>,
    pub executable: Option<bool>,
    pub min_lamports: u64,
    pub max_lamports: u64,
}

impl Default for AccountConstraints {
    fn default() -> Self {
        Self {
            key: None,
            owner: None,
            is_signer: None,
            is_writable: None,
            executable: Some(false),
            min_lamports: 0,
            max_lamports: u64::MAX,
        }
    }
}

impl AccountConstraints {
    pub fn signer(mut self) -> Self {
        self.is_signer = Some(true);
        self
    }
    pub fn writable(mut self) -> Self {
        self.is_writable = Some(true);
        self
    }
    pub fn owned_by(mut self, owner: Pubkey) -> Self {
        self.owner = Some(owner);
        self
    }
    pub fn with_key(mut self, key: Pubkey) -> Self {
        self.key = Some(key);
        self
    }
    pub fn lamports_in(mut self, min: u64, max: u64) -> Self {
        self.min_lamports = min;
        self.max_lamports = max;
        self
    }
}

fn pick<T: kani::Arbitrary + Copy>(fixed: Option<T>) -> T {
    match fixed {
        Some(v) => v,
        None => kani::any(),
    }
}

fn any_pubkey(fixed: Option<Pubkey>) -> Pubkey {
    match fixed {
        Some(k) => k,
        None => Pubkey::new_from_array(kani::any()),
    }
}

/// Backing storage for one symbolic account with `D` bytes of data.
pub struct SymbolicAccount<const D: usize> {
    pub key: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data: [u8; D],
    pub is_signer: bool,
    pub is_writable: bool,
    pub executable: bool,
    pub rent_epoch: u64,
}

impl<const D: usize> SymbolicAccount<D> {
    /// Generate an account satisfying `c`; data bytes are fully symbolic.
    pub fn new(c: AccountConstraints) -> Self {
        let lamports: u64 = kani::any();
        kani::assume(lamports >= c.min_lamports && lamports <= c.max_lamports);
        Self {
            key: any_pubkey(c.key),
            owner: any_pubkey(c.owner),
            lamports,
            data: kani::any(),
            is_signer: pick(c.is_signer),
            is_writable: pick(c.is_writable),
            executable: pick(c.executable),
            rent_epoch: kani::any(),
        }
    }

    /// Borrow a real `AccountInfo` over this storage.
    pub fn info(&mut self) -> AccountInfo<'_> {
        AccountInfo::new(
            &self.key,
            self.is_signer,
            self.is_writable,
            &mut self.lamports,
            &mut self.data,
            &self.owner,
            self.executable,
            self.rent_epoch,
        )
    }
}

/// Total lamports across a set of accounts at one point in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LamportSnapshot {
    pub total: u128,
}

impl LamportSnapshot {
    pub fn capture(accounts: &[AccountInfo<'_>]) -> Self {
        let mut total = 0u128;
        let mut i = 0;
        while i < accounts.len() {
            total += accounts[i].lamports() as u128;
            i += 1;
        }
        Self { total }
    }

    /// Assert the accounts hold the same total as when captured.
    #[track_caller]
    pub fn assert_conserved(&self, accounts: &[AccountInfo<'_>]) {
        kani::assert(Self::capture(accounts).total == self.total, "lamports not conserved");
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    fn move_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<(), ()> {
        if !from.is_signer || !from.is_writable || !to.is_writable {
            return Err(());
        }
        let f = from.lamports().checked_sub(amount).ok_or(())?;
        let t = to.lamports().checked_add(amount).ok_or(())?;
        **from.try_borrow_mut_lamports().map_err(|_| ())? = f;
        **to.try_borrow_mut_lamports().map_err(|_| ())? = t;
        Ok(())
    }

    #[kani::proof]
    #[kani::unwind(3)]
    fn account_info_lamport_move_conserves() {
        let mut a = SymbolicAccount::<0>::new(AccountConstraints::default().writable());
        let mut b = SymbolicAccount::<0>::new(AccountConstraints::default().writable());
        let infos = [a.info(), b.info()];
        let snap = LamportSnapshot::capture(&infos);
        let r = move_lamports(&infos[0], &infos[1], kani::any());
        snap.assert_conserved(&infos);
        if r.is_ok() {
            kani::assert(infos[0].is_signer, "only signers move lamports");
        }
    }

    #[kani::proof]
    fn account_info_respects_constraints() {
        let owner = Pubkey::new_from_array([9u8; 32]);
        let mut a = SymbolicAccount::<4>::new(
            AccountConstraints::default().signer().owned_by(owner).lamports_in(10, 20),
        );
        let info = a.info();
        kani::assert(info.is_signer && *info.owner == owner, "fixed fields honored");
        kani::assert(info.lamports() >= 10 && info.lamports() <= 20, "lamport range honored");
        kani::assert(info.data_len() == 4, "data length");
    }
}
