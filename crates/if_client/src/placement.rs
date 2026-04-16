// placement.rs: Building placement system — select, preview, place, remove.
//
// The player uses number keys to select a building type, then clicks on
// the grid to place it. A ghost preview follows the cursor. Right-click
// removes buildings.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use if_common::recipe::starter_recipes;
use if_common::{GridPosition, TILE_SIZE, TileType};
use if_factory::building::{Building, BuildingMap, BuildingType};
use if_factory::inventory::Inventory;
use if_factory::mining::{MiningDrill, ResourceNode};
use if_factory::power::{PowerConsumer, PowerGenerator};
use if_factory::production::Machine;
use if_factory::stats::ThroughputTracker;
use if_factory::transport::TransportLine;
use if_world::grid::Grid;

use crate::camera::GameCamera;
use crate::ui_panels::EguiWantsPointer;

/// Marker component for the ghost preview sprite.
#[derive(Component)]
pub struct GhostPreview;

/// Marker component for building sprites (so we can despawn them on removal).
/// Building entities (simulation) and sprite entities (visual) are separate
/// because in Phase 5 the server will run buildings without any rendering.
#[derive(Component)]
pub struct BuildingSprite;

/// Links a transport line visual sprite to the transport line entity,
/// so we can despawn the sprite when the line is removed.
#[derive(Component)]
pub struct TransportLineVisual(pub Entity);

/// Resource tracking which building type the player has selected for placement.
/// `None` means no building selected (normal cursor mode).
#[derive(Resource, Default)]
pub struct BuildingPlacement {
    pub building_type: Option<BuildingType>,
}

/// Resource: whether to show throughput stats on building labels.
/// Toggled with Tab.
#[derive(Resource, Default)]
pub struct ShowStats(pub bool);

/// Resource for transport line placement — needs two clicks (source, then dest).
#[derive(Resource, Default)]
pub struct TransportLinePlacement {
    pub source: Option<GridPosition>,
}

/// Colors for each building type.
fn building_color(building_type: BuildingType) -> Color {
    match building_type {
        BuildingType::MiningDrill => Color::srgb(0.9, 0.7, 0.1),
        BuildingType::TransportLine => Color::srgb(0.3, 0.6, 0.9),
        BuildingType::Smelter => Color::srgb(0.9, 0.3, 0.2),
        BuildingType::Assembler => Color::srgb(0.5, 0.2, 0.8),
        BuildingType::Generator => Color::srgb(0.9, 0.9, 0.2),
    }
}

/// Startup system: spawn the ghost preview entity (initially invisible).
pub fn spawn_ghost(mut commands: Commands) {
    commands.spawn((
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, 0.0), // fully transparent initially
            custom_size: Some(Vec2::splat(TILE_SIZE - 1.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 1.0), // Z=1 to render above tiles
        GhostPreview,
    ));
}

/// System: number keys select which building to place. Escape deselects.
///
/// Hotkeys:
///   1 = Mining Drill
///   2 = Transport Line
///   3 = Smelter
///   4 = Assembler
///   Escape = deselect
pub fn building_selection_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selected: ResMut<BuildingPlacement>,
    mut transport_placement: ResMut<TransportLinePlacement>,
    mut show_stats: ResMut<ShowStats>,
) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        selected.building_type = Some(BuildingType::MiningDrill);
        transport_placement.source = None;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        selected.building_type = Some(BuildingType::TransportLine);
        transport_placement.source = None;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        selected.building_type = Some(BuildingType::Smelter);
        transport_placement.source = None;
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        selected.building_type = Some(BuildingType::Assembler);
        transport_placement.source = None;
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        selected.building_type = Some(BuildingType::Generator);
        transport_placement.source = None;
    } else if keyboard.just_pressed(KeyCode::Escape) {
        selected.building_type = None;
        transport_placement.source = None;
    }

    if keyboard.just_pressed(KeyCode::Tab) {
        show_stats.0 = !show_stats.0;
    }
}

/// Convert the mouse cursor position (screen pixels) to a grid coordinate.
///
/// This involves two transformations:
/// 1. Screen → World: undo the camera's transform and projection
/// 2. World → Grid: divide by TILE_SIZE and round to nearest cell
///
/// Returns None if the cursor is off the grid.
pub fn cursor_to_grid(
    window: &Window,
    camera_transform: &GlobalTransform,
    camera: &Camera,
    grid: &Grid,
) -> Option<GridPosition> {
    // Step 1: get cursor position in screen coordinates
    let cursor_pos = window.cursor_position()?;

    // Step 2: convert screen → world using Bevy's camera projection
    let world_pos = camera
        .viewport_to_world_2d(camera_transform, cursor_pos)
        .ok()?;

    // Step 3: convert world → grid (round to nearest cell)
    // Add half a tile to center the rounding (tiles are positioned at their center)
    let gx = ((world_pos.x + TILE_SIZE * 0.5) / TILE_SIZE).floor() as i32;
    let gy = ((world_pos.y + TILE_SIZE * 0.5) / TILE_SIZE).floor() as i32;

    // Bounds check
    if gx >= 0 && gy >= 0 && (gx as u32) < grid.width && (gy as u32) < grid.height {
        Some(GridPosition::new(gx as u32, gy as u32))
    } else {
        None
    }
}

