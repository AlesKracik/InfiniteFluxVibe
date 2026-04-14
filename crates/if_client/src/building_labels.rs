// building_labels.rs: Renders text labels on buildings showing inventory contents.
//
// Each building with an Inventory gets a child text entity that floats
// above it, showing what items it holds and (for machines) processing progress.

use bevy::prelude::*;

use crate::placement::ShowStats;
use if_common::{GridPosition, TILE_SIZE};
use if_factory::building::Building;
use if_factory::inventory::Inventory;
use if_factory::production::Machine;
use if_factory::stats::ThroughputTracker;

/// Marker component linking a label entity to its parent building entity.
/// We store the building's Entity so we can look up its Inventory each frame.
#[derive(Component)]
pub struct BuildingLabel {
    pub building_entity: Entity,
}

/// System: spawns a text label for each new building that has an Inventory.
///
/// We detect "new buildings" by querying for buildings that don't yet have
/// a label. We use `Without<HasLabel>` — a marker we add to buildings once
/// their label is created, preventing duplicate labels.
///
/// **Rust concept — marker components as state flags:** `HasLabel` has no
/// data. It exists purely so the query filter can distinguish "already
/// labeled" from "needs a label." This is a common ECS pattern.
#[derive(Component)]
pub struct HasLabel;

/// System: spawn labels for new buildings.
#[allow(clippy::type_complexity)]
pub fn spawn_building_labels(
    mut commands: Commands,
    new_buildings_q: Query<
        (Entity, &GridPosition),
        (With<Building>, With<Inventory>, Without<HasLabel>),
    >,
) {
    for (building_entity, pos) in &new_buildings_q {
        let world_pos = pos.to_world();

        // Spawn the label slightly above the building
        commands.spawn((
            // Text2d renders text in world space (not UI space).
            Text2d::new(""),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout::new_with_justify(Justify::Center),
            // Position above the building sprite
            Transform::from_xyz(world_pos.x, world_pos.y + TILE_SIZE * 0.6, 2.0),
            BuildingLabel { building_entity },
        ));

        // Mark the building so we don't spawn a second label
        commands.entity(building_entity).insert(HasLabel);
    }
}

/// System: update label text to reflect current inventory and machine state.
pub fn update_building_labels(
    mut labels_q: Query<(&BuildingLabel, &mut Text2d)>,
    inventory_q: Query<(&Inventory, Option<&Machine>, Option<&ThroughputTracker>)>,
    show_stats: Res<ShowStats>,
) {
    for (label, mut text) in &mut labels_q {
        let Ok((inventory, machine, tracker)) = inventory_q.get(label.building_entity) else {
            **text = String::new();
            continue;
        };

        let mut lines: Vec<String> = Vec::new();

        // Show machine processing progress if applicable
        if let Some(machine) = machine
            && machine.is_processing()
        {
            let pct = (machine.progress_fraction() * 100.0) as u32;
            lines.push(format!("{pct}%"));
        }

        if show_stats.0 {
            // Stats mode: show throughput
            if let Some(tracker) = tracker {
                lines.push(format!("{:.0}/min", tracker.items_per_minute));
            }
        } else {
            // Normal mode: show inventory contents
            let contents = inventory.contents();
            if contents.is_empty() {
                if lines.is_empty() {
                    lines.push("empty".to_string());
                }
            } else {
                for stack in &contents {
                    lines.push(format!("{}x {}", stack.quantity, stack.item));
                }
            }
        }

        **text = lines.join("\n");
    }
}

/// System: despawn labels whose building no longer exists.
pub fn cleanup_orphaned_labels(
    mut commands: Commands,
    labels_q: Query<(Entity, &BuildingLabel)>,
    buildings_q: Query<(), With<Building>>,
) {
    for (label_entity, label) in &labels_q {
        if buildings_q.get(label.building_entity).is_err() {
            commands.entity(label_entity).despawn();
        }
    }
}
