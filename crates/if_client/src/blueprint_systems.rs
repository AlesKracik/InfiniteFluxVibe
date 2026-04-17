// blueprint_systems.rs: Blueprint copy, paste, storage, and UI systems.
//
// The blueprint system lets players capture a rectangular region of buildings
// and stamp them down elsewhere. Hotkeys:
//   B     = toggle blueprint mode
//   Esc   = exit blueprint mode / cancel selection
//   C     = enter copy-selection mode (then two clicks for corners)
//   V     = paste selected blueprint at cursor
//   Click = in copy mode, mark corners; in paste mode, stamp buildings

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use std::fs;

use if_common::recipe::starter_recipes;
use if_common::{GridPosition, TILE_SIZE, TileType};
use if_factory::blueprint::{Blueprint, BlueprintEntry, Blueprints};
use if_factory::building::{Building, BuildingMap, BuildingType};
use if_factory::inventory::Inventory;
use if_factory::mining::{MiningDrill, ResourceNode};
use if_factory::power::{PowerConsumer, PowerGenerator};
use if_factory::production::Machine;
use if_factory::stats::ThroughputTracker;
use if_world::grid::Grid;

use crate::camera::GameCamera;
use crate::placement::{BuildingPlacement, BuildingSprite, cursor_to_grid};
use crate::ui_panels::EguiWantsPointer;

const BLUEPRINTS_DIR: &str = "saves";
const BLUEPRINTS_FILE: &str = "saves/blueprints.bin";

// --- Resources ---

/// Tracks the current blueprint interaction mode.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub enum BlueprintMode {
    /// Not in blueprint mode.
    #[default]
    Inactive,
    /// Blueprint panel is open, browsing saved blueprints.
    Browsing,
    /// Selecting the first corner of a copy region.
    CopySelectFirst,
    /// First corner selected, waiting for second corner.
    CopySelectSecond(GridPosition),
    /// A blueprint is selected and following the cursor for pasting.
    Pasting(usize),
}

/// Marker component for ghost preview sprites shown during blueprint paste.
#[derive(Component)]
pub struct BlueprintGhost;

// --- Systems ---

/// System: hotkey B toggles blueprint mode, Esc exits.
pub fn blueprint_hotkey_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<BlueprintMode>,
    mut selected_building: ResMut<BuildingPlacement>,
    ghosts: Query<Entity, With<BlueprintGhost>>,
    mut commands: Commands,
) {
    if keyboard.just_pressed(KeyCode::KeyB) {
        match *mode {
            BlueprintMode::Inactive => {
                *mode = BlueprintMode::Browsing;
                // Deselect any building placement
                selected_building.building_type = None;
            }
            _ => {
                // Exit blueprint mode
                *mode = BlueprintMode::Inactive;
                despawn_ghosts(&mut commands, &ghosts);
            }
        }
    }

    if keyboard.just_pressed(KeyCode::Escape) && *mode != BlueprintMode::Inactive {
        *mode = BlueprintMode::Inactive;
        despawn_ghosts(&mut commands, &ghosts);
    }
}

