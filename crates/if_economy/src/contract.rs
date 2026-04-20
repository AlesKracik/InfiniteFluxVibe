// contract.rs: Player-posted contracts & the contract board.
//
// Contracts are "jobs for hire" — a poster promises a reward in Credits
// in exchange for work done. Three shapes for now:
//   - Courier: move cargo from A to B
//   - Manufacturing: produce X of an item
//   - Mercenary: complete an objective (placeholder for Phase 6)
//
// Lifecycle:
//   1. `post` — creates a contract, assigns an id, appends to the board.
//   2. `accept` — a player claims it. One-shot; re-accepting is rejected.
//   3. `complete` — marks done. In a real deployment the server verifies
//      completion criteria before calling this.
//
// Payment is deliberately NOT handled here — the contract just describes
// the deal. The caller threads funds through the relevant Wallets at the
// right moments (escrow on post, release on complete). This keeps the
// contract module pure data.

use crate::credits::Credits;
use bevy::prelude::*;
use if_common::item::ItemType;
use serde::{Deserialize, Serialize};

/// What kind of work the contract describes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractKind {
    /// Pay someone to move cargo from A to B.
    Courier {
        from: String,
        to: String,
        item: ItemType,
        quantity: u32,
    },
    /// Pay someone to produce an item.
    Manufacturing { item: ItemType, quantity: u32 },
    /// Pay someone to complete a combat/military objective (placeholder).
    Mercenary { objective: String },
}

/// A single contract. Attached as a Component so entities (marker objects
/// the UI uses to represent a contract on the board) can carry one, and
/// also stored bulk-style in `ContractBoard.contracts` for lookup/listing.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: u64,
    pub kind: ContractKind,
    pub reward: Credits,
    pub poster: u64,
    pub accepted_by: Option<u64>,
    pub completed: bool,
}

impl Contract {
    pub fn is_open(&self) -> bool {
        self.accepted_by.is_none() && !self.completed
    }
    pub fn is_in_progress(&self) -> bool {
        self.accepted_by.is_some() && !self.completed
    }
}

/// The global board of outstanding and historical contracts.
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContractBoard {
    pub contracts: Vec<Contract>,
    next_id: u64,
}

impl ContractBoard {
    /// Create a new contract and add it to the board. Returns its id.
    pub fn post(&mut self, kind: ContractKind, reward: Credits, poster: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.contracts.push(Contract {
            id,
            kind,
            reward,
            poster,
            accepted_by: None,
            completed: false,
        });
        id
    }

    /// Accept the contract. Fails if already accepted or completed, or
    /// if the poster tries to accept their own contract.
    pub fn accept(&mut self, contract_id: u64, player: u64) -> bool {
        let Some(c) = self.contracts.iter_mut().find(|c| c.id == contract_id) else {
            return false;
        };
        if !c.is_open() {
            return false;
        }
        if c.poster == player {
            return false;
        }
        c.accepted_by = Some(player);
        true
    }

    /// Mark a contract complete. Fails if not yet accepted, or already completed.
    pub fn complete(&mut self, contract_id: u64) -> bool {
        let Some(c) = self.contracts.iter_mut().find(|c| c.id == contract_id) else {
            return false;
        };
        if c.completed || c.accepted_by.is_none() {
            return false;
        }
        c.completed = true;
        true
    }

    /// Read-only lookup by id.
    pub fn get(&self, contract_id: u64) -> Option<&Contract> {
        self.contracts.iter().find(|c| c.id == contract_id)
    }

    /// Iterator over every contract that is still accepting takers.
    pub fn open(&self) -> impl Iterator<Item = &Contract> {
        self.contracts.iter().filter(|c| c.is_open())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn courier() -> ContractKind {
        ContractKind::Courier {
            from: "Terra".into(),
            to: "Luna".into(),
            item: ItemType::IronIngot,
            quantity: 100,
        }
    }

    #[test]
    fn post_assigns_monotonic_ids() {
        let mut b = ContractBoard::default();
        let a = b.post(courier(), Credits::from_whole(100), 1);
        let c = b.post(courier(), Credits::from_whole(100), 1);
        assert_eq!(a, 0);
        assert_eq!(c, 1);
    }

    #[test]
    fn post_appends_contract() {
        let mut b = ContractBoard::default();
        let id = b.post(courier(), Credits::from_whole(500), 42);
        let c = b.get(id).unwrap();
        assert_eq!(c.poster, 42);
        assert_eq!(c.reward, Credits::from_whole(500));
        assert!(c.is_open());
    }

    #[test]
    fn accept_marks_in_progress() {
        let mut b = ContractBoard::default();
        let id = b.post(courier(), Credits::from_whole(100), 1);
        assert!(b.accept(id, 2));
        let c = b.get(id).unwrap();
        assert!(c.is_in_progress());
        assert!(!c.is_open());
        assert_eq!(c.accepted_by, Some(2));
    }

    #[test]
    fn cannot_accept_own_contract() {
        let mut b = ContractBoard::default();
        let id = b.post(courier(), Credits::from_whole(100), 1);
        assert!(!b.accept(id, 1));
    }

    #[test]
    fn cannot_double_accept() {
        let mut b = ContractBoard::default();
        let id = b.post(courier(), Credits::from_whole(100), 1);
        assert!(b.accept(id, 2));
        assert!(!b.accept(id, 3));
        let c = b.get(id).unwrap();
        assert_eq!(c.accepted_by, Some(2));
    }

    #[test]
    fn complete_requires_acceptance() {
        let mut b = ContractBoard::default();
        let id = b.post(courier(), Credits::from_whole(100), 1);
        assert!(!b.complete(id)); // not yet accepted
        b.accept(id, 2);
        assert!(b.complete(id));
        let c = b.get(id).unwrap();
        assert!(c.completed);
    }

    #[test]
    fn cannot_complete_twice() {
        let mut b = ContractBoard::default();
        let id = b.post(courier(), Credits::from_whole(100), 1);
        b.accept(id, 2);
        assert!(b.complete(id));
        assert!(!b.complete(id));
    }

    #[test]
    fn open_iterator_excludes_accepted_and_completed() {
        let mut b = ContractBoard::default();
        let a = b.post(courier(), Credits::from_whole(100), 1);
        let _c = b.post(courier(), Credits::from_whole(100), 1);
        b.accept(a, 2);
        assert_eq!(b.open().count(), 1);
    }

    #[test]
    fn unknown_id_ops_return_false() {
        let mut b = ContractBoard::default();
        assert!(!b.accept(42, 1));
        assert!(!b.complete(42));
        assert!(b.get(42).is_none());
    }

    #[test]
    fn manufacturing_contract_roundtrip() {
        let mut b = ContractBoard::default();
        let id = b.post(
            ContractKind::Manufacturing {
                item: ItemType::BasicCircuit,
                quantity: 50,
            },
            Credits::from_whole(250),
            10,
        );
        let c = b.get(id).unwrap();
        match &c.kind {
            ContractKind::Manufacturing { item, quantity } => {
                assert_eq!(*item, ItemType::BasicCircuit);
                assert_eq!(*quantity, 50);
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn mercenary_contract_holds_objective() {
        let mut b = ContractBoard::default();
        let id = b.post(
            ContractKind::Mercenary {
                objective: "Destroy pirate base alpha".into(),
            },
            Credits::from_whole(5000),
            10,
        );
        let c = b.get(id).unwrap();
        match &c.kind {
            ContractKind::Mercenary { objective } => {
                assert!(objective.contains("alpha"));
            }
            _ => panic!("wrong kind"),
        }
    }
}
