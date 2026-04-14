// if_client: The Bevy application — window, rendering, camera, input.
//
// This is the binary crate (has main.rs, not lib.rs). Running
// `cargo run -p if_client` launches this.

mod building_labels;
mod camera;
mod grid_renderer;
mod hud;
mod placement;
mod world_setup;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use if_factory::FactoryPlugin;
use if_factory::building::BuildingMap;
use if_world::WorldPlugin;
use placement::BuildingPlacement;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Infinite Flux".to_string(),
                resolution: WindowResolution::new(1280, 720),
                ..default()
            }),
            ..default()
        }))
        // Game plugins
        .add_plugins(WorldPlugin)
        .add_plugins(FactoryPlugin)
        // Resources
        .init_resource::<BuildingPlacement>()
        .init_resource::<BuildingMap>()
        .init_resource::<placement::TransportLinePlacement>()
        .init_resource::<placement::ShowStats>()
        // Startup systems: spawn_grid first, then everything else
        .add_systems(
            Startup,
            (
                if_world::grid::spawn_grid,
                (
                    camera::spawn_camera,
                    world_setup::spawn_resource_nodes,
                    grid_renderer::spawn_grid_visuals,
                    placement::spawn_ghost,
                    hud::spawn_hud,
                ),
            )
                .chain(),
        )
        // Update systems
        .add_systems(
            Update,
            (
                camera::camera_movement,
                grid_renderer::update_tile_colors,
                placement::building_selection_system,
                placement::ghost_preview_system,
                placement::placement_click_system,
                hud::update_hud,
                building_labels::spawn_building_labels,
                building_labels::update_building_labels,
                building_labels::cleanup_orphaned_labels,
            ),
        )
        .run();
}
