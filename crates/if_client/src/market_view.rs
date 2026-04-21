// market_view.rs: Market + Contracts UI for Phase 6.
//
// This module owns the client-side market and contract-board UIs. The types
// here are *placeholders* — the real authoritative ones will live in the
// `if_economy` crate. The orchestrator will wire them together at merge time.
//
// Hotkeys:
//   * `K` toggles the Market panel (order book + trade entry)
//   * `J` toggles the Jobs panel (contract board)
//
// Money is never f32/f64 — we model credits as `i64` cents in `CreditsUi`.

// Placeholder types mirror `if_economy`'s shape; some fields/methods will only
// be read/used once the orchestrator wires them together. Suppress dead-code
// warnings on the whole module so the placeholder surface stays intact.
#![allow(dead_code)]

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::{HashMap, VecDeque};

use if_common::item::ItemType;

use crate::ui_panels::EguiWantsPointer;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// All item types in the game (same list used by other UI modules).
pub const UI_MARKET_ITEMS: &[ItemType] = &[
    ItemType::CopperOre,
    ItemType::IronOre,
    ItemType::CopperIngot,
    ItemType::IronIngot,
    ItemType::CopperPlate,
    ItemType::IronPlate,
    ItemType::CopperWire,
    ItemType::BasicCircuit,
    ItemType::HullPlate,
];

/// Hard cap on the price-history ring buffer.
pub const PRICE_HISTORY_MAX: usize = 20;

/// The player's starting wallet in whole credits (demo only).
pub const STARTING_WALLET_WHOLE: i64 = 10_000;

// ---------------------------------------------------------------------------
// Placeholder money/order types (shape matches the upcoming `if_economy`)
// ---------------------------------------------------------------------------

/// Credits expressed in integer cents. 1 whole credit = 100 units.
///
/// Never use floats for money — rounding errors would compound over trades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct CreditsUi(pub i64);

impl CreditsUi {
    pub fn from_whole(n: i64) -> Self {
        Self(n * 100)
    }

    /// Human-friendly representation, e.g. `10000.00 cr` or `-3.05 cr`.
    pub fn display(self) -> String {
        let whole = self.0 / 100;
        let cents = (self.0 % 100).abs();
        format!("{whole}.{cents:02} cr")
    }
}

impl std::ops::Add for CreditsUi {
    type Output = CreditsUi;
    fn add(self, rhs: CreditsUi) -> CreditsUi {
        CreditsUi(self.0 + rhs.0)
    }
}

impl std::ops::Sub for CreditsUi {
    type Output = CreditsUi;
    fn sub(self, rhs: CreditsUi) -> CreditsUi {
        CreditsUi(self.0 - rhs.0)
    }
}

impl std::ops::AddAssign for CreditsUi {
    fn add_assign(&mut self, rhs: CreditsUi) {
        self.0 += rhs.0;
    }
}

