// order_book.rs: Price-time priority order book & matching engine.
//
// The order book is the heart of the market. Players submit Buy or Sell
// orders; the matching engine crosses them against the opposite side and
// emits Trade records. Any unfilled remainder rests on the book as a
// resting order until cancelled or matched.
//
// # Matching rules (price-time priority)
//
// 1. A new Buy at price P matches Sells with `ask <= P`, starting from
//    the *lowest* ask. If two sells share an ask, the one placed earlier
//    (FIFO) fills first.
// 2. A new Sell at price P matches Buys with `bid >= P`, starting from
//    the *highest* bid, FIFO within a price.
// 3. Trade executes at the **resting order's price**. In exchange
//    terminology, the maker (resting order) sets the price; the taker
//    (new order) pays it.
// 4. Orders partially fill: the resting order's `quantity` is decremented,
//    and if it hits zero, it's removed. Fully-filled new orders produce
//    no remainder; any leftover of the new order is pushed onto its side
//    of the book.
//
// # Storage
//
// We keep buys sorted *descending* by price (best bid first, index 0)
// and sells sorted *ascending* (best ask first, index 0). Within a price
// level the vector order is the insertion order — older orders earlier.
// Binary insertion keeps this O(log n + n) for each submit, which is fine
// for the order volumes we expect.

use crate::credits::Credits;
use if_common::item::ItemType;
use serde::{Deserialize, Serialize};

/// Which side of the book an order sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// A single resting or incoming order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Globally unique ID — allocated by `Markets::next_id`.
    pub id: u64,
    pub side: OrderSide,
    pub item: ItemType,
    /// Price per unit (Credits).
    pub price: Credits,
    /// Remaining unfilled quantity.
    pub quantity: u32,
    /// Player/entity that placed the order.
    pub placed_by: u64,
}

impl Order {
    pub fn new(
        id: u64,
        side: OrderSide,
        item: ItemType,
        price: Credits,
        quantity: u32,
        placed_by: u64,
    ) -> Self {
        Self {
            id,
            side,
            item,
            price,
            quantity,
            placed_by,
        }
    }
}

/// A completed trade — the result of matching a taker against a maker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub item: ItemType,
    /// Execution price (always the resting/maker order's price).
    pub price: Credits,
    pub quantity: u32,
    pub buyer: u64,
    pub seller: u64,
    /// Game-tick timestamp — source-of-truth is the caller, we just record.
    pub timestamp: u64,
}

/// Per-item order book with matching engine.
///
/// Note: no `Default` derive because `ItemType` has no `Default`. Always
/// construct via `OrderBook::new(item)` — `Markets::book_mut` does this
/// automatically.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderBook {
    pub item: ItemType,
    /// Buy orders, best (highest) bid at index 0.
    buys: Vec<Order>,
    /// Sell orders, best (lowest) ask at index 0.
    sells: Vec<Order>,
    /// Last trade price, for marking the market.
    pub last_price: Option<Credits>,
    /// Full trade history (trimmed externally if memory becomes a concern).
    pub trade_history: Vec<Trade>,
}

impl OrderBook {
    /// Create an empty book for the given item.
    pub fn new(item: ItemType) -> Self {
        Self {
            item,
            buys: Vec::new(),
            sells: Vec::new(),
            last_price: None,
            trade_history: Vec::new(),
        }
    }