/// System: handle copy region selection clicks.
#[allow(clippy::too_many_arguments)]
pub fn blueprint_copy_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mut mode: ResMut<BlueprintMode>,
    mut blueprints: ResMut<Blueprints>,
    building_map: Res<BuildingMap>,
    grid: Res<Grid>,
    building_q: Query<(&GridPosition, &Building, Option<&Machine>)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&GlobalTransform, &Camera), With<GameCamera>>,
    egui_wants: Res<EguiWantsPointer>,
) {
    if egui_wants.0 {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = window_q.single() else {
        return;
    };
    let Ok((camera_transform, camera)) = camera_q.single() else {
        return;
    };
    let Some(grid_pos) = cursor_to_grid(window, camera_transform, camera, &grid) else {
        return;
    };

    match *mode {
        BlueprintMode::CopySelectFirst => {
            *mode = BlueprintMode::CopySelectSecond(grid_pos);
            info!(
                "Blueprint copy: first corner at ({}, {})",
                grid_pos.x, grid_pos.y
            );
        }
        BlueprintMode::CopySelectSecond(first_corner) => {
            // Compute the rectangular region
            let min_x = first_corner.x.min(grid_pos.x);
            let max_x = first_corner.x.max(grid_pos.x);
            let min_y = first_corner.y.min(grid_pos.y);
            let max_y = first_corner.y.max(grid_pos.y);

            // Collect all buildings in the region
            let mut building_data: Vec<(u32, u32, BuildingType, Option<String>)> = Vec::new();

            for x in min_x..=max_x {
                for y in min_y..=max_y {
                    let pos = GridPosition::new(x, y);
                    if let Some(entity) = building_map.get(&pos) {
                        // Look up the building component
                        if let Ok((_, building, machine)) = building_q.get(entity) {
                            let recipe_name = machine.map(|m| m.recipe.name.clone());
                            building_data.push((x, y, building.building_type, recipe_name));
                        }
                    }
                }
            }

            if building_data.is_empty() {
                info!("Blueprint copy: no buildings in selected region");
                *mode = BlueprintMode::Browsing;
                return;
            }

            let name = format!("Blueprint {}", blueprints.blueprints.len() + 1);
            let blueprint = Blueprint::from_buildings(&name, &building_data);
            info!(
                "Blueprint '{}' created with {} buildings",
                blueprint.name,
                blueprint.entries.len()
            );
            blueprints.blueprints.push(blueprint);

            // Save to disk
            save_blueprints(&blueprints);

            *mode = BlueprintMode::Browsing;
        }
        _ => {}
    }
}

/// System: handle blueprint paste placement.
#[allow(clippy::too_many_arguments)]
pub fn blueprint_paste_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mode: Res<BlueprintMode>,
    blueprints: Res<Blueprints>,
    grid: Res<Grid>,
    mut building_map: ResMut<BuildingMap>,
    mut commands: Commands,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&GlobalTransform, &Camera), With<GameCamera>>,
    node_q: Query<(Entity, &GridPosition), With<ResourceNode>>,
    egui_wants: Res<EguiWantsPointer>,
) {
    if egui_wants.0 {
        return;
    }

    let BlueprintMode::Pasting(index) = *mode else {
        return;
    };

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(blueprint) = blueprints.blueprints.get(index) else {
        return;
    };

    let Ok(window) = window_q.single() else {
        return;
    };
    let Ok((camera_transform, camera)) = camera_q.single() else {
        return;
    };
    let Some(origin) = cursor_to_grid(window, camera_transform, camera, &grid) else {
        return;
    };

    // Place each building from the blueprint
    let mut placed = 0u32;
    let mut skipped = 0u32;

    for entry in &blueprint.entries {
        let target_x = origin.x as i32 + entry.offset.0;
        let target_y = origin.y as i32 + entry.offset.1;

        // Bounds check
        if target_x < 0
            || target_y < 0
            || target_x as u32 >= grid.width
            || target_y as u32 >= grid.height
        {
            skipped += 1;
            continue;
        }

        let pos = GridPosition::new(target_x as u32, target_y as u32);

        // Skip occupied positions (for non-transport buildings)
        if entry.building_type != BuildingType::TransportLine && building_map.is_occupied(&pos) {
            skipped += 1;
            continue;
        }

        // Validate placement based on building type
        let tile = grid.get(pos.x, pos.y);
        match entry.building_type {
            BuildingType::MiningDrill => {
                if !matches!(tile, Some(TileType::CopperDeposit | TileType::IronDeposit)) {
                    skipped += 1;
                    continue;
                }
            }
            BuildingType::TransportLine => {
                // Transport lines are handled separately in the original system.
                // Skip them in blueprint paste for now.
                skipped += 1;
                continue;
            }
            _ => {
                if matches!(tile, Some(TileType::Rock)) {
                    skipped += 1;
                    continue;
                }
            }
        }

        spawn_blueprint_building(&mut commands, entry, &pos, &mut building_map, &node_q);
        placed += 1;
    }

    info!(
        "Blueprint '{}': placed {placed}, skipped {skipped}",
        blueprint.name
    );
}

