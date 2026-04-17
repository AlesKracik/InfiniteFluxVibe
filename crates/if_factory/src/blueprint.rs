// blueprint.rs: Blueprint data model for copying and pasting factory layouts.
//
// A blueprint captures a rectangular region of buildings as relative offsets
// from an origin point. Blueprints can be serialized/deserialized for
// persistent storage.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::building::BuildingType;

/// A saved factory layout that can be stamped onto the grid.
///
/// Buildings are stored as offsets from (0,0), so the blueprint can be
/// placed at any grid position by adding the target position to each offset.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Blueprint {
    pub name: String,
    /// Buildings relative to an origin (0,0) position.
    pub entries: Vec<BlueprintEntry>,
}

/// A single building within a blueprint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BlueprintEntry {
    /// Offset from the blueprint origin.
    pub offset: (i32, i32),
    pub building_type: BuildingType,
    /// For machines (Smelter, Assembler): which recipe name to use on paste.
    pub recipe_name: Option<String>,
}

impl Blueprint {
    /// Create a new empty blueprint with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Vec::new(),
        }
    }

    /// Create a blueprint from a list of absolute grid positions and building data.
    ///
    /// The origin is the minimum (x, y) corner of the bounding box containing
    /// all buildings. Offsets are computed relative to that origin.
    ///
    /// `buildings` is a slice of (x, y, building_type, optional recipe_name).
    pub fn from_buildings(
        name: impl Into<String>,
        buildings: &[(u32, u32, BuildingType, Option<String>)],
    ) -> Self {
        if buildings.is_empty() {
            return Self::new(name);
        }

        let min_x = buildings.iter().map(|(x, _, _, _)| *x).min().unwrap();
        let min_y = buildings.iter().map(|(_, y, _, _)| *y).min().unwrap();

        let entries = buildings
            .iter()
            .map(|(x, y, bt, recipe)| BlueprintEntry {
                offset: (*x as i32 - min_x as i32, *y as i32 - min_y as i32),
                building_type: *bt,
                recipe_name: recipe.clone(),
            })
            .collect();

        Self {
            name: name.into(),
            entries,
        }
    }

    /// Serialize the blueprint to bytes using bincode.
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize a blueprint from bytes using bincode.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// Resource holding all saved blueprints.
#[derive(Clone, Debug, Default, Resource, Serialize, Deserialize)]
pub struct Blueprints {
    pub blueprints: Vec<Blueprint>,
}

impl Blueprints {
    /// Serialize all blueprints to bytes using bincode.
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize blueprints from bytes using bincode.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_blueprint() {
        let bp = Blueprint::new("empty");
        assert_eq!(bp.name, "empty");
        assert!(bp.entries.is_empty());
    }

    #[test]
    fn from_buildings_computes_offsets() {
        let buildings = vec![
            (5, 5, BuildingType::MiningDrill, None),
            (
                7,
                5,
                BuildingType::Smelter,
                Some("Smelt Copper".to_string()),
            ),
            (5, 7, BuildingType::Generator, None),
        ];

        let bp = Blueprint::from_buildings("test", &buildings);
        assert_eq!(bp.entries.len(), 3);

        // Origin should be (5, 5), so offsets are relative to that
        assert_eq!(bp.entries[0].offset, (0, 0));
        assert_eq!(bp.entries[0].building_type, BuildingType::MiningDrill);

        assert_eq!(bp.entries[1].offset, (2, 0));
        assert_eq!(bp.entries[1].building_type, BuildingType::Smelter);
        assert_eq!(bp.entries[1].recipe_name, Some("Smelt Copper".to_string()));

        assert_eq!(bp.entries[2].offset, (0, 2));
        assert_eq!(bp.entries[2].building_type, BuildingType::Generator);
    }

    #[test]
    fn from_buildings_with_single_building() {
        let buildings = vec![(10, 20, BuildingType::Assembler, Some("Test".to_string()))];

        let bp = Blueprint::from_buildings("single", &buildings);
        assert_eq!(bp.entries.len(), 1);
        assert_eq!(bp.entries[0].offset, (0, 0));
    }

    #[test]
    fn from_empty_buildings() {
        let bp = Blueprint::from_buildings("none", &[]);
        assert!(bp.entries.is_empty());
    }

    #[test]
    fn serialization_round_trip() {
        let buildings = vec![
            (3, 4, BuildingType::MiningDrill, None),
            (5, 6, BuildingType::Smelter, Some("Smelt Iron".to_string())),
            (
                7,
                8,
                BuildingType::Assembler,
                Some("Assemble Basic Circuit".to_string()),
            ),
            (9, 4, BuildingType::Generator, None),
        ];

        let bp = Blueprint::from_buildings("roundtrip test", &buildings);
        let bytes = bp.to_bytes().expect("serialization should succeed");
        let restored = Blueprint::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(bp, restored);
    }

    #[test]
    fn blueprints_collection_round_trip() {
        let bp1 = Blueprint::from_buildings("bp1", &[(0, 0, BuildingType::MiningDrill, None)]);
        let bp2 = Blueprint::from_buildings(
            "bp2",
            &[
                (
                    1,
                    1,
                    BuildingType::Smelter,
                    Some("Smelt Copper".to_string()),
                ),
                (2, 2, BuildingType::Generator, None),
            ],
        );

        let collection = Blueprints {
            blueprints: vec![bp1.clone(), bp2.clone()],
        };

        let bytes = collection.to_bytes().expect("serialization should succeed");
        let restored = Blueprints::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(restored.blueprints.len(), 2);
        assert_eq!(restored.blueprints[0], bp1);
        assert_eq!(restored.blueprints[1], bp2);
    }

    #[test]
    fn invalid_bytes_return_error() {
        let result = Blueprint::from_bytes(&[0xFF, 0x00, 0x12]);
        assert!(result.is_err());
    }

    #[test]
    fn offset_calculation_with_non_zero_origin() {
        // Buildings at (10,20), (12,22), (11,21)
        // Min corner: (10, 20)
        // Offsets: (0,0), (2,2), (1,1)
        let buildings = vec![
            (10, 20, BuildingType::MiningDrill, None),
            (12, 22, BuildingType::Smelter, None),
            (11, 21, BuildingType::Generator, None),
        ];

        let bp = Blueprint::from_buildings("offset test", &buildings);
        assert_eq!(bp.entries[0].offset, (0, 0));
        assert_eq!(bp.entries[1].offset, (2, 2));
        assert_eq!(bp.entries[2].offset, (1, 1));
    }
}