    /// Submit an order. Attempts to match against existing resting orders,
    /// then rests any remainder on the book. Returns trades executed.
    ///
    /// The `now` argument is the game tick used as the trade timestamp.
    pub fn submit(&mut self, mut order: Order, now: u64) -> Vec<Trade> {
        // Defensive: an order for the wrong item should never be routed here,
        // but if it is, we refuse to match — we preserve the caller's order by
        // returning empty trades and not resting it. This prevents cross-item
        // contamination of the book.
        if order.item != self.item {
            return Vec::new();
        }
        if order.quantity == 0 {
            return Vec::new();
        }

        let mut trades = Vec::new();
        match order.side {
            OrderSide::Buy => {
                // Match against sells (ascending ask order).
                while order.quantity > 0 {
                    // Best ask at index 0 — must be <= our bid price.
                    let Some(best_sell) = self.sells.first() else {
                        break;
                    };
                    if best_sell.price > order.price {
                        break;
                    }
                    // Trade at the maker's price (resting sell).
                    let fill_qty = order.quantity.min(best_sell.quantity);
                    let trade_price = best_sell.price;
                    let seller_id = best_sell.placed_by;
                    trades.push(Trade {
                        item: self.item,
                        price: trade_price,
                        quantity: fill_qty,
                        buyer: order.placed_by,
                        seller: seller_id,
                        timestamp: now,
                    });
                    order.quantity -= fill_qty;
                    let sell = &mut self.sells[0];
                    sell.quantity -= fill_qty;
                    if sell.quantity == 0 {
                        self.sells.remove(0);
                    }
                    self.last_price = Some(trade_price);
                }
                if order.quantity > 0 {
                    // Insert into buys, keeping descending-price + FIFO-within-price.
                    let pos = self
                        .buys
                        .iter()
                        .position(|o| o.price < order.price)
                        .unwrap_or(self.buys.len());
                    self.buys.insert(pos, order);
                }
            }
            OrderSide::Sell => {
                // Match against buys (descending bid order).
                while order.quantity > 0 {
                    let Some(best_buy) = self.buys.first() else {
                        break;
                    };
                    if best_buy.price < order.price {
                        break;
                    }
                    let fill_qty = order.quantity.min(best_buy.quantity);
                    let trade_price = best_buy.price;
                    let buyer_id = best_buy.placed_by;
                    trades.push(Trade {
                        item: self.item,
                        price: trade_price,
                        quantity: fill_qty,
                        buyer: buyer_id,
                        seller: order.placed_by,
                        timestamp: now,
                    });
                    order.quantity -= fill_qty;
                    let buy = &mut self.buys[0];
                    buy.quantity -= fill_qty;
                    if buy.quantity == 0 {
                        self.buys.remove(0);
                    }
                    self.last_price = Some(trade_price);
                }
                if order.quantity > 0 {
                    // Insert into sells, keeping ascending-price + FIFO-within-price.
                    let pos = self
                        .sells
                        .iter()
                        .position(|o| o.price > order.price)
                        .unwrap_or(self.sells.len());
                    self.sells.insert(pos, order);
                }
            }
        }

        // Record to history (cloned — history is the source of truth for analytics).
        self.trade_history.extend(trades.iter().cloned());
        trades
    }

    /// Highest price a buyer is willing to pay.
    pub fn best_bid(&self) -> Option<Credits> {
        self.buys.first().map(|o| o.price)
    }

    /// Lowest price a seller is willing to accept.
    pub fn best_ask(&self) -> Option<Credits> {
        self.sells.first().map(|o| o.price)
    }

    /// Ask minus bid. None if either side is empty.
    pub fn spread(&self) -> Option<Credits> {
        Some(self.best_ask()? - self.best_bid()?)
    }

    /// Cancel an order by ID from either side. Returns the removed order if found.
    pub fn cancel(&mut self, order_id: u64) -> Option<Order> {
        if let Some(idx) = self.buys.iter().position(|o| o.id == order_id) {
            return Some(self.buys.remove(idx));
        }
        if let Some(idx) = self.sells.iter().position(|o| o.id == order_id) {
            return Some(self.sells.remove(idx));
        }
        None
    }

    /// Read-only snapshot of resting buys (for UI / tests).
    pub fn buys(&self) -> &[Order] {
        &self.buys
    }
    /// Read-only snapshot of resting sells (for UI / tests).
    pub fn sells(&self) -> &[Order] {
        &self.sells
    }

