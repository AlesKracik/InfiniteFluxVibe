// transport.rs: Transport lines move items between buildings.
//
// A transport line connects a source building to a destination building.
// Each tick, it tries to pull items from the source's inventory and push
// them into the destination's inventory. The items take time to travel
// (transit_ticks), simulating physical distance.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use if_common::item::ItemType;

use crate::inventory::Inventory;

/// A single item in transit on the transport line.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransitItem {
    pub item: ItemType,
    pub quantity: u32,
    pub ticks_remaining: u32,
}

/// Component for a transport line entity.
///
/// The transport line doesn't occupy a sequence of grid cells (that's a
/// visual concern for the renderer). Logically it's just a connection:
/// "pull items from entity A, deliver to entity B after N ticks."
///
/// `item_filter`: if Some, only transport this item type. If None, transport
/// anything. This lets players set up dedicated lines for specific materials.
#[derive(Component, Debug)]
pub struct TransportLine {
    pub source: Entity,
    pub destination: Entity,
    pub transit_ticks: u32,
    pub capacity: u32,
    pub item_filter: Option<ItemType>,
    items_in_transit: Vec<TransitItem>,
}

impl TransportLine {
    pub fn new(source: Entity, destination: Entity, transit_ticks: u32, capacity: u32) -> Self {
        Self {
            source,
            destination,
            transit_ticks,
            capacity,
            item_filter: None,
            items_in_transit: Vec::new(),
        }
    }

    /// Get the items currently in transit (for save/load).
    pub fn items_in_transit(&self) -> &[TransitItem] {
        &self.items_in_transit
    }

    /// Create a transport line with pre-existing items in transit (for loading).
    pub fn with_transit_items(
        source: Entity,
        destination: Entity,
        transit_ticks: u32,
        capacity: u32,
        item_filter: Option<ItemType>,
        items_in_transit: Vec<TransitItem>,
    ) -> Self {
        Self {
            source,
            destination,
            transit_ticks,
            capacity,
            item_filter,
            items_in_transit,
        }
    }

    /// How many items are currently in transit.
    pub fn items_in_transit_count(&self) -> u32 {
        self.items_in_transit.iter().map(|t| t.quantity).sum()
    }

    /// How much capacity is available for new items.
    pub fn space_available(&self) -> u32 {
        self.capacity.saturating_sub(self.items_in_transit_count())
    }
}

