// mining.rs: Resource nodes and mining drill logic.
//
// Resource nodes are grid tiles that contain ore. Mining drills are buildings
// placed on resource nodes that extract items over time into their inventory.

use bevy::prelude::*;

use if_common::item::ItemType;

use crate::inventory::Inventory;
use crate::power::PowerGrid;

/// Component for a resource node entity. Tracks what it yields and how fast.
///
/// `remaining` is the total amount of ore left. When it hits 0, the node
/// is depleted. `yield_per_tick` is how much a drill extracts each tick
/// (before skill bonuses).
#[derive(Component, Clone, Debug)]
pub struct ResourceNode {
    pub resource: ItemType,
    pub yield_per_tick: f32,
    pub remaining: u32,
}

impl ResourceNode {
    pub fn new(resource: ItemType, remaining: u32) -> Self {
        Self {
            resource,
            yield_per_tick: 0.1, // extracts ~6 items/sec at 60 ticks/sec
            remaining,
        }
    }

    pub fn is_depleted(&self) -> bool {
        self.remaining == 0
    }
}

/// Marker component for a mining drill building.
///
/// The drill needs to know which resource node entity it's mining from.
/// We store the Entity reference — this is how ECS handles relationships.
///
/// `extraction_progress` accumulates fractional yields. When it reaches >= 1.0,
/// a whole item is produced. This allows sub-tick extraction rates without
/// losing precision (e.g., 0.1 per tick → 1 item every 10 ticks).
#[derive(Component, Debug)]
pub struct MiningDrill {
    pub target_node: Entity,
    pub extraction_progress: f32,
}

impl MiningDrill {
    pub fn new(target_node: Entity) -> Self {
        Self {
            target_node,
            extraction_progress: 0.0,
        }
    }
}

/// System: mining drills extract items from resource nodes.
///
/// This is a Bevy system — it runs every FixedUpdate tick. Look at the
/// function signature:
///
/// - `drill_q`: all entities that have both a MiningDrill and an Inventory
/// - `node_q`: all entities that have a ResourceNode
///
/// Bevy's ECS scheduler sees these parameter types and knows which component
/// storage to access. It also knows that drill_q borrows Inventory mutably
/// and node_q borrows ResourceNode mutably, so it can parallelize correctly.
pub fn mining_system(
    mut drill_q: Query<(&mut MiningDrill, &mut Inventory)>,
    mut node_q: Query<&mut ResourceNode>,
    power_grid: Res<PowerGrid>,
) {
    for (mut drill, mut inventory) in &mut drill_q {
        // Look up the resource node this drill is mining from.
        // `get_mut` returns Result — the node might not exist (deleted, etc.)
        let Ok(mut node) = node_q.get_mut(drill.target_node) else {
            continue; // node gone — skip this drill
        };

        if node.is_depleted() {
            continue;
        }

        // Accumulate fractional progress, scaled by power availability.
        // At full power (1.0) this is unchanged. At half power (0.5),
        // drills mine at half speed.
        drill.extraction_progress += node.yield_per_tick * power_grid.power_ratio;

        // Extract the integer part in one go — no loop needed.
        // `as u32` truncates toward zero, giving us the whole items.
        let whole_items = (drill.extraction_progress as u32).min(node.remaining);
        if whole_items > 0 {
            let added = inventory.try_add(node.resource, whole_items);
            drill.extraction_progress -= added as f32;
            node.remaining -= added;

            if added < whole_items {
                // Inventory couldn't fit everything — cap progress so it
                // doesn't balloon while blocked (backpressure).
                drill.extraction_progress = drill.extraction_progress.min(1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;

    /// Helper: set up a minimal world with a resource node and a drill mining it.
    fn setup_drill_and_node(
        world: &mut World,
        node_remaining: u32,
        yield_per_tick: f32,
        inv_capacity: u32,
    ) -> (Entity, Entity) {
        // Insert PowerGrid with full power so existing tests behave unchanged.
        world.insert_resource(PowerGrid::default());

        let node_entity = world
            .spawn(ResourceNode {
                resource: ItemType::CopperOre,
                yield_per_tick,
                remaining: node_remaining,
            })
            .id();

        let drill_entity = world
            .spawn((MiningDrill::new(node_entity), Inventory::new(inv_capacity)))
            .id();

        (node_entity, drill_entity)
    }

    #[test]
    fn drill_extracts_over_time() {
        let mut world = World::new();
        let (_, drill_entity) = setup_drill_and_node(&mut world, 100, 0.5, 50);

        // Run the system manually — this is how you unit test Bevy systems.
        // We create a schedule with just our system and run it.
        let mut schedule = Schedule::default();
        schedule.add_systems(mining_system);

        // After 1 tick (yield 0.5), no full item yet
        schedule.run(&mut world);
        let inv = world.get::<Inventory>(drill_entity).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 0);

        // After 2 ticks (yield 1.0), should have 1 item
        schedule.run(&mut world);
        let inv = world.get::<Inventory>(drill_entity).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 1);
    }

    #[test]
    fn drill_stops_when_inventory_full() {
        let mut world = World::new();
        let (node_entity, drill_entity) = setup_drill_and_node(&mut world, 100, 10.0, 5);

        let mut schedule = Schedule::default();
        schedule.add_systems(mining_system);

        // Run enough ticks to fill the inventory
        for _ in 0..20 {
            schedule.run(&mut world);
        }

        let inv = world.get::<Inventory>(drill_entity).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 5); // capped at capacity

        // Node should still have remaining ore (not all consumed)
        let node = world.get::<ResourceNode>(node_entity).unwrap();
        assert_eq!(node.remaining, 95);
    }

    #[test]
    fn drill_stops_when_node_depleted() {
        let mut world = World::new();
        let (node_entity, drill_entity) = setup_drill_and_node(&mut world, 3, 10.0, 50);

        let mut schedule = Schedule::default();
        schedule.add_systems(mining_system);

        for _ in 0..10 {
            schedule.run(&mut world);
        }

        let inv = world.get::<Inventory>(drill_entity).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 3); // only 3 existed

        let node = world.get::<ResourceNode>(node_entity).unwrap();
        assert!(node.is_depleted());
    }
}
