// save.rs: Serializable game state for save/load.
//
// Entity references (Bevy's Entity type) cannot be serialized directly.
// Instead, we assign each entity a u32 "save ID" (its index in a Vec)
// and replace Entity references with save IDs. On load, we reconstruct
// the Entity references from the save ID mapping.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::GridPosition;
use crate::TileType;
use crate::item::ItemType;
use crate::recipe::Recipe;
use crate::skill::{SkillLevel, SkillType};

/// The complete serializable game state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveData {
    /// Grid dimensions and tile data.
    pub grid: SaveGrid,
    /// Player skills.
    pub skills: HashMap<SkillType, SkillLevel>,
    /// All resource node entities.
    pub resource_nodes: Vec<SaveResourceNode>,
    /// All building entities (drills, machines, generators).
    pub buildings: Vec<SaveBuilding>,
    /// All transport line entities.
    pub transport_lines: Vec<SaveTransportLine>,
}

/// Serializable grid state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveGrid {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<TileType>,
}

/// Serializable resource node.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveResourceNode {
    /// Save ID for this resource node entity.
    pub save_id: u32,
    pub position: GridPosition,
    pub resource: ItemType,
    pub yield_per_tick: f32,
    pub remaining: u32,
}

/// The type-specific data for a building.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SaveBuildingKind {
    MiningDrill {
        /// Save ID of the target ResourceNode entity.
        target_node_id: u32,
        extraction_progress: f32,
    },
    Smelter {
        recipe: Recipe,
        processing_ticks_remaining: u32,
        tick_progress: f32,
        is_processing: bool,
    },
    Assembler {
        recipe: Recipe,
        processing_ticks_remaining: u32,
        tick_progress: f32,
        is_processing: bool,
    },
    Generator {
        output: f32,
    },
}

/// Serializable building entity.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveBuilding {
    /// Save ID for this building entity.
    pub save_id: u32,
    pub position: GridPosition,
    pub kind: SaveBuildingKind,
    /// Inventory contents (item type -> quantity) and capacity.
    /// None for buildings without inventories (e.g., generators).
    pub inventory: Option<SaveInventory>,
    /// Power consumer demand, if applicable.
    pub power_demand: Option<f32>,
}

/// Serializable inventory.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveInventory {
    pub items: HashMap<ItemType, u32>,
    pub capacity: u32,
}

/// Serializable transit item on a transport line.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveTransitItem {
    pub item: ItemType,
    pub quantity: u32,
    pub ticks_remaining: u32,
}

/// Serializable transport line.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveTransportLine {
    /// Save ID of the source building entity.
    pub source_id: u32,
    /// Save ID of the destination building entity.
    pub destination_id: u32,
    pub transit_ticks: u32,
    pub capacity: u32,
    pub item_filter: Option<ItemType>,
    pub items_in_transit: Vec<SaveTransitItem>,
}

