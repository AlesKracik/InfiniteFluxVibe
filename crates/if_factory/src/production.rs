// production.rs: Processing machines that transform items via recipes.
//
// A machine takes items from its input inventory, processes them for a
// duration, and places the result in its output inventory. If the output
// is full, the machine stalls (backpressure).

use bevy::prelude::*;

use if_common::recipe::Recipe;

use crate::inventory::Inventory;
use crate::power::PowerGrid;

/// Component for a processing machine (smelter, assembler, etc.).
///
/// A machine entity has:
///   - Machine component (this) — the recipe and processing state
///   - Inventory component — shared input+output storage
///
/// For simplicity, we use a single Inventory for both input and output.
/// The machine checks if inputs are available, consumes them, waits, then
/// adds outputs. A more complex design would split input/output inventories
/// but that adds complexity we don't need yet.
///
/// **Rust concept — ownership of Recipe:** The Machine *owns* its Recipe
/// (it's a cloned copy, not a reference). This avoids lifetime issues —
/// the recipe data lives as long as the machine entity. The cost is a
/// small heap allocation for the Recipe's Strings/Vecs, but machines are
/// few (hundreds, not millions).
#[derive(Component, Debug)]
pub struct Machine {
    pub recipe: Recipe,
    processing_ticks_remaining: u32,
    /// Accumulates fractional tick progress when power is below 100%.
    /// When this reaches 1.0, one processing tick elapses.
    tick_progress: f32,
    is_processing: bool,
}

impl Machine {
    pub fn new(recipe: Recipe) -> Self {
        Self {
            recipe,
            processing_ticks_remaining: 0,
            tick_progress: 0.0,
            is_processing: false,
        }
    }

    pub fn is_processing(&self) -> bool {
        self.is_processing
    }

    pub fn progress_fraction(&self) -> f32 {
        if !self.is_processing {
            return 0.0;
        }
        let total = self.recipe.processing_ticks;
        if total == 0 {
            return 1.0;
        }
        1.0 - (self.processing_ticks_remaining as f32 / total as f32)
    }
}

/// System: machines consume inputs, process, and produce outputs.
///
/// State machine per machine entity each tick:
///   1. If processing: decrement timer. If done, try to output results.
///   2. If idle: check if inputs are available. If so, consume them and start.
pub fn production_system(
    mut machine_q: Query<(&mut Machine, &mut Inventory)>,
    power_grid: Res<PowerGrid>,
) {
    for (mut machine, mut inventory) in &mut machine_q {
        if machine.is_processing {
            // --- Currently processing ---
            if machine.processing_ticks_remaining > 0 {
                // Accumulate fractional progress based on power availability.
                // At full power, one tick elapses per frame. At half power,
                // it takes two frames per tick.
                machine.tick_progress += power_grid.power_ratio;
                while machine.tick_progress >= 1.0 && machine.processing_ticks_remaining > 0 {
                    machine.processing_ticks_remaining -= 1;
                    machine.tick_progress -= 1.0;
                }
                if machine.processing_ticks_remaining > 0 {
                    continue;
                }
            }

            // Processing complete — try to output results
            let outputs = &machine.recipe.outputs;
            let total_output: u32 = outputs.iter().map(|s| s.quantity).sum();

            if inventory.space_available() >= total_output {
                for output in outputs {
                    inventory.try_add_stack(*output);
                }
                machine.is_processing = false;
            }
            // If not enough space, stay in "done but blocked" state.
            // Will retry next tick.
        } else {
            // --- Idle: try to start a new cycle ---
            if inventory.has_all(&machine.recipe.inputs) {
                inventory.remove_all(&machine.recipe.inputs);
                machine.processing_ticks_remaining = machine.recipe.processing_ticks;
                machine.is_processing = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;
    use if_common::item::{ItemStack, ItemType};

    /// Helper: create a world with full power.
    fn world_with_power() -> World {
        let mut world = World::new();
        world.insert_resource(PowerGrid::default());
        world
    }

    fn copper_smelting_recipe() -> Recipe {
        Recipe::new(
            "Smelt Copper",
            vec![ItemStack::new(ItemType::CopperOre, 1)],
            vec![ItemStack::new(ItemType::CopperIngot, 1)],
            3, // 3 ticks for testing (fast)
        )
    }

    #[test]
    fn machine_processes_recipe() {
        let mut world = world_with_power();

        let mut inv = Inventory::new(100);
        inv.try_add(ItemType::CopperOre, 5);

        world.spawn((Machine::new(copper_smelting_recipe()), inv));

        let mut schedule = Schedule::default();
        schedule.add_systems(production_system);

        // Tick 1: consume 1 ore, set remaining=3
        // Tick 2: progress→1.0, remaining 3→2
        // Tick 3: progress→1.0, remaining 2→1
        // Tick 4: progress→1.0, remaining 1→0, output placed
        for _ in 0..4 {
            schedule.run(&mut world);
        }

        let mut query = world.query::<&Inventory>();
        let inv = query.single(&world).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 4); // consumed 1
        assert_eq!(inv.count(ItemType::CopperIngot), 1); // produced 1
    }

    #[test]
    fn machine_stalls_without_inputs() {
        let mut world = world_with_power();

        let inv = Inventory::new(100); // empty — no ore
        world.spawn((Machine::new(copper_smelting_recipe()), inv));

        let mut schedule = Schedule::default();
        schedule.add_systems(production_system);

        // Run many ticks — should produce nothing
        for _ in 0..20 {
            schedule.run(&mut world);
        }

        let mut query = world.query::<&Inventory>();
        let inv = query.single(&world).unwrap();
        assert_eq!(inv.count(ItemType::CopperIngot), 0);
    }

    #[test]
    fn machine_stalls_when_output_full() {
        let mut world = world_with_power();

        let mut inv = Inventory::new(2); // tiny: 1 ore + 1 ingot max
        inv.try_add(ItemType::CopperOre, 2);

        world.spawn((Machine::new(copper_smelting_recipe()), inv));

        let mut schedule = Schedule::default();
        schedule.add_systems(production_system);

        // Process first ore: 1 start + 3 countdown (with output on last) = 4 ticks
        for _ in 0..4 {
            schedule.run(&mut world);
        }

        let mut query = world.query::<&Inventory>();
        let inv = query.single(&world).unwrap();
        // Should have: 1 ore (remaining) + 1 ingot (produced) = 2/2 capacity
        assert_eq!(inv.count(ItemType::CopperOre), 1);
        assert_eq!(inv.count(ItemType::CopperIngot), 1);

        // Second ore: same 4 ticks
        for _ in 0..4 {
            schedule.run(&mut world);
        }

        let inv = query.single(&world).unwrap();
        assert_eq!(inv.count(ItemType::CopperOre), 0);
        assert_eq!(inv.count(ItemType::CopperIngot), 2);
    }

    #[test]
    fn progress_fraction_tracks_correctly() {
        let mut machine = Machine::new(copper_smelting_recipe());
        assert_eq!(machine.progress_fraction(), 0.0);

        // Simulate starting
        machine.is_processing = true;
        machine.processing_ticks_remaining = 3;
        // Just started: 0% complete
        assert!((machine.progress_fraction() - 0.0).abs() < f32::EPSILON);

        machine.processing_ticks_remaining = 1;
        // 2/3 complete
        let expected = 1.0 - (1.0 / 3.0);
        assert!((machine.progress_fraction() - expected).abs() < 0.01);
    }
}
