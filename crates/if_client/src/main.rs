// if_client: The Bevy application — window, rendering, camera, input.
//
// This is the binary crate (has main.rs, not lib.rs). Running
// `cargo run -p if_client` launches this.

mod audio;
mod blueprint_systems;
mod building_labels;
mod camera;
mod galaxy_view;
mod grid_renderer;
mod hud;
mod logistics;
mod market_view;
mod net;
mod notifications;
mod orbital_view;
mod placement;
mod save_load;
mod ship_view;
mod tooltips;
mod tutorial;
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
        // Networking is opt-in: the plugin installs resources and systems
        // but nothing actually connects until the player presses F9.
        .add_plugins(net::ClientNetPlugin)
        // Resources
        .init_resource::<BuildingPlacement>()
        .init_resource::<BuildingMap>()
        .init_resource::<placement::TransportLinePlacement>()
        .init_resource::<placement::ShowStats>()
        .init_resource::<EguiWantsPointer>()
        .init_resource::<notifications::Notifications>()
        .init_resource::<blueprint_systems::BlueprintMode>()
        .init_resource::<audio::AudioSettings>()
        .init_resource::<audio::SoundEffects>()
        .init_resource::<tutorial::TutorialState>()
        .init_resource::<orbital_view::ViewMode>()
        .init_resource::<orbital_view::SavedCameras>()
        .init_resource::<orbital_view::CurrentBody>()
        .init_resource::<ship_view::FleetUiState>()
        .init_resource::<galaxy_view::GalaxyUiState>()
        .init_resource::<galaxy_view::PendingWarp>()
        .init_resource::<logistics::LogisticsUiState>()
        .init_resource::<market_view::MarketsUi>()
        .init_resource::<market_view::MarketUiState>()
        .init_resource::<market_view::ContractBoardUi>()
        .init_resource::<market_view::ContractUiState>()
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
                    blueprint_systems::load_blueprints,
                    audio::load_sound_effects,
                    orbital_view::spawn_system_visuals,
                    galaxy_view::spawn_galaxy_visuals,
                    market_view::init_markets_ui,
                    market_view::init_contracts_ui,
                ),
                // Ship/station spawn must read planet positions, so it runs
                // after the system visuals exist.
                ship_view::spawn_ship_and_station_visuals,
            )
                .chain(),
        )
        // Update systems — split into multiple add_systems calls to stay
        // under Bevy's per-tuple size limit.
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
        .add_systems(
            Update,
            (
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
        .add_systems(
            Update,
            (
                // blueprints
                blueprint_systems::blueprint_hotkey_system,
                blueprint_systems::blueprint_copy_hotkey_system,
                blueprint_systems::blueprint_copy_system,
                blueprint_systems::blueprint_paste_system,
                blueprint_systems::blueprint_ghost_system,
                blueprint_systems::blueprint_panel_system,
            ),
        )
        .add_systems(
            Update,
            (
                // audio
                audio::sound_on_building_placed,
                audio::sound_on_resource_depleted,
                audio::sound_on_save_load,
                audio::sound_on_recipe_complete,
            ),
        )
        .add_systems(
            Update,
            (tutorial::tutorial_advance_system, tutorial::tutorial_panel),
        )
        .add_systems(
            Update,
            (
                // orbital / system view
                orbital_view::view_mode_toggle_system,
                orbital_view::animate_orbits,
                orbital_view::update_orbital_positions,
                orbital_view::apply_view_visibility,
                orbital_view::auto_tag_surface_visuals,
                orbital_view::system_body_labels,
                orbital_view::system_info_panel,
                orbital_view::system_click_to_visit,
            ),
        )
        .add_systems(
            Update,
            (
                // ship / fleet view (System mode only)
                ship_view::animate_ships,
                ship_view::animate_stations,
                ship_view::sync_ship_transforms,
                ship_view::fleet_panel,
                ship_view::travel_picker_panel,
                ship_view::cargo_window,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                // galaxy view + logistics
                galaxy_view::apply_galaxy_visibility,
                galaxy_view::sync_active_ring,
                galaxy_view::galaxy_click_to_select,
                galaxy_view::galaxy_info_panel,
                galaxy_view::apply_pending_warp,
                logistics::logistics_hotkey_system,
                logistics::logistics_panel,
            ),
        )
        .add_systems(
            Update,
            (
                // market + contracts
                market_view::market_hotkey_system,
                market_view::contracts_hotkey_system,
                market_view::market_panel,
                market_view::contracts_panel,
            ),
        )
        .run();
}
