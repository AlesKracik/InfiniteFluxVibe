// wallet.rs: Wallets (personal & corporate) and Corporations.
//
// A `Wallet` is a component holding a Credits balance. `debit` enforces
// the no-overdraft rule: a transaction that would drop the balance below
// zero is rejected, so the atomicity of "pay N credits" is just
// `if wallet.debit(N) { ... do the thing ... }`.
//
// Corporations are entities that own a Wallet and have members (players).
// Dividends and share ownership are modelled loosely for Phase 6 —
// they'll gain weight in later phases.

use crate::credits::Credits;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A container of credits. Attach to players, corporations, or ATMs.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    pub balance: Credits,
}

impl Wallet {
    pub fn new(initial: Credits) -> Self {
        Self { balance: initial }
    }

    /// Attempt to deduct `amount`. Returns `true` on success, `false` if
    /// the balance is insufficient (in which case the wallet is unchanged).
    ///
    /// A zero-amount debit always succeeds; a negative amount is treated
    /// as a credit (i.e. a refund) and always succeeds. Callers that want
    /// to reject negative debits must check themselves.
    pub fn debit(&mut self, amount: Credits) -> bool {
        if amount.is_negative() {
            // A negative debit is a credit — accept it.
            self.balance -= amount; // subtracting a negative = adding
            return true;
        }
        if self.balance < amount {
            return false;
        }
        self.balance -= amount;
        true
    }

    /// Always succeeds. Negative credits are accepted (and behave like
    /// debits without overdraft protection — use `debit` if that matters).
    pub fn credit(&mut self, amount: Credits) {
        self.balance += amount;
    }

    /// Can this wallet afford `amount` right now?
    pub fn can_afford(&self, amount: Credits) -> bool {
        self.balance >= amount
    }
}

/// A corporation: a named entity with a shared wallet and member list.
///
/// Shares and dividend logic are deliberately simple: `shares_outstanding`
/// plus a `members` list holding each member's share count. A dividend
/// payout divides the pot proportionally — rounding dust stays in the
/// corporate wallet. Later phases can replace this with a formal cap-table.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Corporation {
    pub name: String,
    pub wallet: Wallet,
    /// (member player id, shares held)
    pub members: Vec<(u64, u32)>,
    /// Total shares issued. Should equal sum of member shares in a healthy corp.
    pub shares_outstanding: u32,
}

