// if_economy: Economic systems for Infinite Flux Vibe.
//
// This crate implements the economy layer:
// - Fixed-point currency (Credits) — NEVER uses floats.
// - Order book matching engine (price-time priority).
// - Markets resource (one order book per item type).
// - Corporation wallets & dividends.
// - Contracts (courier, manufacturing, mercenary).
// - Price history ring buffer for charting.
//
// The cardinal rule: money is money. We use integer arithmetic only,
// stored internally as cents (1/100 of a credit). Any code that
// touches `f32`/`f64` for currency is a bug.

pub mod contract;
pub mod credits;
pub mod order_book;
pub mod price_history;
pub mod wallet;

pub use contract::{Contract, ContractBoard, ContractKind};
pub use credits::Credits;
pub use order_book::{Order, OrderBook, OrderSide, Trade};
pub use price_history::PriceHistory;
pub use wallet::{Corporation, Wallet};

use bevy::prelude::*;
use if_common::item::ItemType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The central registry of every order book in the economy.
///
/// One `OrderBook` per `ItemType`. Books are created lazily on first
/// access via `book_mut`. Order IDs are allocated from a single
/// monotonic counter so every order across all books has a unique ID
/// — this makes cancellation and audit trails simpler.
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Markets {
    /// One order book per item type.
    pub books: HashMap<ItemType, OrderBook>,
    /// Monotonic order id counter — never reused, even after cancel.
    next_order_id: u64,
}

impl Markets {
    /// Allocate the next unique order ID. Monotonically increasing.
    pub fn next_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Get (or create) the order book for a given item type.
    pub fn book_mut(&mut self, item: ItemType) -> &mut OrderBook {
        self.books
            .entry(item)
            .or_insert_with(|| OrderBook::new(item))
    }

    /// Read-only access to an item's order book, if any trades have happened there.
    pub fn book(&self, item: ItemType) -> Option<&OrderBook> {
        self.books.get(&item)
    }
}

/// The Bevy plugin that registers economy resources.
///
/// The plugin is intentionally minimal for Phase 6: we expose the
/// `Markets` and `ContractBoard` resources. Systems that advance the
/// economy (e.g. ticking interest, expiring contracts) will be added
/// in later phases — for now, game code drives matches by calling
/// `Markets::book_mut(item).submit(...)` directly.
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Markets>()
            .init_resource::<ContractBoard>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markets_next_id_is_monotonic() {
        let mut m = Markets::default();
        let a = m.next_id();
        let b = m.next_id();
        let c = m.next_id();
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 2);
    }

    #[test]
    fn markets_book_mut_creates_lazily() {
        let mut m = Markets::default();
        assert!(m.book(ItemType::IronOre).is_none());
        let _b = m.book_mut(ItemType::IronOre);
        assert!(m.book(ItemType::IronOre).is_some());
    }

    #[test]
    fn markets_separate_books_per_item() {
        let mut m = Markets::default();
        let _ = m.book_mut(ItemType::CopperOre);
        let _ = m.book_mut(ItemType::IronOre);
        assert_eq!(m.books.len(), 2);
        assert_eq!(
            m.book(ItemType::CopperOre).unwrap().item,
            ItemType::CopperOre
        );
        assert_eq!(m.book(ItemType::IronOre).unwrap().item, ItemType::IronOre);
    }

    #[test]
    fn economy_plugin_installs_resources() {
        let mut app = App::new();
        app.add_plugins(EconomyPlugin);
        assert!(app.world().get_resource::<Markets>().is_some());
        assert!(app.world().get_resource::<ContractBoard>().is_some());
    }
}
