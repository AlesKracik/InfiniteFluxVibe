// recipe.rs: Recipes define how items are transformed.
//
// A recipe says: "give me these inputs, wait this long, and I'll produce
// these outputs." Machines reference recipes to know what they can make.

use crate::item::{ItemStack, ItemType};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A recipe: inputs consumed → outputs produced, with a processing duration.
///
/// Recipes are data, not behavior — they describe *what* a transformation does,
/// not *how* (that's the machine's job). This separation is key for data-driven
/// design: later we'll load recipes from RON/JSON files instead of hardcoding.
///
/// `processing_ticks` is in simulation ticks (not real seconds) so the sim
/// runs at a fixed rate regardless of frame rate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub inputs: Vec<ItemStack>,
    pub outputs: Vec<ItemStack>,
    pub processing_ticks: u32,
}

impl Recipe {
    pub fn new(
        name: impl Into<String>,
        inputs: Vec<ItemStack>,
        outputs: Vec<ItemStack>,
        processing_ticks: u32,
    ) -> Self {
        Self {
            name: name.into(),
            inputs,
            outputs,
            processing_ticks,
        }
    }
}

impl fmt::Display for Recipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inputs: Vec<String> = self.inputs.iter().map(|s| format!("{s}")).collect();
        let outputs: Vec<String> = self.outputs.iter().map(|s| format!("{s}")).collect();
        write!(
            f,
            "{}: {} → {} ({} ticks)",
            self.name,
            inputs.join(" + "),
            outputs.join(" + "),
            self.processing_ticks
        )
    }
}

/// All the recipes available in the game (P1.9).
///
/// This function returns the starting set of recipes. The chain is:
///   Ore → Ingot → Plate → Component → Ship Part
///
/// Later this will be loaded from a data file, but hardcoding is fine for now
/// (Principle 7: explicit over magical — understand the data before abstracting).
pub fn starter_recipes() -> Vec<Recipe> {
    vec![
        // --- Smelting (ore → ingot) ---
        Recipe::new(
            "Smelt Copper",
            vec![ItemStack::new(ItemType::CopperOre, 1)],
            vec![ItemStack::new(ItemType::CopperIngot, 1)],
            60, // 1 second at 60 ticks/sec
        ),
        Recipe::new(
            "Smelt Iron",
            vec![ItemStack::new(ItemType::IronOre, 1)],
            vec![ItemStack::new(ItemType::IronIngot, 1)],
            90, // slower — iron is harder to process
        ),
        // --- Pressing (ingot → plate) ---
        Recipe::new(
            "Press Copper Plate",
            vec![ItemStack::new(ItemType::CopperIngot, 1)],
            vec![ItemStack::new(ItemType::CopperPlate, 1)],
            40,
        ),
        Recipe::new(
            "Press Iron Plate",
            vec![ItemStack::new(ItemType::IronIngot, 1)],
            vec![ItemStack::new(ItemType::IronPlate, 1)],
            60,
        ),
        // --- Fabrication (plate → component) ---
        Recipe::new(
            "Draw Copper Wire",
            vec![ItemStack::new(ItemType::CopperPlate, 1)],
            vec![ItemStack::new(ItemType::CopperWire, 2)],
            30,
        ),
        Recipe::new(
            "Assemble Basic Circuit",
            vec![
                ItemStack::new(ItemType::CopperWire, 3),
                ItemStack::new(ItemType::IronPlate, 1),
            ],
            vec![ItemStack::new(ItemType::BasicCircuit, 1)],
            120,
        ),
        // --- Ship component (multi-input final product) ---
        Recipe::new(
            "Forge Hull Plate",
            vec![
                ItemStack::new(ItemType::IronPlate, 3),
                ItemStack::new(ItemType::CopperPlate, 1),
            ],
            vec![ItemStack::new(ItemType::HullPlate, 1)],
            180,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_recipes_are_valid() {
        let recipes = starter_recipes();
        assert!(!recipes.is_empty());
        for recipe in &recipes {
            // Every recipe must have at least one input and one output
            assert!(!recipe.inputs.is_empty(), "{} has no inputs", recipe.name);
            assert!(!recipe.outputs.is_empty(), "{} has no outputs", recipe.name);
            // Processing time must be positive
            assert!(
                recipe.processing_ticks > 0,
                "{} has zero processing time",
                recipe.name
            );
            // All quantities must be positive
            for input in &recipe.inputs {
                assert!(
                    input.quantity > 0,
                    "{} has zero-quantity input",
                    recipe.name
                );
            }
            for output in &recipe.outputs {
                assert!(
                    output.quantity > 0,
                    "{} has zero-quantity output",
                    recipe.name
                );
            }
        }
    }

    #[test]
    fn recipe_display() {
        let recipe = Recipe::new(
            "Smelt Copper",
            vec![ItemStack::new(ItemType::CopperOre, 1)],
            vec![ItemStack::new(ItemType::CopperIngot, 1)],
            60,
        );
        assert_eq!(
            format!("{recipe}"),
            "Smelt Copper: 1x Copper Ore → 1x Copper Ingot (60 ticks)"
        );
    }

    #[test]
    fn recipe_chain_is_complete() {
        // Verify the full production chain exists: ore → ingot → plate → component → hull
        let recipes = starter_recipes();
        let names: Vec<&str> = recipes.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Smelt Copper"));
        assert!(names.contains(&"Smelt Iron"));
        assert!(names.contains(&"Press Copper Plate"));
        assert!(names.contains(&"Press Iron Plate"));
        assert!(names.contains(&"Draw Copper Wire"));
        assert!(names.contains(&"Assemble Basic Circuit"));
        assert!(names.contains(&"Forge Hull Plate"));
    }
}