/// System: update the ghost preview position and color based on cursor + selection.
pub fn ghost_preview_system(
    selected: Res<BuildingPlacement>,
    grid: Res<Grid>,
    building_map: Res<BuildingMap>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&GlobalTransform, &Camera), With<GameCamera>>,
    mut ghost_q: Query<(&mut Transform, &mut Sprite), With<GhostPreview>>,
) {
    let Ok((mut ghost_transform, mut ghost_sprite)) = ghost_q.single_mut() else {
        return;
    };

    // If nothing selected, hide the ghost
    let Some(building_type) = selected.building_type else {
        ghost_sprite.color = Color::srgba(1.0, 1.0, 1.0, 0.0);
        return;
    };

    let Ok(window) = window_q.single() else {
        return;
    };
    let Ok((camera_transform, camera)) = camera_q.single() else {
        return;
    };

    // Convert cursor to grid position
    let Some(grid_pos) = cursor_to_grid(window, camera_transform, camera, &grid) else {
        ghost_sprite.color = Color::srgba(1.0, 1.0, 1.0, 0.0);
        return;
    };

    // Move ghost to the grid cell
    let world_pos = grid_pos.to_world();
    ghost_transform.translation.x = world_pos.x;
    ghost_transform.translation.y = world_pos.y;

    // Color: green if valid placement, red if invalid
    let valid = is_valid_placement(building_type, &grid_pos, &grid, &building_map);
    let base_color = building_color(building_type);
    if valid {
        // Semi-transparent version of the building color
        ghost_sprite.color = base_color.with_alpha(0.5);
    } else {
        // Red tint for invalid placement
        ghost_sprite.color = Color::srgba(1.0, 0.2, 0.2, 0.5);
    }
}

/// Check if a building can be placed at this position.
fn is_valid_placement(
    building_type: BuildingType,
    pos: &GridPosition,
    grid: &Grid,
    building_map: &BuildingMap,
) -> bool {
    // Can't place on an occupied cell (transport lines check differently)
    if building_type != BuildingType::TransportLine && building_map.is_occupied(pos) {
        return false;
    }

    // Mining drills must be on a resource deposit
    if building_type == BuildingType::MiningDrill {
        let tile = grid.get(pos.x, pos.y);
        return matches!(tile, Some(TileType::CopperDeposit | TileType::IronDeposit));
    }

    // Other buildings can go on any non-rock, non-occupied cell
    let tile = grid.get(pos.x, pos.y);
    !matches!(tile, Some(TileType::Rock))
}