impl std::ops::SubAssign for CreditsUi {
    fn sub_assign(&mut self, rhs: CreditsUi) {
        self.0 -= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrderSideUi {
    #[default]
    Buy,
    Sell,
}

impl OrderSideUi {
    pub fn label(self) -> &'static str {
        match self {
            OrderSideUi::Buy => "Buy",
            OrderSideUi::Sell => "Sell",
        }
    }

    pub fn opposite(self) -> OrderSideUi {
        match self {
            OrderSideUi::Buy => OrderSideUi::Sell,
            OrderSideUi::Sell => OrderSideUi::Buy,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrderUi {
    pub id: u64,
    pub side: OrderSideUi,
    pub item: ItemType,
    pub price: CreditsUi,
    pub quantity: u32,
    pub placed_by: u64,
}

/// One order book for one item. Bids sorted descending by price; asks
/// ascending. This matches the usual matching-engine convention.
#[derive(Clone, Debug)]
pub struct OrderBookUi {
    pub item: ItemType,
    pub buys: Vec<OrderUi>,
    pub sells: Vec<OrderUi>,
    pub last_price: Option<CreditsUi>,
    pub price_history: VecDeque<CreditsUi>,
}

impl OrderBookUi {
    pub fn new(item: ItemType) -> Self {
        Self {
            item,
            buys: Vec::new(),
            sells: Vec::new(),
            last_price: None,
            price_history: VecDeque::with_capacity(PRICE_HISTORY_MAX),
        }
    }

    pub fn best_bid(&self) -> Option<CreditsUi> {
        self.buys.first().map(|o| o.price)
    }

    pub fn best_ask(&self) -> Option<CreditsUi> {
        self.sells.first().map(|o| o.price)
    }

    pub fn spread(&self) -> Option<CreditsUi> {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        }
    }

    /// Push a new last-trade price, capping the ring buffer at
    /// `PRICE_HISTORY_MAX` samples.
    pub fn record_trade(&mut self, price: CreditsUi) {
        self.last_price = Some(price);
        if self.price_history.len() == PRICE_HISTORY_MAX {
            self.price_history.pop_front();
        }
        self.price_history.push_back(price);
    }

    /// Insert an order maintaining sort order (bids desc, asks asc). Stable
    /// when prices match so older orders keep priority.
    pub fn insert_sorted(&mut self, order: OrderUi) {
        match order.side {
            OrderSideUi::Buy => {
                let idx = self
                    .buys
                    .iter()
                    .position(|o| o.price < order.price)
                    .unwrap_or(self.buys.len());
                self.buys.insert(idx, order);
            }
            OrderSideUi::Sell => {
                let idx = self
                    .sells
                    .iter()
                    .position(|o| o.price > order.price)
                    .unwrap_or(self.sells.len());
                self.sells.insert(idx, order);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Resource, Clone, Debug, Default)]
pub struct MarketsUi {
    pub books: HashMap<ItemType, OrderBookUi>,
    pub player_wallet: CreditsUi,
    /// Player-owned quantities of each item. Tracked here (instead of in
    /// `if_factory::Inventory`) so the market UI can validate sell orders
    /// without reaching into the factory graph.
    pub player_holdings: HashMap<ItemType, u32>,
    pub player_id: u64,
    pub next_order_id: u64,
}

impl MarketsUi {
    pub fn book(&self, item: ItemType) -> Option<&OrderBookUi> {
        self.books.get(&item)
    }

    pub fn book_mut(&mut self, item: ItemType) -> &mut OrderBookUi {
        self.books
            .entry(item)
            .or_insert_with(|| OrderBookUi::new(item))
    }

    pub fn next_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    pub fn holdings(&self, item: ItemType) -> u32 {
        self.player_holdings.get(&item).copied().unwrap_or(0)
    }
}

#[derive(Resource, Clone, Debug)]
pub struct MarketUiState {
    pub open: bool,
    pub selected_item: Option<ItemType>,
    pub draft_side: OrderSideUi,
    pub draft_price: CreditsUi,
    pub draft_quantity: u32,
}

impl Default for MarketUiState {
    fn default() -> Self {
        Self {
            open: false,
            selected_item: Some(ItemType::CopperOre),
            draft_side: OrderSideUi::default(),
            draft_price: CreditsUi::from_whole(10),
            draft_quantity: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Contracts (placeholder types)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContractKindUi {
    #[default]
    Courier,
    Manufacturing,
    Mercenary,
}

impl ContractKindUi {
    pub const ALL: &'static [ContractKindUi] = &[
        ContractKindUi::Courier,
        ContractKindUi::Manufacturing,
        ContractKindUi::Mercenary,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ContractKindUi::Courier => "Courier",
            ContractKindUi::Manufacturing => "Manufacturing",
            ContractKindUi::Mercenary => "Mercenary",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContractUi {
    pub id: u64,
    pub kind: ContractKindUi,
    pub item: ItemType,
    pub quantity: u32,
    pub reward: CreditsUi,
    pub poster: String,
    pub accepted_by: Option<u64>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ContractBoardUi {
    pub contracts: Vec<ContractUi>,
    pub next_id: u64,
}

impl ContractBoardUi {
    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn post(&mut self, mut contract: ContractUi) -> u64 {
        let id = self.next_id();
        contract.id = id;
        self.contracts.push(contract);
        id
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ContractUiState {
    pub open: bool,
    pub draft_kind: ContractKindUi,
    pub draft_item: ItemType,
    pub draft_quantity: u32,
    pub draft_reward: CreditsUi,
    pub draft_poster: String,
}

impl Default for ContractUiState {
    fn default() -> Self {
        Self {
            open: false,
            draft_kind: ContractKindUi::default(),
            draft_item: ItemType::CopperOre,
            draft_quantity: 10,
            draft_reward: CreditsUi::from_whole(100),
            draft_poster: "Player".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Demo data
// ---------------------------------------------------------------------------

/// Tiny deterministic LCG so we don't need rand just for demo seeding.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u32(&mut self, modulo: u32) -> u32 {
        // Numerical Recipes constants.
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        ((self.0 >> 16) as u32) % modulo.max(1)
    }
}

/// Startup system: populate demo orders for every item, plus give the player
/// a demo wallet and some starting inventory.
pub fn init_markets_ui(mut markets: ResMut<MarketsUi>) {
    markets.player_wallet = CreditsUi::from_whole(STARTING_WALLET_WHOLE);
    markets.player_id = 1;
    markets.next_order_id = 0;

    // Seed player holdings so sell orders can be validated in the demo.
    for &item in UI_MARKET_ITEMS {
        markets.player_holdings.insert(item, 50);
    }

    let base_prices: &[(ItemType, i64)] = &[
        (ItemType::CopperOre, 5),
        (ItemType::IronOre, 5),
        (ItemType::CopperIngot, 12),
        (ItemType::IronIngot, 12),
        (ItemType::CopperPlate, 20),
        (ItemType::IronPlate, 20),
        (ItemType::CopperWire, 30),
        (ItemType::BasicCircuit, 75),
        (ItemType::HullPlate, 150),
    ];

    let mut rng = Lcg::new(0xC0FFEE);

    for &(item, base_whole) in base_prices {
        let base = CreditsUi::from_whole(base_whole);
        let book = markets.book_mut(item);

        // 3 bids below base, 3 asks above base.
        for i in 0..3 {
            // Price offset in cents: 50–300 below base.
            let off = 50 + rng.next_u32(250) as i64;
            let bid_price = CreditsUi(base.0 - off);
            let qty = 5 + rng.next_u32(20);
            book.insert_sorted(OrderUi {
                id: i as u64,
                side: OrderSideUi::Buy,
                item,
                price: bid_price,
                quantity: qty,
                placed_by: 1000 + i as u64, // some NPC id
            });

            let off = 50 + rng.next_u32(250) as i64;
            let ask_price = CreditsUi(base.0 + off);
            let qty = 5 + rng.next_u32(20);
            book.insert_sorted(OrderUi {
                id: (100 + i) as u64,
                side: OrderSideUi::Sell,
                item,
                price: ask_price,
                quantity: qty,
                placed_by: 2000 + i as u64,
            });
        }

        // Seed a little price history roughly around the base.
        for _ in 0..PRICE_HISTORY_MAX / 2 {
            let jitter = rng.next_u32(200) as i64 - 100;
            book.record_trade(CreditsUi(base.0 + jitter));
        }
    }

    // Make sure the next issued order id doesn't collide with the demo ids.
    markets.next_order_id = 10_000;
}

/// Startup system: seed a couple of contracts on the job board.
pub fn init_contracts_ui(mut board: ResMut<ContractBoardUi>) {
    board.post(ContractUi {
        id: 0,
        kind: ContractKindUi::Courier,
        item: ItemType::CopperOre,
        quantity: 50,
        reward: CreditsUi::from_whole(250),
        poster: "Mercurius Mining Co.".to_string(),
        accepted_by: None,
    });
    board.post(ContractUi {
        id: 0,
        kind: ContractKindUi::Manufacturing,
        item: ItemType::BasicCircuit,
        quantity: 10,
        reward: CreditsUi::from_whole(1200),
        poster: "Sol Electronics".to_string(),
        accepted_by: None,
    });
    board.post(ContractUi {
        id: 0,
        kind: ContractKindUi::Mercenary,
        item: ItemType::HullPlate,
        quantity: 5,
        reward: CreditsUi::from_whole(2000),
        poster: "Union Shipyards".to_string(),
        accepted_by: None,
    });
}

// ---------------------------------------------------------------------------
// Hotkeys
// ---------------------------------------------------------------------------

pub fn market_hotkey_system(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<MarketUiState>) {
    if keyboard.just_pressed(KeyCode::KeyK) {
        state.open = !state.open;
    }
}

pub fn contracts_hotkey_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ContractUiState>,
) {
    if keyboard.just_pressed(KeyCode::KeyJ) {
        state.open = !state.open;
    }
}

// ---------------------------------------------------------------------------
// Matching engine (placeholder)
// ---------------------------------------------------------------------------

/// A single executed fill, returned by `match_order_ui`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeUi {
    pub item: ItemType,
    pub price: CreditsUi,
    pub quantity: u32,
}

/// Match a newly-submitted order against the resting side. Trades execute at
/// the *resting* order's price (price improvement for the taker).
///
/// Wallet / holdings are updated for the taker; NPC counterparty balances are
/// not tracked in this demo. The unfilled remainder is pushed onto the book.
///
/// This is a deliberately simple stand-in for `if_economy::OrderBook::submit`
/// — the orchestrator will replace it at merge time.
pub fn match_order_ui(
    markets: &mut MarketsUi,
    side: OrderSideUi,
    item: ItemType,
    limit_price: CreditsUi,
    mut quantity: u32,
) -> Vec<TradeUi> {
    let mut trades = Vec::new();
    if quantity == 0 {
        return trades;
    }
    let taker_id = markets.player_id;
    let next_id = markets.next_id();
    let book = markets.book_mut(item);

    // Walk resting orders on the opposite side until we fill or can't cross.
    loop {
        if quantity == 0 {
            break;
        }
        let resting_price_opt = match side {
            OrderSideUi::Buy => book.sells.first().map(|o| o.price),
            OrderSideUi::Sell => book.buys.first().map(|o| o.price),
        };
        let Some(resting_price) = resting_price_opt else {
            break;
        };

        // Check crossing.
        let crosses = match side {
            OrderSideUi::Buy => limit_price >= resting_price,
            OrderSideUi::Sell => limit_price <= resting_price,
        };
        if !crosses {
            break;
        }

        // Consume from the front of the resting book.
        let (fill_qty, resting_consumed) = {
            let resting = match side {
                OrderSideUi::Buy => &mut book.sells[0],
                OrderSideUi::Sell => &mut book.buys[0],
            };
            let fill = quantity.min(resting.quantity);
            resting.quantity -= fill;
            (fill, resting.quantity == 0)
        };

        // Record trade at the resting price.
        trades.push(TradeUi {
            item,
            price: resting_price,
            quantity: fill_qty,
        });
        book.record_trade(resting_price);
        quantity -= fill_qty;

        if resting_consumed {
            match side {
                OrderSideUi::Buy => {
                    book.sells.remove(0);
                }
                OrderSideUi::Sell => {
                    book.buys.remove(0);
                }
            }
        }
    }

    // Any remainder rests on the book at the limit price.
    if quantity > 0 {
        book.insert_sorted(OrderUi {
            id: next_id,
            side,
            item,
            price: limit_price,
            quantity,
            placed_by: taker_id,
        });
    }

    // Apply fills to the player's wallet + holdings.
    for trade in &trades {
        let total = CreditsUi(trade.price.0 * trade.quantity as i64);
        match side {
            OrderSideUi::Buy => {
                markets.player_wallet -= total;
                *markets.player_holdings.entry(item).or_insert(0) += trade.quantity;
            }
            OrderSideUi::Sell => {
                markets.player_wallet += total;
                let held = markets.player_holdings.entry(item).or_insert(0);
                *held = held.saturating_sub(trade.quantity);
            }
        }
    }

    trades
}

/// Pre-trade validation for the Submit button. Returns `None` if the order is
/// safe to place, `Some(reason)` describing why it's blocked.
pub fn validate_order_ui(
    markets: &MarketsUi,
    side: OrderSideUi,
    item: ItemType,
    price: CreditsUi,
    quantity: u32,
) -> Option<String> {
    if quantity == 0 {
        return Some("Quantity must be at least 1".to_string());
    }
    if price.0 <= 0 {
        return Some("Price must be positive".to_string());
    }
    match side {
        OrderSideUi::Buy => {
            let cost = CreditsUi(price.0 * quantity as i64);
            if cost > markets.player_wallet {
                return Some(format!(
                    "Insufficient credits (need {}, have {})",
                    cost.display(),
                    markets.player_wallet.display()
                ));
            }
        }
        OrderSideUi::Sell => {
            let held = markets.holdings(item);
            if quantity > held {
                return Some(format!(
                    "Insufficient holdings ({held} in stock, {quantity} requested)"
                ));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Market panel
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn market_panel(
    mut contexts: EguiContexts,
    mut markets: ResMut<MarketsUi>,
    mut state: ResMut<MarketUiState>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if !state.open {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut submit_requested = false;
    let mut close = false;

    egui::Window::new("Market")
        .collapsible(true)
        .resizable(true)
        .default_width(760.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Market");
                ui.separator();
                ui.label(format!("Wallet: {}", markets.player_wallet.display()));
                ui.separator();
                if ui.button("Close [K]").clicked() {
                    close = true;
                }
            });
            ui.separator();

            ui.columns(2, |cols| {
                // ------------------------------------------------------
                // Left column: item list
                // ------------------------------------------------------
                cols[0].label(egui::RichText::new("Items").strong());
                cols[0].separator();
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .id_salt("market_item_list")
                    .show(&mut cols[0], |ui| {
                        for &item in UI_MARKET_ITEMS {
                            let last = markets
                                .book(item)
                                .and_then(|b| b.last_price)
                                .map(|p| p.display())
                                .unwrap_or_else(|| "—".to_string());
                            let is_sel = state.selected_item == Some(item);
                            let label = format!("{}  ({last})", item);
                            let btn = if is_sel {
                                egui::Button::new(egui::RichText::new(label).strong())
                                    .fill(egui::Color32::from_rgb(60, 80, 120))
                            } else {
                                egui::Button::new(label)
                            };
                            if ui.add_sized(egui::vec2(200.0, 22.0), btn).clicked() {
                                state.selected_item = Some(item);
                            }
                        }
                    });

                // ------------------------------------------------------
                // Right column: order book + mini chart + place order
                // ------------------------------------------------------
                let right = &mut cols[1];
                let Some(sel) = state.selected_item else {
                    right.label("Select an item on the left to view its book.");
                    return;
                };

                // Pull a cloned snapshot so we don't hold a borrow of markets
                // across the whole UI closure.
                let book_snapshot = markets.book(sel).cloned();
                let Some(book) = book_snapshot else {
                    right.label("No book available for this item.");
                    return;
                };

                right.heading(format!("{}", sel));

                let best_bid_text = book
                    .best_bid()
                    .map(|p| p.display())
                    .unwrap_or_else(|| "—".into());
                let best_ask_text = book
                    .best_ask()
                    .map(|p| p.display())
                    .unwrap_or_else(|| "—".into());
                let spread_text = book
                    .spread()
                    .map(|p| p.display())
                    .unwrap_or_else(|| "—".into());
                let last_text = book
                    .last_price
                    .map(|p| p.display())
                    .unwrap_or_else(|| "—".into());

                right.horizontal(|ui| {
                    ui.label(format!("Best Bid: {best_bid_text}"));
                    ui.separator();
                    ui.label(format!("Best Ask: {best_ask_text}"));
                    ui.separator();
                    ui.label(format!("Spread: {spread_text}"));
                    ui.separator();
                    ui.label(format!("Last: {last_text}"));
                });

                right.separator();

                // -- Order book table (two columns: Bids | Asks) --
                right.label(egui::RichText::new("Order Book").strong());
                egui::Grid::new("order_book_grid")
                    .num_columns(2)
                    .striped(true)
                    .min_col_width(180.0)
                    .show(right, |ui| {
                        ui.label(
                            egui::RichText::new("Bids")
                                .color(egui::Color32::from_rgb(120, 200, 120))
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new("Asks")
                                .color(egui::Color32::from_rgb(220, 120, 120))
                                .strong(),
                        );
                        ui.end_row();

                        let rows = book.buys.len().max(book.sells.len()).min(10);
                        for i in 0..rows {
                            match book.buys.get(i) {
                                Some(o) => ui.label(
                                    egui::RichText::new(format!(
                                        "{} @ {}",
                                        o.quantity,
                                        o.price.display()
                                    ))
                                    .color(egui::Color32::from_rgb(120, 200, 120)),
                                ),
                                None => ui.label(""),
                            };
                            match book.sells.get(i) {
                                Some(o) => ui.label(
                                    egui::RichText::new(format!(
                                        "{} @ {}",
                                        o.quantity,
                                        o.price.display()
                                    ))
                                    .color(egui::Color32::from_rgb(220, 120, 120)),
                                ),
                                None => ui.label(""),
                            };
                            ui.end_row();
                        }
                    });

                right.separator();

                // -- Price history mini chart --
                right.label(egui::RichText::new("Price History").strong());
                draw_price_chart(right, &book.price_history);

                right.separator();

                // -- Place order panel --
                right.label(egui::RichText::new("Place Order").strong());
                right.horizontal(|ui| {
                    ui.selectable_value(&mut state.draft_side, OrderSideUi::Buy, "Buy");
                    ui.selectable_value(&mut state.draft_side, OrderSideUi::Sell, "Sell");
                });

                right.horizontal(|ui| {
                    ui.label("Price:");
                    let mut whole = state.draft_price.0 / 100;
                    let mut cents = (state.draft_price.0 % 100).abs();
                    let changed = ui
                        .add(egui::DragValue::new(&mut whole).range(0..=1_000_000))
                        .changed()
                        | ui.add(egui::DragValue::new(&mut cents).range(0..=99))
                            .changed();
                    if changed {
                        state.draft_price = CreditsUi(whole * 100 + cents);
                    }
                    ui.label("cr");
                });

                right.horizontal(|ui| {
                    ui.label("Quantity:");
                    let mut q = state.draft_quantity as i32;
                    if ui
                        .add(egui::DragValue::new(&mut q).range(1..=10_000))
                        .changed()
                    {
                        state.draft_quantity = q.max(1) as u32;
                    }
                });

                let validation = validate_order_ui(
                    &markets,
                    state.draft_side,
                    sel,
                    state.draft_price,
                    state.draft_quantity,
                );
                let total = CreditsUi(state.draft_price.0 * state.draft_quantity as i64);
                right.label(format!(
                    "Total (pre-match est.): {}  |  Holdings: {}",
                    total.display(),
                    markets.holdings(sel)
                ));
                if let Some(reason) = &validation {
                    right.colored_label(egui::Color32::from_rgb(230, 120, 120), reason);
                }

                let can_submit = validation.is_none();
                right.add_enabled_ui(can_submit, |ui| {
                    if ui.button("Submit Order").clicked() {
                        submit_requested = true;
                    }
                });
            });
        });

    // Apply deferred actions after the UI closure ends.
    if submit_requested && let Some(item) = state.selected_item {
        let _trades = match_order_ui(
            &mut markets,
            state.draft_side,
            item,
            state.draft_price,
            state.draft_quantity,
        );
    }
    if close {
        state.open = false;
    }

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

/// Paint a tiny line chart of the price history into the current ui.
fn draw_price_chart(ui: &mut egui::Ui, history: &VecDeque<CreditsUi>) {
    let desired = egui::vec2(ui.available_width().max(200.0), 80.0);
    let (rect, _resp) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Background.
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 28));

    if history.len() < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "(not enough data)",
            egui::FontId::proportional(12.0),
            egui::Color32::GRAY,
        );
        return;
    }

    // Compute min/max for vertical scaling.
    let (mut min_p, mut max_p) = (i64::MAX, i64::MIN);
    for p in history.iter() {
        if p.0 < min_p {
            min_p = p.0;
        }
        if p.0 > max_p {
            max_p = p.0;
        }
    }
    if min_p == max_p {
        // Flat line — pick a small range so we draw something visible.
        min_p -= 50;
        max_p += 50;
    }
    let span = (max_p - min_p) as f32;

    let n = history.len();
    let w = rect.width();
    let h = rect.height() - 8.0;

    let map = |i: usize, v: i64| {
        let x = rect.left() + (i as f32 / (n - 1) as f32) * w;
        let t = (v - min_p) as f32 / span;
        let y = rect.bottom() - 4.0 - t * h;
        egui::pos2(x, y)
    };

    let color = egui::Color32::from_rgb(120, 180, 240);
    let mut prev = map(0, history[0].0);
    for (i, price) in history.iter().enumerate().skip(1) {
        let cur = map(i, price.0);
        painter.line_segment([prev, cur], egui::Stroke::new(1.5, color));
        prev = cur;
    }
}

// ---------------------------------------------------------------------------
// Contracts panel
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn contracts_panel(
    mut contexts: EguiContexts,
    mut board: ResMut<ContractBoardUi>,
    mut state: ResMut<ContractUiState>,
    markets: Res<MarketsUi>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if !state.open {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut accept_idx: Option<usize> = None;
    let mut post_clicked = false;
    let mut close = false;

    egui::Window::new("Contracts / Job Board")
        .collapsible(true)
        .resizable(true)
        .default_width(560.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Job Board");
                ui.separator();
                ui.label(format!("Wallet: {}", markets.player_wallet.display()));
                ui.separator();
                if ui.button("Close [J]").clicked() {
                    close = true;
                }
            });
            ui.separator();

            ui.label(egui::RichText::new("Available Contracts").strong());
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .id_salt("contract_list")
                .show(ui, |ui| {
                    if board.contracts.is_empty() {
                        ui.label("(no contracts posted yet)");
                    }
                    for (i, c) in board.contracts.iter().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(c.kind.label()).strong());
                                ui.separator();
                                ui.label(format!("{} x {}", c.quantity, c.item));
                                ui.separator();
                                ui.label(format!("Reward: {}", c.reward.display()));
                            });
                            ui.label(format!("Poster: {}", c.poster));
                            let accepted_label = match c.accepted_by {
                                Some(id) => format!("Accepted by player #{id}"),
                                None => "Open".to_string(),
                            };
                            ui.label(accepted_label);

                            ui.horizontal(|ui| {
                                let enabled = c.accepted_by.is_none();
                                if ui
                                    .add_enabled(enabled, egui::Button::new("Accept"))
                                    .clicked()
                                {
                                    accept_idx = Some(i);
                                }
                            });
                        });
                    }
                });

            ui.separator();
            ui.label(egui::RichText::new("Post New Contract").strong());

            ui.horizontal(|ui| {
                ui.label("Kind:");
                egui::ComboBox::from_id_salt("contract_kind")
                    .selected_text(state.draft_kind.label())
                    .show_ui(ui, |ui| {
                        for &k in ContractKindUi::ALL {
                            ui.selectable_value(&mut state.draft_kind, k, k.label());
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Item:");
                egui::ComboBox::from_id_salt("contract_item")
                    .selected_text(state.draft_item.to_string())
                    .show_ui(ui, |ui| {
                        for &it in UI_MARKET_ITEMS {
                            ui.selectable_value(&mut state.draft_item, it, it.to_string());
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Quantity:");
                let mut q = state.draft_quantity as i32;
                if ui
                    .add(egui::DragValue::new(&mut q).range(1..=10_000))
                    .changed()
                {
                    state.draft_quantity = q.max(1) as u32;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Reward (whole cr):");
                let mut whole = state.draft_reward.0 / 100;
                if ui
                    .add(egui::DragValue::new(&mut whole).range(0..=1_000_000))
                    .changed()
                {
                    state.draft_reward = CreditsUi::from_whole(whole);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Poster:");
                ui.text_edit_singleline(&mut state.draft_poster);
            });

            if ui.button("Post Contract").clicked() {
                post_clicked = true;
            }
        });

    if let Some(i) = accept_idx
        && let Some(c) = board.contracts.get_mut(i)
    {
        c.accepted_by = Some(markets.player_id);
    }

    if post_clicked {
        let contract = ContractUi {
            id: 0, // filled in by board.post
            kind: state.draft_kind,
            item: state.draft_item,
            quantity: state.draft_quantity,
            reward: state.draft_reward,
            poster: if state.draft_poster.is_empty() {
                "Anonymous".to_string()
            } else {
                state.draft_poster.clone()
            },
            accepted_by: None,
        };
        board.post(contract);
    }

    if close {
        state.open = false;
    }

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_markets() -> MarketsUi {
        MarketsUi {
            player_wallet: CreditsUi::from_whole(10_000),
            player_id: 1,
            ..Default::default()
        }
    }

    #[test]
    fn credits_display() {
        assert_eq!(CreditsUi(1050).display(), "10.50 cr");
        assert_eq!(CreditsUi(100).display(), "1.00 cr");
        assert_eq!(CreditsUi(-305).display(), "-3.05 cr");
    }

    #[test]
    fn insert_sorted_maintains_order() {
        let mut book = OrderBookUi::new(ItemType::CopperOre);
        let mk = |side, price_cents| OrderUi {
            id: 0,
            side,
            item: ItemType::CopperOre,
            price: CreditsUi(price_cents),
            quantity: 1,
            placed_by: 1,
        };
        book.insert_sorted(mk(OrderSideUi::Buy, 500));
        book.insert_sorted(mk(OrderSideUi::Buy, 700));
        book.insert_sorted(mk(OrderSideUi::Buy, 600));
        // Bids descending.
        let prices: Vec<i64> = book.buys.iter().map(|o| o.price.0).collect();
        assert_eq!(prices, vec![700, 600, 500]);

        book.insert_sorted(mk(OrderSideUi::Sell, 900));
        book.insert_sorted(mk(OrderSideUi::Sell, 800));
        book.insert_sorted(mk(OrderSideUi::Sell, 1000));
        // Asks ascending.
        let prices: Vec<i64> = book.sells.iter().map(|o| o.price.0).collect();
        assert_eq!(prices, vec![800, 900, 1000]);
    }

    #[test]
    fn buy_crosses_best_ask_executes_at_ask_price() {
        let mut m = fresh_markets();
        // Resting sell at 5.00 for 10 units.
        m.book_mut(ItemType::CopperOre).insert_sorted(OrderUi {
            id: 1,
            side: OrderSideUi::Sell,
            item: ItemType::CopperOre,
            price: CreditsUi::from_whole(5),
            quantity: 10,
            placed_by: 99,
        });

        // Player submits a buy at 7.00 for 4 units — should fill at 5.00.
        let trades = match_order_ui(
            &mut m,
            OrderSideUi::Buy,
            ItemType::CopperOre,
            CreditsUi::from_whole(7),
            4,
        );
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, CreditsUi::from_whole(5));
        assert_eq!(trades[0].quantity, 4);

        // Book has 6 left on the ask.
        let book = m.book(ItemType::CopperOre).unwrap();
        assert_eq!(book.sells.len(), 1);
        assert_eq!(book.sells[0].quantity, 6);
        assert_eq!(book.last_price, Some(CreditsUi::from_whole(5)));

        // Wallet decreased by 4 * 5.00.
        assert_eq!(m.player_wallet, CreditsUi::from_whole(10_000 - 20));
        assert_eq!(m.holdings(ItemType::CopperOre), 4);
    }

    #[test]
    fn sell_crosses_best_bid_executes_at_bid_price() {
        let mut m = fresh_markets();
        m.player_holdings.insert(ItemType::IronOre, 50);
        // Resting bid at 8.00 for 10 units.
        m.book_mut(ItemType::IronOre).insert_sorted(OrderUi {
            id: 1,
            side: OrderSideUi::Buy,
            item: ItemType::IronOre,
            price: CreditsUi::from_whole(8),
            quantity: 10,
            placed_by: 99,
        });

        // Player sells at 6.00 for 3 — fills at 8.00.
        let trades = match_order_ui(
            &mut m,
            OrderSideUi::Sell,
            ItemType::IronOre,
            CreditsUi::from_whole(6),
            3,
        );
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, CreditsUi::from_whole(8));
        assert_eq!(m.player_wallet, CreditsUi::from_whole(10_000 + 24));
        assert_eq!(m.holdings(ItemType::IronOre), 47);
    }

    #[test]
    fn partial_fill_leaves_remainder_on_book() {
        let mut m = fresh_markets();
        // Single resting ask of 5 units at 10.00.
        m.book_mut(ItemType::CopperIngot).insert_sorted(OrderUi {
            id: 1,
            side: OrderSideUi::Sell,
            item: ItemType::CopperIngot,
            price: CreditsUi::from_whole(10),
            quantity: 5,
            placed_by: 99,
        });

        // Player buys 8 at 20.00. 5 fill, 3 rest as a new bid at 20.00.
        let trades = match_order_ui(
            &mut m,
            OrderSideUi::Buy,
            ItemType::CopperIngot,
            CreditsUi::from_whole(20),
            8,
        );
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, 5);

        let book = m.book(ItemType::CopperIngot).unwrap();
        assert!(book.sells.is_empty());
        assert_eq!(book.buys.len(), 1);
        assert_eq!(book.buys[0].quantity, 3);
        assert_eq!(book.buys[0].price, CreditsUi::from_whole(20));
    }

    #[test]
    fn no_cross_means_order_rests() {
        let mut m = fresh_markets();
        m.book_mut(ItemType::HullPlate).insert_sorted(OrderUi {
            id: 1,
            side: OrderSideUi::Sell,
            item: ItemType::HullPlate,
            price: CreditsUi::from_whole(200),
            quantity: 5,
            placed_by: 99,
        });

        // Player bids at 150 — below the ask, so just rests.
        let trades = match_order_ui(
            &mut m,
            OrderSideUi::Buy,
            ItemType::HullPlate,
            CreditsUi::from_whole(150),
            2,
        );
        assert!(trades.is_empty());

        let book = m.book(ItemType::HullPlate).unwrap();
        assert_eq!(book.buys.len(), 1);
        assert_eq!(book.sells.len(), 1);
        assert_eq!(book.last_price, None);
        // Wallet untouched — no trade happened.
        assert_eq!(m.player_wallet, CreditsUi::from_whole(10_000));
    }

    #[test]
    fn price_history_ring_buffer_caps() {
        let mut book = OrderBookUi::new(ItemType::CopperOre);
        for i in 0..(PRICE_HISTORY_MAX as i64 + 5) {
            book.record_trade(CreditsUi(i));
        }
        assert_eq!(book.price_history.len(), PRICE_HISTORY_MAX);
        // Oldest samples (0..=4) should have been dropped.
        assert_eq!(book.price_history.front().copied(), Some(CreditsUi(5)));
        assert_eq!(
            book.price_history.back().copied(),
            Some(CreditsUi(PRICE_HISTORY_MAX as i64 + 4))
        );
    }

    #[test]
    fn validate_buy_rejects_over_wallet() {
        let m = fresh_markets();
        // 10,000.01 cr for 1 unit — over the 10,000.00 wallet.
        let reason = validate_order_ui(
            &m,
            OrderSideUi::Buy,
            ItemType::CopperOre,
            CreditsUi(1_000_001),
            1,
        );
        assert!(reason.is_some());
    }

    #[test]
    fn validate_sell_rejects_over_holdings() {
        let mut m = fresh_markets();
        m.player_holdings.insert(ItemType::IronOre, 5);
        let reason = validate_order_ui(
            &m,
            OrderSideUi::Sell,
            ItemType::IronOre,
            CreditsUi::from_whole(10),
            6,
        );
        assert!(reason.is_some());
        // 5 is allowed.
        let ok = validate_order_ui(
            &m,
            OrderSideUi::Sell,
            ItemType::IronOre,
            CreditsUi::from_whole(10),
            5,
        );
        assert!(ok.is_none());
    }

    #[test]
    fn validate_rejects_zero_quantity_and_nonpositive_price() {
        let m = fresh_markets();
        assert!(
            validate_order_ui(
                &m,
                OrderSideUi::Buy,
                ItemType::CopperOre,
                CreditsUi::from_whole(1),
                0
            )
            .is_some()
        );
        assert!(
            validate_order_ui(&m, OrderSideUi::Buy, ItemType::CopperOre, CreditsUi(0), 1).is_some()
        );
    }

    #[test]
    fn contract_board_post_assigns_ids() {
        let mut board = ContractBoardUi::default();
        let id1 = board.post(ContractUi {
            id: 0,
            kind: ContractKindUi::Courier,
            item: ItemType::CopperOre,
            quantity: 10,
            reward: CreditsUi::from_whole(100),
            poster: "X".into(),
            accepted_by: None,
        });
        let id2 = board.post(ContractUi {
            id: 0,
            kind: ContractKindUi::Courier,
            item: ItemType::IronOre,
            quantity: 10,
            reward: CreditsUi::from_whole(100),
            poster: "Y".into(),
            accepted_by: None,
        });
        assert_ne!(id1, id2);
        assert_eq!(board.contracts.len(), 2);
    }

    #[test]
    fn init_markets_ui_populates_all_items() {
        let mut world = World::new();
        world.init_resource::<MarketsUi>();
        let mut schedule = Schedule::default();
        schedule.add_systems(init_markets_ui);
        schedule.run(&mut world);

        let markets = world.resource::<MarketsUi>();
        assert_eq!(markets.player_wallet, CreditsUi::from_whole(10_000));
        for &item in UI_MARKET_ITEMS {
            let book = markets.book(item).expect("book missing");
            assert!(!book.buys.is_empty(), "no bids for {item:?}");
            assert!(!book.sells.is_empty(), "no asks for {item:?}");
        }
    }
}