/// System: show ghost previews when in paste mode.
#[allow(clippy::too_many_arguments)]
pub fn blueprint_ghost_system(
    mode: Res<BlueprintMode>,
    blueprints: Res<Blueprints>,
    grid: Res<Grid>,
    building_map: Res<BuildingMap>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&GlobalTransform, &Camera), With<GameCamera>>,
    mut ghost_q: Query<(&mut Transform, &mut Sprite), With<BlueprintGhost>>,
    mut commands: Commands,
    existing_ghosts: Query<Entity, With<BlueprintGhost>>,
) {
    let BlueprintMode::Pasting(index) = *mode else {
        // Not pasting: remove any existing ghosts
        for entity in &existing_ghosts {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Some(blueprint) = blueprints.blueprints.get(index) else {
        return;
    };

    let Ok(window) = window_q.single() else {
        return;
    };
    let Ok((camera_transform, camera)) = camera_q.single() else {
        return;
    };
    let Some(origin) = cursor_to_grid(window, camera_transform, camera, &grid) else {
        // Hide ghosts when cursor is off-grid
        for (_, mut sprite) in &mut ghost_q {
            sprite.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
        return;
    };

    // Ensure we have the right number of ghost sprites
    let ghost_count = existing_ghosts.iter().count();
    let needed = blueprint.entries.len();

    if ghost_count != needed {
        // Despawn existing and respawn correct number
        for entity in &existing_ghosts {
            commands.entity(entity).despawn();
        }
        for _ in 0..needed {
            commands.spawn((
                Sprite {
                    color: Color::srgba(1.0, 1.0, 1.0, 0.3),
                    custom_size: Some(Vec2::splat(TILE_SIZE - 1.0)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 1.5),
                BlueprintGhost,
            ));
        }
        return; // Ghosts will be positioned next frame
    }

    // Position and color each ghost
    let mut ghost_iter = ghost_q.iter_mut();
    for entry in &blueprint.entries {
        let Some((mut transform, mut sprite)) = ghost_iter.next() else {
            break;
        };

        let target_x = origin.x as i32 + entry.offset.0;
        let target_y = origin.y as i32 + entry.offset.1;

        if target_x < 0
            || target_y < 0
            || target_x as u32 >= grid.width
            || target_y as u32 >= grid.height
        {
            sprite.color = Color::srgba(1.0, 0.2, 0.2, 0.3);
            // Still position it roughly
            let world_pos = Vec2::new(target_x as f32 * TILE_SIZE, target_y as f32 * TILE_SIZE);
            transform.translation.x = world_pos.x;
            transform.translation.y = world_pos.y;
            continue;
        }

        let pos = GridPosition::new(target_x as u32, target_y as u32);
        let world_pos = pos.to_world();
        transform.translation.x = world_pos.x;
        transform.translation.y = world_pos.y;

        // Color based on validity
        let valid =
            !building_map.is_occupied(&pos) || entry.building_type == BuildingType::TransportLine;

        let base_color = building_color(entry.building_type);
        if valid {
            sprite.color = base_color.with_alpha(0.4);
        } else {
            sprite.color = Color::srgba(1.0, 0.2, 0.2, 0.4);
        }
    }
}

/// System: egui panel for blueprint mode.
#[allow(clippy::too_many_arguments)]
pub fn blueprint_panel_system(
    mut contexts: EguiContexts,
    mut mode: ResMut<BlueprintMode>,
    mut blueprints: ResMut<Blueprints>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    ghost_entities: Query<Entity, With<BlueprintGhost>>,
    mut commands: Commands,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }

    if *mode == BlueprintMode::Inactive {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Blueprints [B]")
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(200.0, -10.0))
        .resizable(false)
        .collapsible(false)
        .default_width(220.0)
        .show(ctx, |ui| {
            // Mode indicator
            let mode_text = match &*mode {
                BlueprintMode::Inactive => "Inactive",
                BlueprintMode::Browsing => "Browsing",
                BlueprintMode::CopySelectFirst => "Select first corner...",
                BlueprintMode::CopySelectSecond(_) => "Select second corner...",
                BlueprintMode::Pasting(_) => "Pasting (click to place)",
            };
            ui.label(egui::RichText::new(mode_text).italics().size(11.0));
            ui.separator();

            // Copy button
            if ui.button("Copy Region [C]").clicked()
                || matches!(*mode, BlueprintMode::Browsing)
                    && ui.input(|i| i.key_pressed(egui::Key::C))
            {
                *mode = BlueprintMode::CopySelectFirst;
                despawn_ghosts(&mut commands, &ghost_entities);
            }

            ui.separator();
            ui.heading("Saved Blueprints");

            if blueprints.blueprints.is_empty() {
                ui.label("No blueprints saved yet.");
                ui.label("Use Copy to capture a region.");
            } else {
                let mut to_delete: Option<usize> = None;

                for (i, bp) in blueprints.blueprints.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let label = format!("{} ({} buildings)", bp.name, bp.entries.len());
                        let is_selected = matches!(*mode, BlueprintMode::Pasting(idx) if idx == i);

                        let button = if is_selected {
                            egui::Button::new(egui::RichText::new(&label).strong())
                                .fill(egui::Color32::from_rgb(60, 100, 60))
                        } else {
                            egui::Button::new(&label)
                        };

                        if ui.add(button).clicked() {
                            *mode = BlueprintMode::Pasting(i);
                        }

                        if ui
                            .add(egui::Button::new("X").fill(egui::Color32::from_rgb(120, 40, 40)))
                            .clicked()
                        {
                            to_delete = Some(i);
                        }
                    });
                }

                if let Some(idx) = to_delete {
                    blueprints.blueprints.remove(idx);
                    save_blueprints(&blueprints);
                    // If we were pasting the deleted blueprint, go back to browsing
                    if matches!(*mode, BlueprintMode::Pasting(i) if i == idx) {
                        *mode = BlueprintMode::Browsing;
                        despawn_ghosts(&mut commands, &ghost_entities);
                    } else if let BlueprintMode::Pasting(i) = &mut *mode {
                        // Adjust index if a blueprint before the selected one was deleted
                        if *i > idx {
                            *i -= 1;
                        }
                    }
                }
            }

            ui.separator();
            if ui.button("Close [Esc]").clicked() {
                *mode = BlueprintMode::Inactive;
                despawn_ghosts(&mut commands, &ghost_entities);
            }
        });

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

/// System: keyboard shortcut C to enter copy mode while browsing.
pub fn blueprint_copy_hotkey_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<BlueprintMode>,
    ghosts: Query<Entity, With<BlueprintGhost>>,
    mut commands: Commands,
) {
    if *mode == BlueprintMode::Browsing && keyboard.just_pressed(KeyCode::KeyC) {
        *mode = BlueprintMode::CopySelectFirst;
        despawn_ghosts(&mut commands, &ghosts);
    }
}

