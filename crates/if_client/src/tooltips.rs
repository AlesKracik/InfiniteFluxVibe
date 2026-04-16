// tooltips.rs: Hover tooltips for buildings.
//
// When the player hovers over a building (with no building selected for placement),
// an egui tooltip appears near the cursor showing building details: type, inventory,
// processing state, power output, and throughput.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use if_factory::building::{Building, BuildingMap};
use if_factory::inventory::Inventory;
use if_factory::mining::{MiningDrill, ResourceNode};
use if_factory::power::PowerGenerator;
use if_factory::production::Machine;
use if_factory::stats::ThroughputTracker;
use if_world::grid::Grid;

use crate::camera::GameCamera;
use crate::placement::{BuildingPlacement, cursor_to_grid};
use crate::ui_panels::EguiWantsPointer;

/// System: show a tooltip window when hovering over a building.
///
/// Only shows when no building is selected for placement, so the tooltip
/// does not interfere with the ghost preview.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn building_tooltip_system(
    mut contexts: EguiContexts,
    selected: Res<BuildingPlacement>,
    grid: Res<Grid>,
    building_map: Res<BuildingMap>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&GlobalTransform, &Camera), With<GameCamera>>,
    building_q: Query<(
        &Building,
        Option<&Inventory>,
        Option<&Machine>,
        Option<&MiningDrill>,
        Option<&PowerGenerator>,
        Option<&ThroughputTracker>,
    )>,
    node_q: Query<&ResourceNode>,
    egui_wants: Res<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }

    // Don't show tooltips when a building is selected for placement
    if selected.building_type.is_some() {
        return;
    }

    // Don't show tooltips when egui already wants the pointer (over a panel)
    if egui_wants.0 {
        return;
    }

    let Ok(window) = window_q.single() else {
        return;
    };
    let Ok((camera_transform, camera)) = camera_q.single() else {
        return;
    };

    // Convert cursor to grid position
    let Some(grid_pos) = cursor_to_grid(window, camera_transform, camera, &grid) else {
        return;
    };

    // Look up the building at this grid position
    let Some(entity) = building_map.get(&grid_pos) else {
        return;
    };

    let Ok((building, inventory, machine, drill, generator, throughput)) = building_q.get(entity)
    else {
        return;
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Position the tooltip near the cursor
    let cursor_pos = window.cursor_position().unwrap_or_default();
    let tooltip_pos = egui::pos2(cursor_pos.x + 16.0, cursor_pos.y + 16.0);

    egui::Area::new(egui::Id::new("building_tooltip"))
        .fixed_pos(tooltip_pos)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(220.0);

                // Building type header
                ui.label(
                    egui::RichText::new(format!("{}", building.building_type))
                        .strong()
                        .size(14.0),
                );
                ui.label(
                    egui::RichText::new(format!("at ({}, {})", grid_pos.x, grid_pos.y))
                        .size(11.0)
                        .color(egui::Color32::GRAY),
                );
                ui.separator();

                // Inventory contents
                if let Some(inv) = inventory {
                    let contents = inv.contents();
                    if contents.is_empty() {
                        ui.label("Inventory: empty");
                    } else {
                        ui.label(
                            egui::RichText::new(format!(
                                "Inventory ({}/{}):",
                                inv.total_count(),
                                inv.capacity
                            ))
                            .size(12.0),
                        );
                        for stack in &contents {
                            ui.label(format!("  {}: {}", stack.item, stack.quantity));
                        }
                    }
                    ui.add_space(2.0);
                }

                // Machine-specific info
                if let Some(m) = machine {
                    ui.label(format!("Recipe: {}", m.recipe.name));
                    if m.is_processing() {
                        let pct = (m.progress_fraction() * 100.0) as u32;
                        ui.label(format!("Processing: {pct}%"));
                    } else {
                        ui.label("Idle");
                    }
                    ui.add_space(2.0);
                }

                // Drill-specific info
                if let Some(d) = drill {
                    if let Ok(node) = node_q.get(d.target_node) {
                        ui.label(format!("Mining: {}", node.resource));
                        ui.label(format!("Rate: {:.1}/tick", node.yield_per_tick));
                        ui.label(format!("Remaining: {}", node.remaining));
                    }
                    ui.add_space(2.0);
                }

                // Generator-specific info
                if let Some(power_gen) = generator {
                    ui.label(format!("Power output: {:.0}", power_gen.output));
                    ui.add_space(2.0);
                }

                // Throughput
                if let Some(t) = throughput
                    && t.items_per_minute > 0.1
                {
                    ui.label(format!("Throughput: {:.1}/min", t.items_per_minute));
                }
            });
        });
}
