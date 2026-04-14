// if_world: World simulation — grid, resources, ticking logic.
//
// This crate owns the "truth" of the game world. It doesn't know about
// rendering or input — it just manages the simulation state.

pub mod grid;

use bevy::prelude::*;

/// The Bevy plugin that registers all world simulation systems and resources.
///
/// In Bevy, a `Plugin` is a bundle of setup logic. The client app calls
/// `app.add_plugins(WorldPlugin)` and gets everything this crate provides.
/// This keeps crates decoupled — if_client doesn't need to know the internals
/// of if_world, just that it has a plugin to add.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, _app: &mut App) {
        // World systems are registered by the client via .chain() ordering.
        // The plugin exists as a placeholder for future world simulation
        // systems (e.g., resource depletion ticking).
    }
}