impl Corporation {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            wallet: Wallet::default(),
            members: Vec::new(),
            shares_outstanding: 0,
        }
    }

    /// Issue `shares` to a player. Increases shares_outstanding.
    pub fn issue_shares(&mut self, player: u64, shares: u32) {
        if let Some((_, existing)) = self.members.iter_mut().find(|(p, _)| *p == player) {
            *existing = existing.saturating_add(shares);
        } else {
            self.members.push((player, shares));
        }
        self.shares_outstanding = self.shares_outstanding.saturating_add(shares);
    }

    /// Pay a dividend of `total` from the corp wallet, split pro-rata by shares.
    /// Returns per-member payouts (player_id, amount). Rounding dust remains in wallet.
    /// If there are no shares outstanding or the wallet can't cover the payout,
    /// returns an empty Vec and makes no changes.
    pub fn pay_dividend(&mut self, total: Credits) -> Vec<(u64, Credits)> {
        if self.shares_outstanding == 0 || !self.wallet.can_afford(total) || total <= Credits::ZERO
        {
            return Vec::new();
        }
        // Per-share dividend: integer division truncates — dust stays in wallet.
        let per_share = total / self.shares_outstanding as i64;
        if per_share == Credits::ZERO {
            return Vec::new();
        }
        let mut payouts = Vec::with_capacity(self.members.len());
        let mut paid_out = Credits::ZERO;
        for (pid, shares) in &self.members {
            let amount = per_share * (*shares as i64);
            if amount > Credits::ZERO {
                payouts.push((*pid, amount));
                paid_out += amount;
            }
        }
        // Debit only the actually-distributed total; dust stays in corp wallet.
        let _ = self.wallet.debit(paid_out);
        payouts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_wallet_is_zero() {
        let w = Wallet::default();
        assert_eq!(w.balance, Credits::ZERO);
    }

    #[test]
    fn credit_increases_balance() {
        let mut w = Wallet::default();
        w.credit(Credits::from_whole(100));
        assert_eq!(w.balance, Credits::from_whole(100));
    }

    #[test]
    fn debit_succeeds_with_funds() {
        let mut w = Wallet::new(Credits::from_whole(100));
        assert!(w.debit(Credits::from_whole(30)));
        assert_eq!(w.balance, Credits::from_whole(70));
    }

    #[test]
    fn debit_rejects_insufficient_funds() {
        let mut w = Wallet::new(Credits::from_whole(10));
        assert!(!w.debit(Credits::from_whole(100)));
        // Balance unchanged on failure
        assert_eq!(w.balance, Credits::from_whole(10));
    }

    #[test]
    fn debit_exact_balance_succeeds() {
        let mut w = Wallet::new(Credits::from_whole(100));
        assert!(w.debit(Credits::from_whole(100)));
        assert_eq!(w.balance, Credits::ZERO);
    }

    #[test]
    fn debit_zero_succeeds() {
        let mut w = Wallet::new(Credits::from_whole(10));
        assert!(w.debit(Credits::ZERO));
        assert_eq!(w.balance, Credits::from_whole(10));
    }

    #[test]
    fn can_afford() {
        let w = Wallet::new(Credits::from_whole(50));
        assert!(w.can_afford(Credits::from_whole(50)));
        assert!(w.can_afford(Credits::from_whole(10)));
        assert!(!w.can_afford(Credits::from_whole(51)));
    }

    #[test]
    fn corporation_issue_shares() {
        let mut c = Corporation::new("Acme");
        c.issue_shares(1, 100);
        c.issue_shares(2, 50);
        assert_eq!(c.shares_outstanding, 150);
        assert_eq!(c.members.len(), 2);
    }

    #[test]
    fn corporation_issue_shares_to_existing_member_adds() {
        let mut c = Corporation::new("Acme");
        c.issue_shares(1, 100);
        c.issue_shares(1, 50);
        assert_eq!(c.shares_outstanding, 150);
        assert_eq!(c.members.len(), 1);
        assert_eq!(c.members[0].1, 150);
    }

    #[test]
    fn corporation_pays_dividend_proportionally() {
        let mut c = Corporation::new("Acme");
        c.wallet.credit(Credits::from_whole(1000));
        c.issue_shares(1, 75);
        c.issue_shares(2, 25);
        let payouts = c.pay_dividend(Credits::from_whole(100));
        // 75% to player 1, 25% to player 2
        assert_eq!(payouts.len(), 2);
        let p1 = payouts.iter().find(|(p, _)| *p == 1).unwrap().1;
        let p2 = payouts.iter().find(|(p, _)| *p == 2).unwrap().1;
        assert_eq!(p1, Credits::from_whole(75));
        assert_eq!(p2, Credits::from_whole(25));
        // Wallet debited by total paid
        assert_eq!(c.wallet.balance, Credits::from_whole(900));
    }

    #[test]
    fn corporation_dividend_insufficient_funds_returns_empty() {
        let mut c = Corporation::new("Acme");
        c.wallet.credit(Credits::from_whole(10));
        c.issue_shares(1, 100);
        let payouts = c.pay_dividend(Credits::from_whole(100));
        assert!(payouts.is_empty());
        // Wallet unchanged
        assert_eq!(c.wallet.balance, Credits::from_whole(10));
    }

    #[test]
    fn corporation_dividend_no_shares_returns_empty() {
        let mut c = Corporation::new("Acme");
        c.wallet.credit(Credits::from_whole(1000));
        let payouts = c.pay_dividend(Credits::from_whole(100));
        assert!(payouts.is_empty());
    }
}
