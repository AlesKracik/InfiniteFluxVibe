// if_world: World simulation — grid, resources, ticking logic.
//
// This crate owns the "truth" of the game world. It doesn't know about
// rendering or input — it just manages the simulation state.

pub mod bodies;
pub mod galaxy;
pub mod generation;
pub mod grid;
pub mod logistics;
pub mod ships;

use bevy::prelude::*;

/// The Bevy plugin that registers all world simulation systems and resources.
///
/// In Bevy, a `Plugin` is a bundle of setup logic. The client app calls
/// `app.add_plugins(WorldPlugin)` and gets everything this crate provides.
/// This keeps crates decoupled — if_client doesn't need to know the internals
/// of if_world, just that it has a plugin to add.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        // Orbital motion runs every frame. Rendering systems that draw the
        // system map read the updated `orbit_angle`. Factory systems only
        // care about the currently-viewed body's `Grid`, so they are
        // unaffected by this update.
        app.add_systems(Update, bodies::orbital_motion_system)
            // Ships/stations live alongside the bodies that anchor them, so
            // we bundle the `ShipsPlugin` with the core world plugin. Clients
            // that don't want ship simulation can add `WorldPlugin` only and
            // skip this by composing their own plugin instead.
            .add_plugins(ships::ShipsPlugin)
            // Phase 4 adds galaxy/warp/freight routes; bundled here so any
            // client using `WorldPlugin` automatically gets the full stack.
            .add_plugins(logistics::LogisticsPlugin);
    }
}
