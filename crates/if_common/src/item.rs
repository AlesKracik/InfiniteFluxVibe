// item.rs: Item types and item stacks.
//
// Items are the lifeblood of the factory — everything that gets mined,
// transported, processed, and traded is an Item.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Every distinct material or product in the game.
///
/// This enum will grow over time. The beauty of Rust enums + `match` is that
/// when you add a new variant, the compiler tells you every place that needs
/// updating (exhaustiveness checking). Try adding a variant and see what breaks.
///
/// `Hash` is derived so we can use ItemType as a HashMap key (for inventories,
/// recipe lookups, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemType {
    // --- Raw ores (extracted from resource nodes) ---
    CopperOre,
    IronOre,

    // --- Smelted materials ---
    CopperIngot,
    IronIngot,

    // --- Processed components ---
    CopperPlate,
    IronPlate,
    CopperWire,
    BasicCircuit,

    // --- Ship components (goal of the tutorial chain) ---
    HullPlate,
}

impl fmt::Display for ItemType {
    /// Human-readable names for the UI.
    ///
    /// `Display` is the trait behind `{}` in format strings (as opposed to
    /// `Debug` which is `{:?}`). We implement it manually rather than derive
    /// because we want "Copper Ore" not "CopperOre".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemType::CopperOre => write!(f, "Copper Ore"),
            ItemType::IronOre => write!(f, "Iron Ore"),
            ItemType::CopperIngot => write!(f, "Copper Ingot"),
            ItemType::IronIngot => write!(f, "Iron Ingot"),
            ItemType::CopperPlate => write!(f, "Copper Plate"),
            ItemType::IronPlate => write!(f, "Iron Plate"),
            ItemType::CopperWire => write!(f, "Copper Wire"),
            ItemType::BasicCircuit => write!(f, "Basic Circuit"),
            ItemType::HullPlate => write!(f, "Hull Plate"),
        }
    }
}

impl ItemType {
    /// Returns a color for rendering this item type.
    pub fn color(&self) -> Color {
        match self {
            ItemType::CopperOre => Color::srgb(0.72, 0.45, 0.2),
            ItemType::IronOre => Color::srgb(0.55, 0.55, 0.6),
            ItemType::CopperIngot => Color::srgb(0.85, 0.55, 0.25),
            ItemType::IronIngot => Color::srgb(0.65, 0.65, 0.7),
            ItemType::CopperPlate => Color::srgb(0.9, 0.65, 0.3),
            ItemType::IronPlate => Color::srgb(0.7, 0.7, 0.75),
            ItemType::CopperWire => Color::srgb(0.95, 0.6, 0.2),
            ItemType::BasicCircuit => Color::srgb(0.2, 0.7, 0.3),
            ItemType::HullPlate => Color::srgb(0.5, 0.5, 0.55),
        }
    }
}

/// A stack of items: a type + quantity.
///
/// This is the fundamental unit of "stuff" in the game. A mining drill
/// produces an ItemStack. A transport line carries ItemStacks. A machine
/// consumes and produces ItemStacks.
///
/// We use `u32` for quantity because items are discrete (no half-items)
/// and never negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: ItemType,
    pub quantity: u32,
}

impl ItemStack {
    pub fn new(item: ItemType, quantity: u32) -> Self {
        Self { item, quantity }
    }
}

impl fmt::Display for ItemStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x {}", self.quantity, self.item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_stack_display() {
        let stack = ItemStack::new(ItemType::CopperOre, 5);
        assert_eq!(format!("{stack}"), "5x Copper Ore");
    }

    #[test]
    fn item_stack_equality() {
        let a = ItemStack::new(ItemType::IronOre, 10);
        let b = ItemStack::new(ItemType::IronOre, 10);
        let c = ItemStack::new(ItemType::IronOre, 5);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