/// System: left-click places a building, right-click removes one.
#[allow(clippy::too_many_arguments)]
pub fn placement_click_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    selected: Res<BuildingPlacement>,
    grid: Res<Grid>,
    mut building_map: ResMut<BuildingMap>,
    mut transport_placement: ResMut<TransportLinePlacement>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&GlobalTransform, &Camera), With<GameCamera>>,
    // For finding resource nodes at a position:
    node_q: Query<(Entity, &GridPosition), With<ResourceNode>>,
    // For despawning removed buildings:
    building_sprites_q: Query<(Entity, &GridPosition), With<BuildingSprite>>,
    // For finding and removing transport lines connected to a removed building:
    transport_line_q: Query<(Entity, &TransportLine)>,
    transport_visual_q: Query<(Entity, &TransportLineVisual)>,
    egui_wants: Res<EguiWantsPointer>,
) {
    // Don't process clicks when egui panels want the pointer
    if egui_wants.0 {
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

    // --- Left click: place ---
    if mouse.just_pressed(MouseButton::Left)
        && let Some(building_type) = selected.building_type
    {
        // Transport lines need two clicks (source + destination)
        if building_type == BuildingType::TransportLine {
            handle_transport_placement(
                &mut commands,
                &grid_pos,
                &building_map,
                &mut transport_placement,
            );
            return;
        }

        if !is_valid_placement(building_type, &grid_pos, &grid, &building_map) {
            return;
        }

        spawn_building(
            &mut commands,
            building_type,
            &grid_pos,
            &mut building_map,
            &node_q,
        );
    }

    // --- Right click: remove ---
    if mouse.just_pressed(MouseButton::Right) {
        if let Some(entity) = building_map.remove(&grid_pos) {
            // Despawn any transport lines connected to this building
            for (line_entity, line) in &transport_line_q {
                if line.source == entity || line.destination == entity {
                    commands.entity(line_entity).despawn();
                    // Despawn the visual sprite linked to this line
                    for (visual_entity, visual) in &transport_visual_q {
                        if visual.0 == line_entity {
                            commands.entity(visual_entity).despawn();
                        }
                    }
                }
            }
            commands.entity(entity).despawn();
        }
        // Also despawn the building sprite for this position
        for (sprite_entity, sprite_pos) in &building_sprites_q {
            if *sprite_pos == grid_pos {
                commands.entity(sprite_entity).despawn();
            }
        }
    }
}

/// Spawn a building entity with the correct components for its type.
fn spawn_building(
    commands: &mut Commands,
    building_type: BuildingType,
    pos: &GridPosition,
    building_map: &mut BuildingMap,
    node_q: &Query<(Entity, &GridPosition), With<ResourceNode>>,
) {
    let world_pos = pos.to_world();

    // Spawn the building entity with type-specific components
    let entity = match building_type {
        BuildingType::MiningDrill => {
            // Find the resource node at this position
            let node_entity = node_q
                .iter()
                .find(|(_, node_pos)| **node_pos == *pos)
                .map(|(e, _)| e);

            let Some(node_entity) = node_entity else {
                return; // No resource node here — shouldn't happen (validity check)
            };

            commands
                .spawn((
                    Building { building_type },
                    *pos,
                    MiningDrill::new(node_entity),
                    Inventory::new(50),
                    PowerConsumer { demand: 10.0 },
                    ThroughputTracker::new(),
                ))
                .id()
        }
        BuildingType::Smelter => {
            let recipe = starter_recipes()
                .into_iter()
                .find(|r| r.name.starts_with("Smelt"))
                .expect("smelting recipe exists");

            commands
                .spawn((
                    Building { building_type },
                    *pos,
                    Machine::new(recipe),
                    Inventory::new(50),
                    PowerConsumer { demand: 15.0 },
                    ThroughputTracker::new(),
                ))
                .id()
        }
        BuildingType::Assembler => {
            let recipe = starter_recipes()
                .into_iter()
                .find(|r| !r.name.starts_with("Smelt"))
                .expect("assembler recipe exists");

            commands
                .spawn((
                    Building { building_type },
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
                Building { building_type },
                *pos,
                PowerGenerator { output: 50.0 },
            ))
            .id(),
        BuildingType::TransportLine => {
            return; // Handled separately in handle_transport_placement
        }
    };

    building_map.insert(*pos, entity);

    // Spawn the visual sprite for this building
    commands.spawn((
        Sprite {
            color: building_color(building_type),
            custom_size: Some(Vec2::splat(TILE_SIZE - 2.0)),
            ..default()
        },
        Transform::from_xyz(world_pos.x, world_pos.y, 0.5), // Z=0.5 above tiles
        *pos,
        BuildingSprite,
    ));
}

/// Handle transport line placement — first click = source, second click = destination.
fn handle_transport_placement(
    commands: &mut Commands,
    pos: &GridPosition,
    building_map: &BuildingMap,
    transport_placement: &mut TransportLinePlacement,
) {
    if let Some(source_pos) = transport_placement.source {
        // Second click — create the transport line between source and destination
        let Some(source_entity) = building_map.get(&source_pos) else {
            transport_placement.source = None;
            return; // Source building was removed
        };
        let Some(dest_entity) = building_map.get(pos) else {
            return; // Destination has no building — need something to deliver to
        };

        // Calculate transit time based on distance
        let dx = (pos.x as f32 - source_pos.x as f32).abs();
        let dy = (pos.y as f32 - source_pos.y as f32).abs();
        let distance = (dx * dx + dy * dy).sqrt();
        let transit_ticks = (distance * 10.0).max(5.0) as u32;

        let line_entity = commands
            .spawn(TransportLine::new(
                source_entity,
                dest_entity,
                transit_ticks,
                20,
            ))
            .id();

        // Spawn a visual line indicator (simple sprite at midpoint for now)
        let src_world = source_pos.to_world();
        let dst_world = pos.to_world();
        let mid = Vec2::new(
            (src_world.x + dst_world.x) / 2.0,
            (src_world.y + dst_world.y) / 2.0,
        );
        commands.spawn((
            Sprite {
                color: building_color(BuildingType::TransportLine),
                custom_size: Some(Vec2::new(distance * TILE_SIZE * 0.3, 4.0)),
                ..default()
            },
            Transform::from_xyz(mid.x, mid.y, 0.4)
                .with_rotation(Quat::from_rotation_z(dy.atan2(dx))),
            BuildingSprite,
            TransportLineVisual(line_entity),
        ));

        transport_placement.source = None;
    } else {
        // First click — mark the source
        if building_map.is_occupied(pos) {
            transport_placement.source = Some(*pos);
        }
    }
}
