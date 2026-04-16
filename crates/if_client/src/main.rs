// if_client: The Bevy application — window, rendering, camera, input.
//
// This is the binary crate (has main.rs, not lib.rs). Running
// `cargo run -p if_client` launches this.

mod building_labels;
mod camera;
mod grid_renderer;
mod hud;
mod notifications;
mod placement;
mod save_load;
mod tooltips;
mod ui_panels;
mod world_setup;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_egui::EguiPlugin;
use if_factory::FactoryPlugin;
use if_factory::building::BuildingMap;
use if_world::WorldPlugin;
use placement::BuildingPlacement;
use ui_panels::EguiWantsPointer;

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
        .add_plugins(EguiPlugin::default())
        // Resources
        .init_resource::<BuildingPlacement>()
        .init_resource::<BuildingMap>()
        .init_resource::<placement::TransportLinePlacement>()
        .init_resource::<placement::ShowStats>()
        .init_resource::<EguiWantsPointer>()
        .init_resource::<notifications::Notifications>()
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
                // egui panels
                ui_panels::building_palette_panel,
                ui_panels::resource_overview_panel,
                ui_panels::statistics_dashboard,
                // tooltips and notifications
                tooltips::building_tooltip_system,
                notifications::notification_display_system,
                notifications::notify_building_placed,
                notifications::notify_resource_depleted,
                notifications::notify_power_shortage,
                // save/load
                save_load::save_system,
                save_load::load_system,
            ),
        )
        .run();
}
