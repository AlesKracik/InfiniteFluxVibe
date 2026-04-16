// save_load.rs: Save and load game state with F5/F9 hotkeys.
//
// Save collects all game state into a SaveData struct, serializes it
// with bincode, and writes to saves/quicksave.bin.
//
// Load reads the file, deserializes, despawns all game entities, and
// respawns everything from the save data, reconstructing Entity references
// via a save ID -> Entity mapping.

use bevy::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use if_common::GridPosition;
use if_common::TILE_SIZE;
use if_common::save::*;
use if_common::skill::PlayerSkills;
use if_factory::building::{Building, BuildingMap, BuildingType};
use if_factory::inventory::Inventory;
use if_factory::mining::{MiningDrill, ResourceNode};
use if_factory::power::{PowerConsumer, PowerGenerator};
use if_factory::production::Machine;
use if_factory::stats::ThroughputTracker;
use if_factory::transport::TransportLine;
use if_world::grid::Grid;

use crate::building_labels::BuildingLabel;
use crate::grid_renderer::TileSprite;
use crate::placement::{BuildingSprite, TransportLineVisual};

const SAVE_DIR: &str = "saves";
const SAVE_FILE: &str = "saves/quicksave.bin";

/// System: F5 triggers quicksave.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn save_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    grid: Res<Grid>,
    player_skills: Res<PlayerSkills>,
    node_q: Query<(Entity, &GridPosition, &ResourceNode)>,
    building_q: Query<(
        Entity,
        &GridPosition,
        &Building,
        Option<&Inventory>,
        Option<&MiningDrill>,
        Option<&Machine>,
        Option<&PowerConsumer>,
        Option<&PowerGenerator>,
    )>,
    transport_q: Query<&TransportLine>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }

    info!("Saving game...");

    // Build entity -> save_id mapping.
    // Resource nodes get IDs first, then buildings.
    let mut entity_to_id: HashMap<Entity, u32> = HashMap::new();
    let mut next_id: u32 = 0;

    // Collect resource nodes
    let mut resource_nodes = Vec::new();
    for (entity, pos, node) in &node_q {
        let save_id = next_id;
        entity_to_id.insert(entity, save_id);
        next_id += 1;

        resource_nodes.push(SaveResourceNode {
            save_id,
            position: *pos,
            resource: node.resource,
            yield_per_tick: node.yield_per_tick,
            remaining: node.remaining,
        });
    }

    // Collect buildings
    let mut buildings = Vec::new();
    for (entity, pos, building, inventory, drill, machine, consumer, generator) in &building_q {
        let save_id = next_id;
        entity_to_id.insert(entity, save_id);
        next_id += 1;

        let kind = match building.building_type {
            BuildingType::MiningDrill => {
                let drill = drill.expect("MiningDrill should have MiningDrill component");
                let target_node_id = entity_to_id
                    .get(&drill.target_node)
                    .copied()
                    .unwrap_or(u32::MAX);
                SaveBuildingKind::MiningDrill {
                    target_node_id,
                    extraction_progress: drill.extraction_progress(),
                }
            }
            BuildingType::Smelter => {
                let machine = machine.expect("Smelter should have Machine component");
                SaveBuildingKind::Smelter {
                    recipe: machine.recipe.clone(),
                    processing_ticks_remaining: machine.processing_ticks_remaining(),
                    tick_progress: machine.tick_progress(),
                    is_processing: machine.is_processing(),
                }
            }
            BuildingType::Assembler => {
                let machine = machine.expect("Assembler should have Machine component");
                SaveBuildingKind::Assembler {
                    recipe: machine.recipe.clone(),
                    processing_ticks_remaining: machine.processing_ticks_remaining(),
                    tick_progress: machine.tick_progress(),
                    is_processing: machine.is_processing(),
                }
            }
            BuildingType::Generator => {
                let power_gen = generator.expect("Generator should have PowerGenerator component");
                SaveBuildingKind::Generator {
                    output: power_gen.output,
                }
            }
            BuildingType::TransportLine => {
                // Transport lines are stored separately, skip
                continue;
            }
        };

        let saved_inventory = inventory.map(|inv| SaveInventory {
            items: inv
                .contents()
                .into_iter()
                .map(|stack| (stack.item, stack.quantity))
                .collect(),
            capacity: inv.capacity,
        });

        let power_demand = consumer.map(|c| c.demand);

        buildings.push(SaveBuilding {
            save_id,
            position: *pos,
            kind,
            inventory: saved_inventory,
            power_demand,
        });
    }

    // Collect transport lines
    let mut transport_lines = Vec::new();
    for line in &transport_q {
        let source_id = entity_to_id.get(&line.source).copied().unwrap_or(u32::MAX);
        let destination_id = entity_to_id
            .get(&line.destination)
            .copied()
            .unwrap_or(u32::MAX);

        let items_in_transit = line
            .items_in_transit()
            .iter()
            .map(|t| SaveTransitItem {
                item: t.item,
                quantity: t.quantity,
                ticks_remaining: t.ticks_remaining,
            })
            .collect();

        transport_lines.push(SaveTransportLine {
            source_id,
            destination_id,
            transit_ticks: line.transit_ticks,
            capacity: line.capacity,
            item_filter: line.item_filter,
            items_in_transit,
        });
    }

    // Build grid save data
    let save_grid = SaveGrid {
        width: grid.width,
        height: grid.height,
        tiles: grid.tiles().to_vec(),
    };

    let save_data = SaveData {
        grid: save_grid,
        skills: player_skills.skills_map().clone(),
        resource_nodes,
        buildings,
        transport_lines,
    };

    // Serialize and write
    match save_data.to_bytes() {
        Ok(bytes) => {
            if let Err(e) = fs::create_dir_all(SAVE_DIR) {
                error!("Failed to create save directory: {e}");
                return;
            }
            match fs::write(SAVE_FILE, &bytes) {
                Ok(()) => info!("Game saved to {} ({} bytes)", SAVE_FILE, bytes.len()),
                Err(e) => error!("Failed to write save file: {e}"),
            }
        }
        Err(e) => error!("Failed to serialize save data: {e}"),
    }
}