// --- Startup system ---

/// Startup system: load blueprints from disk and insert as resource.
pub fn load_blueprints(mut commands: Commands) {
    let blueprints = match fs::read(BLUEPRINTS_FILE) {
        Ok(bytes) => match Blueprints::from_bytes(&bytes) {
            Ok(bp) => {
                info!(
                    "Loaded {} blueprints from {}",
                    bp.blueprints.len(),
                    BLUEPRINTS_FILE
                );
                bp
            }
            Err(e) => {
                warn!("Failed to deserialize blueprints: {e}");
                Blueprints::default()
            }
        },
        Err(_) => {
            // No file yet, start fresh
            Blueprints::default()
        }
    };
    commands.insert_resource(blueprints);
}

// --- Helpers ---

/// Despawn all blueprint ghost entities.
fn despawn_ghosts(commands: &mut Commands, ghosts: &Query<Entity, With<BlueprintGhost>>) {
    for entity in ghosts {
        commands.entity(entity).despawn();
    }
}

/// Save blueprints to disk.
fn save_blueprints(blueprints: &Blueprints) {
    match blueprints.to_bytes() {
        Ok(bytes) => {
            if let Err(e) = fs::create_dir_all(BLUEPRINTS_DIR) {
                error!("Failed to create blueprints directory: {e}");
                return;
            }
            match fs::write(BLUEPRINTS_FILE, &bytes) {
                Ok(()) => info!("Blueprints saved to {BLUEPRINTS_FILE}"),
                Err(e) => error!("Failed to write blueprints file: {e}"),
            }
        }
        Err(e) => error!("Failed to serialize blueprints: {e}"),
    }
}