impl SaveData {
    /// Serialize the save data to bytes using bincode.
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize save data from bytes using bincode.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemStack, ItemType};

    fn sample_save_data() -> SaveData {
        let mut skills = HashMap::new();
        skills.insert(SkillType::Mining, SkillLevel::new(250));
        skills.insert(SkillType::Smelting, SkillLevel::new(100));

        let mut inv_items = HashMap::new();
        inv_items.insert(ItemType::CopperOre, 15);
        inv_items.insert(ItemType::IronOre, 8);

        SaveData {
            grid: SaveGrid {
                width: 32,
                height: 32,
                tiles: vec![TileType::Empty; 32 * 32],
            },
            skills,
            resource_nodes: vec![
                SaveResourceNode {
                    save_id: 0,
                    position: GridPosition::new(5, 5),
                    resource: ItemType::CopperOre,
                    yield_per_tick: 0.1,
                    remaining: 9500,
                },
                SaveResourceNode {
                    save_id: 1,
                    position: GridPosition::new(10, 10),
                    resource: ItemType::IronOre,
                    yield_per_tick: 0.1,
                    remaining: 10000,
                },
            ],
            buildings: vec![
                SaveBuilding {
                    save_id: 2,
                    position: GridPosition::new(5, 5),
                    kind: SaveBuildingKind::MiningDrill {
                        target_node_id: 0,
                        extraction_progress: 0.3,
                    },
                    inventory: Some(SaveInventory {
                        items: inv_items,
                        capacity: 50,
                    }),
                    power_demand: Some(10.0),
                },
                SaveBuilding {
                    save_id: 3,
                    position: GridPosition::new(7, 7),
                    kind: SaveBuildingKind::Smelter {
                        recipe: Recipe::new(
                            "Smelt Copper",
                            vec![ItemStack::new(ItemType::CopperOre, 1)],
                            vec![ItemStack::new(ItemType::CopperIngot, 1)],
                            60,
                        ),
                        processing_ticks_remaining: 30,
                        tick_progress: 0.5,
                        is_processing: true,
                    },
                    inventory: Some(SaveInventory {
                        items: HashMap::new(),
                        capacity: 50,
                    }),
                    power_demand: Some(15.0),
                },
                SaveBuilding {
                    save_id: 4,
                    position: GridPosition::new(12, 12),
                    kind: SaveBuildingKind::Generator { output: 50.0 },
                    inventory: None,
                    power_demand: None,
                },
            ],
            transport_lines: vec![SaveTransportLine {
                source_id: 2,
                destination_id: 3,
                transit_ticks: 15,
                capacity: 20,
                item_filter: Some(ItemType::CopperOre),
                items_in_transit: vec![SaveTransitItem {
                    item: ItemType::CopperOre,
                    quantity: 3,
                    ticks_remaining: 7,
                }],
            }],
        }
    }

    #[test]
    fn round_trip_serialization() {
        let original = sample_save_data();
        let bytes = original.to_bytes().expect("serialization should succeed");
        let restored = SaveData::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(original, restored);
    }

    #[test]
    fn empty_save_data_round_trip() {
        let empty = SaveData {
            grid: SaveGrid {
                width: 4,
                height: 4,
                tiles: vec![TileType::Empty; 16],
            },
            skills: HashMap::new(),
            resource_nodes: vec![],
            buildings: vec![],
            transport_lines: vec![],
        };
        let bytes = empty.to_bytes().expect("serialization should succeed");
        let restored = SaveData::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(empty, restored);
    }

    #[test]
    fn all_tile_types_serialize() {
        let data = SaveData {
            grid: SaveGrid {
                width: 2,
                height: 2,
                tiles: vec![
                    TileType::Empty,
                    TileType::Rock,
                    TileType::CopperDeposit,
                    TileType::IronDeposit,
                ],
            },
            skills: HashMap::new(),
            resource_nodes: vec![],
            buildings: vec![],
            transport_lines: vec![],
        };
        let bytes = data.to_bytes().unwrap();
        let restored = SaveData::from_bytes(&bytes).unwrap();
        assert_eq!(data.grid.tiles, restored.grid.tiles);
    }

    #[test]
    fn all_item_types_serialize() {
        let all_items = vec![
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
        let mut items_map = HashMap::new();
        for (i, item) in all_items.iter().enumerate() {
            items_map.insert(*item, (i as u32) + 1);
        }
        let data = SaveData {
            grid: SaveGrid {
                width: 1,
                height: 1,
                tiles: vec![TileType::Empty],
            },
            skills: HashMap::new(),
            resource_nodes: vec![],
            buildings: vec![SaveBuilding {
                save_id: 0,
                position: GridPosition::new(0, 0),
                kind: SaveBuildingKind::MiningDrill {
                    target_node_id: 0,
                    extraction_progress: 0.0,
                },
                inventory: Some(SaveInventory {
                    items: items_map.clone(),
                    capacity: 1000,
                }),
                power_demand: None,
            }],
            transport_lines: vec![],
        };
        let bytes = data.to_bytes().unwrap();
        let restored = SaveData::from_bytes(&bytes).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn all_skill_types_serialize() {
        let mut skills = HashMap::new();
        skills.insert(SkillType::Mining, SkillLevel::new(100));
        skills.insert(SkillType::Smelting, SkillLevel::new(200));
        skills.insert(SkillType::Fabrication, SkillLevel::new(300));
        skills.insert(SkillType::Logistics, SkillLevel::new(400));

        let data = SaveData {
            grid: SaveGrid {
                width: 1,
                height: 1,
                tiles: vec![TileType::Empty],
            },
            skills,
            resource_nodes: vec![],
            buildings: vec![],
            transport_lines: vec![],
        };
        let bytes = data.to_bytes().unwrap();
        let restored = SaveData::from_bytes(&bytes).unwrap();
        assert_eq!(data.skills, restored.skills);
    }

    #[test]
    fn building_kinds_round_trip() {
        let assembler_recipe = Recipe::new(
            "Assemble Basic Circuit",
            vec![
                ItemStack::new(ItemType::CopperWire, 3),
                ItemStack::new(ItemType::IronPlate, 1),
            ],
            vec![ItemStack::new(ItemType::BasicCircuit, 1)],
            120,
        );
        let data = SaveData {
            grid: SaveGrid {
                width: 1,
                height: 1,
                tiles: vec![TileType::Empty],
            },
            skills: HashMap::new(),
            resource_nodes: vec![],
            buildings: vec![
                SaveBuilding {
                    save_id: 0,
                    position: GridPosition::new(0, 0),
                    kind: SaveBuildingKind::Assembler {
                        recipe: assembler_recipe,
                        processing_ticks_remaining: 55,
                        tick_progress: 0.7,
                        is_processing: true,
                    },
                    inventory: Some(SaveInventory {
                        items: HashMap::new(),
                        capacity: 50,
                    }),
                    power_demand: Some(20.0),
                },
                SaveBuilding {
                    save_id: 1,
                    position: GridPosition::new(1, 0),
                    kind: SaveBuildingKind::Generator { output: 50.0 },
                    inventory: None,
                    power_demand: None,
                },
            ],
            transport_lines: vec![],
        };
        let bytes = data.to_bytes().unwrap();
        let restored = SaveData::from_bytes(&bytes).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn transport_with_multiple_transit_items() {
        let data = SaveData {
            grid: SaveGrid {
                width: 1,
                height: 1,
                tiles: vec![TileType::Empty],
            },
            skills: HashMap::new(),
            resource_nodes: vec![],
            buildings: vec![],
            transport_lines: vec![SaveTransportLine {
                source_id: 0,
                destination_id: 1,
                transit_ticks: 30,
                capacity: 50,
                item_filter: None,
                items_in_transit: vec![
                    SaveTransitItem {
                        item: ItemType::CopperOre,
                        quantity: 5,
                        ticks_remaining: 10,
                    },
                    SaveTransitItem {
                        item: ItemType::IronOre,
                        quantity: 3,
                        ticks_remaining: 20,
                    },
                    SaveTransitItem {
                        item: ItemType::CopperIngot,
                        quantity: 1,
                        ticks_remaining: 1,
                    },
                ],
            }],
        };
        let bytes = data.to_bytes().unwrap();
        let restored = SaveData::from_bytes(&bytes).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn invalid_bytes_return_error() {
        let result = SaveData::from_bytes(&[0xFF, 0x00, 0x12, 0x34]);
        assert!(result.is_err());
    }
}