/// System: transport lines pull items from source, deliver to destination.
///
/// This runs after mining_system (via .chain()) so freshly mined items
/// are available to transport immediately.
///
/// The query has two parts:
/// - `line_q`: the transport line entities
/// - `inv_q`: ALL entities with inventories (sources and destinations)
///
/// We can't query source and destination inventories separately because
/// Bevy doesn't allow overlapping mutable queries. Instead we query all
/// inventories and use `get_mut` to access specific ones.
pub fn transport_system(mut line_q: Query<&mut TransportLine>, mut inv_q: Query<&mut Inventory>) {
    for mut line in &mut line_q {
        // --- Deliver arrived items ---
        // Tick down all items in transit. Deliver any that have arrived.
        let mut delivered = Vec::new();
        for (i, transit) in line.items_in_transit.iter_mut().enumerate() {
            if transit.ticks_remaining > 0 {
                transit.ticks_remaining -= 1;
            }
            if transit.ticks_remaining == 0 {
                delivered.push(i);
            }
        }

        // Deliver in reverse order so indices stay valid as we remove.
        for &i in delivered.iter().rev() {
            let transit = &line.items_in_transit[i];
            let item = transit.item;
            let qty = transit.quantity;

            if let Ok(mut dest_inv) = inv_q.get_mut(line.destination) {
                let added = dest_inv.try_add(item, qty);
                if added == qty {
                    line.items_in_transit.remove(i);
                } else if added > 0 {
                    // Partial delivery — reduce the in-transit amount.
                    line.items_in_transit[i].quantity -= added;
                }
                // If added == 0, item stays in transit (destination full).
                // It will retry next tick.
            }
        }

        // --- Pull new items from source ---
        let space = line.space_available();
        if space == 0 {
            continue;
        }

        if let Ok(mut source_inv) = inv_q.get_mut(line.source) {
            // Determine what to pull
            let items_to_check: Vec<ItemType> = if let Some(filter) = line.item_filter {
                vec![filter]
            } else {
                source_inv.contents().iter().map(|s| s.item).collect()
            };

            // Read transit_ticks before the mutable borrow from push().
            // The borrow checker won't let us read `line.transit_ticks` inside
            // the same expression that mutably borrows `line.items_in_transit`.
            // By copying the value first, we avoid the overlapping borrow.
            let transit_ticks = line.transit_ticks;

            for item in items_to_check {
                let current_space = line.space_available();
                if current_space == 0 {
                    break;
                }
                let available = source_inv.count(item);
                if available > 0 {
                    let to_take = available.min(current_space);
                    let removed = source_inv.try_remove(item, to_take);
                    if removed > 0 {
                        line.items_in_transit.push(TransitItem {
                            item,
                            quantity: removed,
                            ticks_remaining: transit_ticks,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;
    use if_common::item::ItemType;

    #[test]
    fn transport_moves_items() {
        let mut world = World::new();

        let source = world.spawn(Inventory::new(100)).id();
        let dest = world.spawn(Inventory::new(100)).id();
        world.spawn(TransportLine::new(source, dest, 2, 50));

        // Add items to source
        world
            .get_mut::<Inventory>(source)
            .unwrap()
            .try_add(ItemType::CopperOre, 5);

        let mut schedule = Schedule::default();
        schedule.add_systems(transport_system);

        // Tick 1: items pulled from source, 2 ticks transit
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<Inventory>(source)
                .unwrap()
                .count(ItemType::CopperOre),
            0
        );
        assert_eq!(
            world
                .get::<Inventory>(dest)
                .unwrap()
                .count(ItemType::CopperOre),
            0
        );

        // Tick 2: still in transit (1 tick remaining)
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<Inventory>(dest)
                .unwrap()
                .count(ItemType::CopperOre),
            0
        );

        // Tick 3: arrived!
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<Inventory>(dest)
                .unwrap()
                .count(ItemType::CopperOre),
            5
        );
    }

    #[test]
    fn transport_respects_destination_capacity() {
        let mut world = World::new();

        let source = world.spawn(Inventory::new(100)).id();
        let dest = world.spawn(Inventory::new(3)).id(); // tiny destination
        world.spawn(TransportLine::new(source, dest, 1, 50));

        world
            .get_mut::<Inventory>(source)
            .unwrap()
            .try_add(ItemType::IronOre, 10);

        let mut schedule = Schedule::default();
        schedule.add_systems(transport_system);

        // Tick 1: pull items from source
        schedule.run(&mut world);
        // Tick 2: try to deliver — only 3 fit
        schedule.run(&mut world);

        assert_eq!(
            world
                .get::<Inventory>(dest)
                .unwrap()
                .count(ItemType::IronOre),
            3
        );
    }

    #[test]
    fn transport_respects_line_capacity() {
        let mut world = World::new();

        let source = world.spawn(Inventory::new(100)).id();
        let dest = world.spawn(Inventory::new(100)).id();
        world.spawn(TransportLine::new(source, dest, 5, 4)); // line capacity = 4

        world
            .get_mut::<Inventory>(source)
            .unwrap()
            .try_add(ItemType::CopperOre, 10);

        let mut schedule = Schedule::default();
        schedule.add_systems(transport_system);

        schedule.run(&mut world);

        // Only 4 should be pulled (line capacity), 6 remain in source
        assert_eq!(
            world
                .get::<Inventory>(source)
                .unwrap()
                .count(ItemType::CopperOre),
            6
        );
    }
}