    /// Total depth on a side (sum of resting quantities).
    pub fn depth(&self, side: OrderSide) -> u32 {
        let source = match side {
            OrderSide::Buy => &self.buys,
            OrderSide::Sell => &self.sells,
        };
        source.iter().map(|o| o.quantity).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cr(whole: i64) -> Credits {
        Credits::from_whole(whole)
    }

    fn buy(id: u64, price: i64, qty: u32, placed_by: u64) -> Order {
        Order::new(
            id,
            OrderSide::Buy,
            ItemType::IronOre,
            cr(price),
            qty,
            placed_by,
        )
    }

    fn sell(id: u64, price: i64, qty: u32, placed_by: u64) -> Order {
        Order::new(
            id,
            OrderSide::Sell,
            ItemType::IronOre,
            cr(price),
            qty,
            placed_by,
        )
    }

    #[test]
    fn new_book_is_empty() {
        let b = OrderBook::new(ItemType::IronOre);
        assert_eq!(b.item, ItemType::IronOre);
        assert!(b.best_bid().is_none());
        assert!(b.best_ask().is_none());
        assert!(b.last_price.is_none());
    }

    #[test]
    fn resting_buy_no_match() {
        let mut b = OrderBook::new(ItemType::IronOre);
        let trades = b.submit(buy(1, 10, 5, 100), 0);
        assert!(trades.is_empty());
        assert_eq!(b.best_bid(), Some(cr(10)));
        assert_eq!(b.buys().len(), 1);
    }

    #[test]
    fn resting_sell_no_match() {
        let mut b = OrderBook::new(ItemType::IronOre);
        let trades = b.submit(sell(1, 12, 5, 200), 0);
        assert!(trades.is_empty());
        assert_eq!(b.best_ask(), Some(cr(12)));
    }

    #[test]
    fn buy_matches_sell_at_ask_price() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 5, 200), 0); // maker @ 10
        let trades = b.submit(buy(2, 12, 5, 100), 1); // taker offers 12 but pays 10
        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert_eq!(t.price, cr(10));
        assert_eq!(t.quantity, 5);
        assert_eq!(t.buyer, 100);
        assert_eq!(t.seller, 200);
        assert_eq!(b.last_price, Some(cr(10)));
        assert!(b.sells().is_empty());
        assert!(b.buys().is_empty());
    }

    #[test]
    fn sell_matches_buy_at_bid_price() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 15, 5, 100), 0); // maker @ 15
        let trades = b.submit(sell(2, 10, 5, 200), 1); // taker asks 10 but gets 15
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, cr(15));
        assert_eq!(b.last_price, Some(cr(15)));
    }

    #[test]
    fn no_cross_when_spread_positive() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 10, 5, 100), 0);
        let trades = b.submit(sell(2, 12, 5, 200), 1);
        assert!(trades.is_empty());
        assert_eq!(b.best_bid(), Some(cr(10)));
        assert_eq!(b.best_ask(), Some(cr(12)));
        assert_eq!(b.spread(), Some(cr(2)));
    }

    #[test]
    fn partial_fill_taker_remainder_rests() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 3, 200), 0); // only 3 for sale
        let trades = b.submit(buy(2, 10, 10, 100), 1); // want 10
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, 3);
        // Remainder of 7 rests as a buy at 10
        assert_eq!(b.best_bid(), Some(cr(10)));
        assert_eq!(b.buys()[0].quantity, 7);
        assert!(b.sells().is_empty());
    }

    #[test]
    fn partial_fill_maker_remainder_on_book() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 10, 200), 0);
        let trades = b.submit(buy(2, 10, 3, 100), 1);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, 3);
        // Maker (sell) still has 7 resting
        assert_eq!(b.sells()[0].quantity, 7);
        assert!(b.buys().is_empty());
    }

    #[test]
    fn match_walks_multiple_levels() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 5, 200), 0); // best ask
        b.submit(sell(2, 11, 5, 201), 0);
        b.submit(sell(3, 12, 5, 202), 0);
        let trades = b.submit(buy(10, 11, 12, 100), 1); // want 12 @ <=11
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].price, cr(10));
        assert_eq!(trades[0].quantity, 5);
        assert_eq!(trades[1].price, cr(11));
        assert_eq!(trades[1].quantity, 5);
        // Remainder 2 rests as a buy at 11
        assert_eq!(b.best_bid(), Some(cr(11)));
        assert_eq!(b.buys()[0].quantity, 2);
        // 12 sells still on book
        assert_eq!(b.best_ask(), Some(cr(12)));
    }

    #[test]
    fn best_bid_sorted_descending() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 8, 1, 100), 0);
        b.submit(buy(2, 12, 1, 100), 0);
        b.submit(buy(3, 10, 1, 100), 0);
        let prices: Vec<_> = b.buys().iter().map(|o| o.price).collect();
        assert_eq!(prices, vec![cr(12), cr(10), cr(8)]);
    }

    #[test]
    fn best_ask_sorted_ascending() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 12, 1, 200), 0);
        b.submit(sell(2, 8, 1, 200), 0);
        b.submit(sell(3, 10, 1, 200), 0);
        let prices: Vec<_> = b.sells().iter().map(|o| o.price).collect();
        assert_eq!(prices, vec![cr(8), cr(10), cr(12)]);
    }

    #[test]
    fn fifo_at_same_price_level_buy() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 10, 5, 100), 0); // placed first
        b.submit(buy(2, 10, 5, 101), 1); // placed second
        // A sell should hit buy 1 first
        let trades = b.submit(sell(3, 10, 5, 200), 2);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].buyer, 100);
        // buy 2 still resting
        assert_eq!(b.buys().len(), 1);
        assert_eq!(b.buys()[0].id, 2);
    }

    #[test]
    fn fifo_at_same_price_level_sell() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 5, 200), 0);
        b.submit(sell(2, 10, 5, 201), 1);
        let trades = b.submit(buy(3, 10, 5, 100), 2);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].seller, 200);
        assert_eq!(b.sells().len(), 1);
        assert_eq!(b.sells()[0].id, 2);
    }

    #[test]
    fn cancel_removes_buy() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 10, 5, 100), 0);
        b.submit(buy(2, 9, 5, 100), 0);
        let cancelled = b.cancel(1).unwrap();
        assert_eq!(cancelled.id, 1);
        assert_eq!(b.buys().len(), 1);
        assert_eq!(b.buys()[0].id, 2);
    }

    #[test]
    fn cancel_removes_sell() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 5, 200), 0);
        b.submit(sell(2, 11, 5, 200), 0);
        assert!(b.cancel(2).is_some());
        assert_eq!(b.sells().len(), 1);
        assert_eq!(b.sells()[0].id, 1);
    }

    #[test]
    fn cancel_unknown_id_returns_none() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 10, 5, 100), 0);
        assert!(b.cancel(999).is_none());
    }

    #[test]
    fn trade_history_accumulates() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(sell(1, 10, 5, 200), 0);
        b.submit(sell(2, 11, 5, 200), 0);
        b.submit(buy(3, 11, 10, 100), 1);
        assert_eq!(b.trade_history.len(), 2);
    }

    #[test]
    fn zero_quantity_order_is_noop() {
        let mut b = OrderBook::new(ItemType::IronOre);
        let trades = b.submit(buy(1, 10, 0, 100), 0);
        assert!(trades.is_empty());
        assert!(b.buys().is_empty());
    }

    #[test]
    fn wrong_item_is_rejected() {
        let mut b = OrderBook::new(ItemType::IronOre);
        let bad = Order::new(1, OrderSide::Buy, ItemType::CopperOre, cr(10), 5, 100);
        let trades = b.submit(bad, 0);
        assert!(trades.is_empty());
        assert!(b.buys().is_empty());
    }

    #[test]
    fn spread_none_when_one_side_empty() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 10, 5, 100), 0);
        assert!(b.spread().is_none());
    }

    #[test]
    fn depth_sums_quantity() {
        let mut b = OrderBook::new(ItemType::IronOre);
        b.submit(buy(1, 10, 5, 100), 0);
        b.submit(buy(2, 9, 3, 100), 0);
        assert_eq!(b.depth(OrderSide::Buy), 8);
        assert_eq!(b.depth(OrderSide::Sell), 0);
    }

    #[test]
    fn trade_value_is_integer_math() {
        // Regression: ensure total = price × quantity is computed in Credits (cents),
        // never floats.
        let price = Credits::from_cents(250); // 2.50
        let qty = 7u32;
        let total = price * qty;
        assert_eq!(total, Credits::from_cents(1750));
    }
}
