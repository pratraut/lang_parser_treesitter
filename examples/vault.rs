//! Example Rust token vault for call-chain analysis.

use std::collections::HashMap;

pub struct Vault {
    balances:   HashMap<String, u64>,
    total:      u64,
    owner:      String,
}

impl Vault {
    pub fn new(owner: impl Into<String>, initial: u64) -> Self {
        let owner = owner.into();
        let mut v = Self {
            balances: HashMap::new(),
            total:    0,
            owner:    owner.clone(),
        };
        v.mint(owner, initial);
        v
    }

    pub fn deposit(&mut self, account: &str, amount: u64) {
        self.only_positive(amount);
        self.mint(account.to_string(), amount);
    }

    pub fn withdraw(&mut self, account: &str, amount: u64) {
        self.only_positive(amount);
        self.check_balance(account, amount);
        self.burn(account.to_string(), amount);
    }

    pub fn transfer(&mut self, from: &str, to: &str, amount: u64) {
        self.only_positive(amount);
        self.check_balance(from, amount);
        self.internal_transfer(from.to_string(), to.to_string(), amount);
    }

    pub fn balance_of(&self, account: &str) -> u64 {
        *self.balances.get(account).unwrap_or(&0)
    }

    pub fn total_supply(&self) -> u64 {
        self.total
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn mint(&mut self, account: String, amount: u64) {
        self.total += amount;
        *self.balances.entry(account).or_insert(0) += amount;
    }

    fn burn(&mut self, account: String, amount: u64) {
        self.total -= amount;
        *self.balances.entry(account).or_insert(0) -= amount;
    }

    fn internal_transfer(&mut self, from: String, to: String, amount: u64) {
        self.burn(from, amount);
        self.mint(to, amount);
    }

    fn only_positive(&self, amount: u64) {
        assert!(amount > 0, "amount must be positive");
    }

    fn check_balance(&self, account: &str, amount: u64) {
        assert!(
            self.balance_of(account) >= amount,
            "insufficient balance"
        );
    }
}

pub fn create_vault(owner: &str, supply: u64) -> Vault {
    Vault::new(owner, supply)
}

pub fn transfer_between(vault: &mut Vault, from: &str, to: &str, amount: u64) {
    vault.transfer(from, to, amount);
}

fn main() {}