/// System: F9 triggers quickload.
#[allow(clippy::too_many_arguments)]
pub fn load_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    // Entities to despawn:
    node_q: Query<Entity, With<ResourceNode>>,
    building_q: Query<Entity, With<Building>>,
    transport_q: Query<Entity, With<TransportLine>>,
    sprite_q: Query<Entity, With<BuildingSprite>>,
    label_q: Query<Entity, With<BuildingLabel>>,
    transport_visual_q: Query<Entity, With<TransportLineVisual>>,
    tile_sprite_q: Query<Entity, With<TileSprite>>,
    // Resources to replace:
    mut building_map: ResMut<BuildingMap>,
    mut player_skills: ResMut<PlayerSkills>,
) {
    if !keyboard.just_pressed(KeyCode::F9) {
        return;
    }

    let save_path = Path::new(SAVE_FILE);
    if !save_path.exists() {
        warn!("No save file found at {SAVE_FILE}");
        return;
    }

    info!("Loading game...");

    let bytes = match fs::read(save_path) {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read save file: {e}");
            return;
        }
    };

    let save_data = match SaveData::from_bytes(&bytes) {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to deserialize save data: {e}");
            return;
        }
    };

    // --- Despawn all existing game entities ---
    for entity in &node_q {
        commands.entity(entity).despawn();
    }
    for entity in &building_q {
        commands.entity(entity).despawn();
    }
    for entity in &transport_q {
        commands.entity(entity).despawn();
    }
    for entity in &sprite_q {
        commands.entity(entity).despawn();
    }
    for entity in &label_q {
        commands.entity(entity).despawn();
    }
    for entity in &transport_visual_q {
        commands.entity(entity).despawn();
    }
    for entity in &tile_sprite_q {
        commands.entity(entity).despawn();
    }

    // --- Restore grid ---
    let new_grid = Grid::from_raw(
        save_data.grid.width,
        save_data.grid.height,
        save_data.grid.tiles,
    );

    // Respawn tile visuals
    for y in 0..new_grid.height {
        for x in 0..new_grid.width {
            let tile = new_grid.get(x, y).unwrap();
            let pos = GridPosition::new(x, y);
            let world_pos = pos.to_world();
            commands.spawn((
                Sprite {
                    color: tile.color(),
                    custom_size: Some(bevy::math::Vec2::splat(TILE_SIZE)),
                    ..default()
                },
                Transform::from_xyz(world_pos.x, world_pos.y, 0.0),
                pos,
                TileSprite,
            ));
        }
    }

    commands.insert_resource(new_grid);

    // --- Restore player skills ---
    *player_skills = PlayerSkills::from_map(save_data.skills);

    // --- Restore resource nodes ---
    // save_id -> Entity mapping for resolving references
    let mut id_to_entity: HashMap<u32, Entity> = HashMap::new();

    for node_data in &save_data.resource_nodes {
        let entity = commands
            .spawn((
                node_data.position,
                ResourceNode {
                    resource: node_data.resource,
                    yield_per_tick: node_data.yield_per_tick,
                    remaining: node_data.remaining,
                },
            ))
            .id();
        id_to_entity.insert(node_data.save_id, entity);
    }

    // --- Restore buildings ---
    // Reset BuildingMap
    *building_map = BuildingMap::default();

    for building_data in &save_data.buildings {
        let pos = building_data.position;
        let world_pos = pos.to_world();

        let entity = match &building_data.kind {
            SaveBuildingKind::MiningDrill {
                target_node_id,
                extraction_progress,
            } => {
                let target_entity = id_to_entity
                    .get(target_node_id)
                    .copied()
                    .unwrap_or(Entity::PLACEHOLDER);

                let inv = restore_inventory(&building_data.inventory);

                let mut cmd = commands.spawn((
                    Building {
                        building_type: BuildingType::MiningDrill,
                    },
                    pos,
                    MiningDrill::with_progress(target_entity, *extraction_progress),
                    inv,
                    ThroughputTracker::new(),
                ));
                if let Some(demand) = building_data.power_demand {
                    cmd.insert(PowerConsumer { demand });
                }
                cmd.id()
            }
            SaveBuildingKind::Smelter {
                recipe,
                processing_ticks_remaining,
                tick_progress,
                is_processing,
            } => {
                let inv = restore_inventory(&building_data.inventory);

                let mut cmd = commands.spawn((
                    Building {
                        building_type: BuildingType::Smelter,
                    },
                    pos,
                    Machine::with_state(
                        recipe.clone(),
                        *processing_ticks_remaining,
                        *tick_progress,
                        *is_processing,
                    ),
                    inv,
                    ThroughputTracker::new(),
                ));
                if let Some(demand) = building_data.power_demand {
                    cmd.insert(PowerConsumer { demand });
                }
                cmd.id()
            }
            SaveBuildingKind::Assembler {
                recipe,
                processing_ticks_remaining,
                tick_progress,
                is_processing,
            } => {
                let inv = restore_inventory(&building_data.inventory);

                let mut cmd = commands.spawn((
                    Building {
                        building_type: BuildingType::Assembler,
                    },
                    pos,
                    Machine::with_state(
                        recipe.clone(),
                        *processing_ticks_remaining,
                        *tick_progress,
                        *is_processing,
                    ),
                    inv,
                    ThroughputTracker::new(),
                ));
                if let Some(demand) = building_data.power_demand {
                    cmd.insert(PowerConsumer { demand });
                }
                cmd.id()
            }
            SaveBuildingKind::Generator { output } => commands
                .spawn((
                    Building {
                        building_type: BuildingType::Generator,
                    },
                    pos,
                    PowerGenerator { output: *output },
                ))
                .id(),
        };

        id_to_entity.insert(building_data.save_id, entity);
        building_map.insert(pos, entity);

        // Spawn visual sprite for this building
        let building_type = match &building_data.kind {
            SaveBuildingKind::MiningDrill { .. } => BuildingType::MiningDrill,
            SaveBuildingKind::Smelter { .. } => BuildingType::Smelter,
            SaveBuildingKind::Assembler { .. } => BuildingType::Assembler,
            SaveBuildingKind::Generator { .. } => BuildingType::Generator,
        };
        let color = building_color(building_type);
        commands.spawn((
            Sprite {
                color,
                custom_size: Some(bevy::math::Vec2::splat(TILE_SIZE - 2.0)),
                ..default()
            },
            Transform::from_xyz(world_pos.x, world_pos.y, 0.5),
            pos,
            BuildingSprite,
        ));
    }

    // --- Restore transport lines ---
    for line_data in &save_data.transport_lines {
        let source = id_to_entity
            .get(&line_data.source_id)
            .copied()
            .unwrap_or(Entity::PLACEHOLDER);
        let destination = id_to_entity
            .get(&line_data.destination_id)
            .copied()
            .unwrap_or(Entity::PLACEHOLDER);

        let items_in_transit: Vec<if_factory::transport::TransitItem> = line_data
            .items_in_transit
            .iter()
            .map(|t| if_factory::transport::TransitItem {
                item: t.item,
                quantity: t.quantity,
                ticks_remaining: t.ticks_remaining,
            })
            .collect();

        let line_entity = commands
            .spawn(TransportLine::with_transit_items(
                source,
                destination,
                line_data.transit_ticks,
                line_data.capacity,
                line_data.item_filter,
                items_in_transit,
            ))
            .id();

        // Spawn visual for transport line (simplified midpoint sprite)
        if let (Some(&src_entity), Some(&dst_entity)) = (
            id_to_entity.get(&line_data.source_id),
            id_to_entity.get(&line_data.destination_id),
        ) {
            // Find positions of source and destination buildings
            let src_pos = save_data
                .buildings
                .iter()
                .find(|b| id_to_entity.get(&b.save_id) == Some(&src_entity))
                .map(|b| b.position);
            let dst_pos = save_data
                .buildings
                .iter()
                .find(|b| id_to_entity.get(&b.save_id) == Some(&dst_entity))
                .map(|b| b.position);

            if let (Some(src_pos), Some(dst_pos)) = (src_pos, dst_pos) {
                let src_world = src_pos.to_world();
                let dst_world = dst_pos.to_world();
                let mid = bevy::math::Vec2::new(
                    (src_world.x + dst_world.x) / 2.0,
                    (src_world.y + dst_world.y) / 2.0,
                );
                let dx = (dst_pos.x as f32 - src_pos.x as f32).abs();
                let dy = (dst_pos.y as f32 - src_pos.y as f32).abs();
                let distance = (dx * dx + dy * dy).sqrt();

                commands.spawn((
                    Sprite {
                        color: building_color(BuildingType::TransportLine),
                        custom_size: Some(bevy::math::Vec2::new(distance * TILE_SIZE * 0.3, 4.0)),
                        ..default()
                    },
                    Transform::from_xyz(mid.x, mid.y, 0.4)
                        .with_rotation(Quat::from_rotation_z(dy.atan2(dx))),
                    BuildingSprite,
                    TransportLineVisual(line_entity),
                ));
            }
        }
    }

    info!(
        "Game loaded: {} nodes, {} buildings, {} transport lines",
        save_data.resource_nodes.len(),
        save_data.buildings.len(),
        save_data.transport_lines.len()
    );
}

/// Restore an Inventory from saved data.
fn restore_inventory(saved: &Option<SaveInventory>) -> Inventory {
    match saved {
        Some(inv_data) => {
            let mut inv = Inventory::new(inv_data.capacity);
            for (&item, &qty) in &inv_data.items {
                inv.try_add(item, qty);
            }
            inv
        }
        None => Inventory::new(0),
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