/// Spawn a building from a blueprint entry at the given position.
fn spawn_blueprint_building(
    commands: &mut Commands,
    entry: &BlueprintEntry,
    pos: &GridPosition,
    building_map: &mut BuildingMap,
    node_q: &Query<(Entity, &GridPosition), With<ResourceNode>>,
) {
    let world_pos = pos.to_world();

    let entity = match entry.building_type {
        BuildingType::MiningDrill => {
            // Find the resource node at this position
            let node_entity = node_q
                .iter()
                .find(|(_, node_pos)| **node_pos == *pos)
                .map(|(e, _)| e);

            let Some(node_entity) = node_entity else {
                return; // No resource node here
            };

            commands
                .spawn((
                    Building {
                        building_type: BuildingType::MiningDrill,
                    },
                    *pos,
                    MiningDrill::new(node_entity),
                    Inventory::new(50),
                    PowerConsumer { demand: 10.0 },
                    ThroughputTracker::new(),
                ))
                .id()
        }
        BuildingType::Smelter => {
            // Find the recipe by name, or fall back to the first smelting recipe
            let recipe = find_recipe_by_name(entry.recipe_name.as_deref(), "Smelt");

            commands
                .spawn((
                    Building {
                        building_type: BuildingType::Smelter,
                    },
                    *pos,
                    Machine::new(recipe),
                    Inventory::new(50),
                    PowerConsumer { demand: 15.0 },
                    ThroughputTracker::new(),
                ))
                .id()
        }
        BuildingType::Assembler => {
            let recipe = find_recipe_by_name(entry.recipe_name.as_deref(), "");

            commands
                .spawn((
                    Building {
                        building_type: BuildingType::Assembler,
                    },
                    *pos,
                    Machine::new(recipe),
                    Inventory::new(50),
                    PowerConsumer { demand: 20.0 },
                    ThroughputTracker::new(),
                ))
                .id()
        }
        BuildingType::Generator => commands
            .spawn((
                Building {
                    building_type: BuildingType::Generator,
                },
                *pos,
                PowerGenerator { output: 50.0 },
            ))
            .id(),
        BuildingType::TransportLine => {
            return; // Skip transport lines in blueprint paste
        }
    };

    building_map.insert(*pos, entity);

    // Spawn the visual sprite
    commands.spawn((
        Sprite {
            color: building_color(entry.building_type),
            custom_size: Some(Vec2::splat(TILE_SIZE - 2.0)),
            ..default()
        },
        Transform::from_xyz(world_pos.x, world_pos.y, 0.5),
        *pos,
        BuildingSprite,
    ));
}

/// Find a recipe by name, falling back to a recipe matching the given prefix.
fn find_recipe_by_name(name: Option<&str>, fallback_prefix: &str) -> if_common::recipe::Recipe {
    let recipes = starter_recipes();

    if let Some(name) = name
        && let Some(recipe) = recipes.iter().find(|r| r.name == name)
    {
        return recipe.clone();
    }

    // Fallback: find by prefix, or just the first non-smelting recipe
    if !fallback_prefix.is_empty() {
        recipes
            .into_iter()
            .find(|r| r.name.starts_with(fallback_prefix))
            .expect("fallback recipe exists")
    } else {
        recipes
            .into_iter()
            .find(|r| !r.name.starts_with("Smelt"))
            .expect("non-smelting recipe exists")
    }
}

/// Colors for each building type (mirrors placement.rs).
fn building_color(building_type: BuildingType) -> Color {
    match building_type {
        BuildingType::MiningDrill => Color::srgb(0.9, 0.7, 0.1),
        BuildingType::TransportLine => Color::srgb(0.3, 0.6, 0.9),
        BuildingType::Smelter => Color::srgb(0.9, 0.3, 0.2),
        BuildingType::Assembler => Color::srgb(0.5, 0.2, 0.8),
        BuildingType::Generator => Color::srgb(0.9, 0.9, 0.2),
    }
}
