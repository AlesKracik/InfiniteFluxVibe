// if_factory: Factory mechanics — buildings, transport lines, machines, production.
//
// This crate contains all the simulation logic for the factory layer:
// mining drills extract ores, transport lines move items, machines process
// items according to recipes.

pub mod building;
pub mod inventory;
pub mod mining;
pub mod power;
pub mod production;
pub mod stats;
pub mod transport;

use bevy::prelude::*;

/// The Bevy plugin that registers all factory simulation systems.
pub struct FactoryPlugin;

impl Plugin for FactoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<power::PowerGrid>().add_systems(
            FixedUpdate,
            (
                power::power_system,
                mining::mining_system,
                transport::transport_system,
                production::production_system,
                stats::throughput_tracking_system,
            )
                .chain(),
        );
    }
}
