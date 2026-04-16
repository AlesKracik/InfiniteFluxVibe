// building.rs: Building types and shared building components.
//
// In ECS, a "building" is just an entity with certain components attached.
// A mining drill is an entity with: GridPosition + MiningDrill + Inventory.
// A transport line is: GridPosition + TransportLine.
// A machine is: GridPosition + Machine + Inventory.
//
// This file defines the shared building enum and placement logic.

use bevy::prelude::*;
use if_common::GridPosition;
use serde::{Deserialize, Serialize};

/// Enum of all building types the player can place.
///
/// This serves double duty:
/// 1. As a marker for what kind of building an entity is
/// 2. As the "selected building" in the placement system
///
/// Each variant will correspond to a specific set of ECS components
/// that get attached when the building is spawned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingType {
    MiningDrill,
    TransportLine,
    Smelter,
    Assembler,
    Generator,
}

impl std::fmt::Display for BuildingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildingType::MiningDrill => write!(f, "Mining Drill"),
            BuildingType::TransportLine => write!(f, "Transport Line"),
            BuildingType::Smelter => write!(f, "Smelter"),
            BuildingType::Assembler => write!(f, "Assembler"),
            BuildingType::Generator => write!(f, "Generator"),
        }
    }
}

/// Component that marks an entity as a placed building.
/// Stores what type it is so we can query all buildings generically.
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Building {
    pub building_type: BuildingType,
}

/// Resource tracking which entities occupy which grid cells.
/// This prevents placing two buildings on the same cell and allows
/// spatial lookups (e.g., "what building is at (5, 3)?").
///
/// We use a HashMap because the grid is sparse — most cells are empty.
/// A flat array would waste memory for large grids with few buildings.
#[derive(Resource, Default)]
pub struct BuildingMap {
    map: std::collections::HashMap<GridPosition, Entity>,
}

impl BuildingMap {
    /// Check if a grid cell is occupied.
    pub fn is_occupied(&self, pos: &GridPosition) -> bool {
        self.map.contains_key(pos)
    }

    /// Get the entity at a grid position, if any.
    pub fn get(&self, pos: &GridPosition) -> Option<Entity> {
        self.map.get(pos).copied()
    }

    /// Register a building entity at a position.
    /// Returns false if the cell is already occupied.
    pub fn insert(&mut self, pos: GridPosition, entity: Entity) -> bool {
        if self.is_occupied(&pos) {
            false
        } else {
            self.map.insert(pos, entity);
            true
        }
    }

    /// Remove a building from a position. Returns the entity if one was there.
    pub fn remove(&mut self, pos: &GridPosition) -> Option<Entity> {
        self.map.remove(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;

    /// Helper: spawn a dummy entity in a World so we get a real Entity handle.
    fn dummy_entity(world: &mut World) -> Entity {
        world.spawn_empty().id()
    }

    #[test]
    fn building_map_insert_and_lookup() {
        let mut world = World::new();
        let mut map = BuildingMap::default();
        let pos = GridPosition::new(3, 5);
        let entity = dummy_entity(&mut world);

        assert!(!map.is_occupied(&pos));
        assert!(map.insert(pos, entity));
        assert!(map.is_occupied(&pos));
        assert_eq!(map.get(&pos), Some(entity));
    }

    #[test]
    fn building_map_rejects_duplicate() {
        let mut world = World::new();
        let mut map = BuildingMap::default();
        let pos = GridPosition::new(1, 1);
        let e1 = dummy_entity(&mut world);
        let e2 = dummy_entity(&mut world);

        assert!(map.insert(pos, e1));
        assert!(!map.insert(pos, e2)); // same cell — rejected
        assert_eq!(map.get(&pos), Some(e1)); // original still there
    }

    #[test]
    fn building_map_remove() {
        let mut world = World::new();
        let mut map = BuildingMap::default();
        let pos = GridPosition::new(2, 2);
        let entity = dummy_entity(&mut world);

        map.insert(pos, entity);
        assert_eq!(map.remove(&pos), Some(entity));
        assert!(!map.is_occupied(&pos));
        assert_eq!(map.remove(&pos), None); // already removed
    }
}
