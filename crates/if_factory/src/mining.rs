// mining.rs: Resource nodes and mining drill logic.
//
// Resource nodes are grid tiles that contain ore. Mining drills are buildings
// placed on resource nodes that extract items over time into their inventory.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use if_common::item::ItemType;
use if_common::skill::{PlayerSkills, SkillType};

use crate::inventory::Inventory;
use crate::power::PowerGrid;

/// Component for a resource node entity. Tracks what it yields and how fast.
///
/// `remaining` is the total amount of ore left. When it hits 0, the node
/// is depleted. `yield_per_tick` is how much a drill extracts each tick
/// (before skill bonuses).
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
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

    /// Create a mining drill with restored state (for loading).
    pub fn with_progress(target_node: Entity, extraction_progress: f32) -> Self {
        Self {
            target_node,
            extraction_progress,
        }
    }

    /// Get the extraction progress (for save/load).
    pub fn extraction_progress(&self) -> f32 {
        self.extraction_progress
    }
}

/// XP granted per item successfully mined.
const MINING_XP_PER_ITEM: u32 = 5;

/// System: mining drills extract items from resource nodes.
///
/// Extraction rate is scaled by power availability and the player's
/// Mining skill bonus. XP is granted for each item successfully extracted.
pub fn mining_system(
    mut drill_q: Query<(&mut MiningDrill, &mut Inventory)>,
    mut node_q: Query<&mut ResourceNode>,
    power_grid: Res<PowerGrid>,
    mut player_skills: ResMut<PlayerSkills>,
) {
    let mining_bonus = player_skills.get_bonus(SkillType::Mining);

    for (mut drill, mut inventory) in &mut drill_q {
        // Look up the resource node this drill is mining from.
        // `get_mut` returns Result — the node might not exist (deleted, etc.)
        let Ok(mut node) = node_q.get_mut(drill.target_node) else {
            continue; // node gone — skip this drill
        };

        if node.is_depleted() {
            continue;
        }

        // Accumulate fractional progress, scaled by power availability and
        // the player's Mining skill bonus.
        drill.extraction_progress += node.yield_per_tick * power_grid.power_ratio * mining_bonus;

        // Extract the integer part in one go — no loop needed.
        // `as u32` truncates toward zero, giving us the whole items.
        let whole_items = (drill.extraction_progress as u32).min(node.remaining);
        if whole_items > 0 {
            let added = inventory.try_add(node.resource, whole_items);
            drill.extraction_progress -= added as f32;
            node.remaining -= added;

            // Grant Mining XP for items successfully extracted.
            if added > 0 {
                player_skills.add_xp(SkillType::Mining, added * MINING_XP_PER_ITEM);
            }

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
        // Insert PlayerSkills with default (no XP) so existing tests work.
        world.insert_resource(PlayerSkills::default());

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

    #[test]
    fn mining_grants_xp() {
        let mut world = World::new();
        // yield 10.0 per tick => extracts 10 items on first tick
        let (_, _) = setup_drill_and_node(&mut world, 100, 10.0, 50);

        let mut schedule = Schedule::default();
        schedule.add_systems(mining_system);
        schedule.run(&mut world);

        let skills = world.resource::<PlayerSkills>();
        // 10 items * 5 XP each = 50 XP
        assert_eq!(skills.get_level(SkillType::Mining).xp(), 50);
    }

    #[test]
    fn mining_skill_bonus_speeds_extraction() {
        let mut world = World::new();
        world.insert_resource(PowerGrid::default());
        // Pre-level the player's mining skill to level 4 (bonus = 3.0)
        let mut skills = PlayerSkills::default();
        skills.add_xp(SkillType::Mining, 400); // 4 levels
        world.insert_resource(skills);

        let node_entity = world
            .spawn(ResourceNode {
                resource: ItemType::CopperOre,
                yield_per_tick: 0.5,
                remaining: 100,
            })
            .id();

        let drill_entity = world
            .spawn((MiningDrill::new(node_entity), Inventory::new(50)))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(mining_system);

        // With bonus 3.0, effective yield = 0.5 * 3.0 = 1.5 per tick.
        // After 1 tick: progress 1.5, extract 1 item, progress 0.5
        schedule.run(&mut world);
        let inv = world.get::<Inventory>(drill_entity).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 1);

        // After 2 ticks: progress 0.5 + 1.5 = 2.0, extract 2 items
        schedule.run(&mut world);
        let inv = world.get::<Inventory>(drill_entity).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 3);
    }
}
