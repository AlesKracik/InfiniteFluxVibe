// inventory.rs: Item storage for buildings.
//
// Every building that holds items (drills, machines, transport endpoints)
// has an Inventory component. This is a simple slot-based container.

use bevy::prelude::*;
use if_common::item::{ItemStack, ItemType};
use std::collections::HashMap;

/// A container that holds items, attached as a component to building entities.
///
/// Internally it's a HashMap<ItemType, u32> — mapping item types to quantities.
/// This makes adding/removing items O(1) and naturally merges stacks.
///
/// `capacity` limits how many total items can be stored. This creates
/// backpressure in the factory: if a machine's output inventory is full,
/// it stops producing, which stalls the transport line feeding it, which
/// eventually stalls the drill. This emergent behavior is the core of
/// factory optimization gameplay.
#[derive(Component, Clone, Debug, Default)]
pub struct Inventory {
    items: HashMap<ItemType, u32>,
    pub capacity: u32,
}

impl Inventory {
    pub fn new(capacity: u32) -> Self {
        Self {
            items: HashMap::new(),
            capacity,
        }
    }

    /// Total number of items across all types.
    pub fn total_count(&self) -> u32 {
        self.items.values().sum()
    }

    /// How many of a specific item type we have.
    pub fn count(&self, item: ItemType) -> u32 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    /// How much space is available.
    pub fn space_available(&self) -> u32 {
        self.capacity.saturating_sub(self.total_count())
    }

    /// Try to add items. Returns how many were actually added (may be less
    /// than requested if inventory is nearly full).
    ///
    /// This is a partial-success API: if you try to add 10 but only 3 fit,
    /// it adds 3 and returns 3. The caller decides what to do with the
    /// remaining 7 (drop them, leave them on the transport line, etc.).
    pub fn try_add(&mut self, item: ItemType, quantity: u32) -> u32 {
        let can_add = quantity.min(self.space_available());
        if can_add > 0 {
            *self.items.entry(item).or_insert(0) += can_add;
        }
        can_add
    }

    /// Try to add a full ItemStack. Returns how many were actually added.
    pub fn try_add_stack(&mut self, stack: ItemStack) -> u32 {
        self.try_add(stack.item, stack.quantity)
    }

    /// Try to remove items. Returns how many were actually removed (may be
    /// less than requested if we don't have enough).
    pub fn try_remove(&mut self, item: ItemType, quantity: u32) -> u32 {
        let have = self.count(item);
        let can_remove = quantity.min(have);
        if can_remove > 0 {
            let entry = self.items.get_mut(&item).unwrap();
            *entry -= can_remove;
            if *entry == 0 {
                self.items.remove(&item);
            }
        }
        can_remove
    }

    /// Check if the inventory contains at least this many of the given item.
    pub fn has(&self, item: ItemType, quantity: u32) -> bool {
        self.count(item) >= quantity
    }

    /// Check if the inventory contains all items in a list of stacks.
    /// Used by the production system to check if recipe inputs are available.
    pub fn has_all(&self, stacks: &[ItemStack]) -> bool {
        stacks.iter().all(|s| self.has(s.item, s.quantity))
    }

    /// Remove all items specified in a list of stacks.
    /// Panics if any item is insufficient — caller should check with `has_all` first.
    pub fn remove_all(&mut self, stacks: &[ItemStack]) {
        for stack in stacks {
            let removed = self.try_remove(stack.item, stack.quantity);
            assert_eq!(
                removed, stack.quantity,
                "Tried to remove {} {:?} but only had {}",
                stack.quantity, stack.item, removed
            );
        }
    }

    /// Get all items as a list of stacks. Useful for display.
    pub fn contents(&self) -> Vec<ItemStack> {
        self.items
            .iter()
            .map(|(&item, &quantity)| ItemStack::new(item, quantity))
            .collect()
    }

    /// Check if the inventory is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_inventory_is_empty() {
        let inv = Inventory::new(100);
        assert!(inv.is_empty());
        assert_eq!(inv.total_count(), 0);
        assert_eq!(inv.space_available(), 100);
    }

    #[test]
    fn add_and_count() {
        let mut inv = Inventory::new(100);
        inv.try_add(ItemType::CopperOre, 5);
        assert_eq!(inv.count(ItemType::CopperOre), 5);
        assert_eq!(inv.total_count(), 5);

        // Adding more of the same type merges stacks
        inv.try_add(ItemType::CopperOre, 3);
        assert_eq!(inv.count(ItemType::CopperOre), 8);
    }

    #[test]
    fn capacity_limits_addition() {
        let mut inv = Inventory::new(5);
        let added = inv.try_add(ItemType::IronOre, 10);
        assert_eq!(added, 5); // only 5 fit
        assert_eq!(inv.count(ItemType::IronOre), 5);
        assert_eq!(inv.space_available(), 0);

        // Can't add any more
        let added = inv.try_add(ItemType::CopperOre, 1);
        assert_eq!(added, 0);
    }

    #[test]
    fn remove_items() {
        let mut inv = Inventory::new(100);
        inv.try_add(ItemType::CopperOre, 10);

        let removed = inv.try_remove(ItemType::CopperOre, 3);
        assert_eq!(removed, 3);
        assert_eq!(inv.count(ItemType::CopperOre), 7);

        // Remove more than we have
        let removed = inv.try_remove(ItemType::CopperOre, 20);
        assert_eq!(removed, 7); // only had 7
        assert!(inv.is_empty());
    }

    #[test]
    fn has_all_checks_multiple_items() {
        let mut inv = Inventory::new(100);
        inv.try_add(ItemType::CopperWire, 5);
        inv.try_add(ItemType::IronPlate, 2);

        let required = vec![
            ItemStack::new(ItemType::CopperWire, 3),
            ItemStack::new(ItemType::IronPlate, 1),
        ];
        assert!(inv.has_all(&required));

        let too_much = vec![
            ItemStack::new(ItemType::CopperWire, 3),
            ItemStack::new(ItemType::IronPlate, 5), // only have 2
        ];
        assert!(!inv.has_all(&too_much));
    }

    #[test]
    fn remove_all_consumes_exact_amounts() {
        let mut inv = Inventory::new(100);
        inv.try_add(ItemType::CopperWire, 5);
        inv.try_add(ItemType::IronPlate, 3);

        let to_remove = vec![
            ItemStack::new(ItemType::CopperWire, 3),
            ItemStack::new(ItemType::IronPlate, 1),
        ];
        inv.remove_all(&to_remove);
        assert_eq!(inv.count(ItemType::CopperWire), 2);
        assert_eq!(inv.count(ItemType::IronPlate), 2);
    }
}
