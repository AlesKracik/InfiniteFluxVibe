// world_setup.rs: Spawns factory-layer entities from the world grid.
//
// The Grid (in if_world) defines tile types. This module reads those tiles
// and spawns corresponding ECS entities from if_factory (e.g., ResourceNode
// entities for deposit tiles). This bridges the world and factory layers.

use bevy::prelude::*;

use if_common::item::ItemType;
use if_common::{GridPosition, TileType};
use if_factory::mining::ResourceNode;
use if_world::grid::Grid;

/// Startup system: scan the grid for resource deposits and spawn
/// ResourceNode entities so mining drills can target them.
pub fn spawn_resource_nodes(mut commands: Commands, grid: Res<Grid>) {
    for y in 0..grid.height {
        for x in 0..grid.width {
            let tile = grid.get(x, y).unwrap();
            let resource = match tile {
                TileType::CopperDeposit => Some(ItemType::CopperOre),
                TileType::IronDeposit => Some(ItemType::IronOre),
                _ => None,
            };

            if let Some(resource) = resource {
                commands.spawn((GridPosition::new(x, y), ResourceNode::new(resource, 10_000)));
            }
        }
    }
}
