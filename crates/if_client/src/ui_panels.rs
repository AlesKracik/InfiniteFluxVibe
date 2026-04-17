// ui_panels.rs: egui-based UI panels for building palette, resource overview, and statistics.
//
// Uses bevy_egui to render immediate-mode GUI panels on top of the game world.
// The panels communicate with the game via Bevy resources and queries.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use if_common::item::ItemType;
use if_common::skill::{PlayerSkills, SkillType};
use if_factory::building::{Building, BuildingType};
use if_factory::inventory::Inventory;
use if_factory::power::PowerGrid;
use if_factory::stats::ThroughputTracker;

use crate::audio::AudioSettings;
use crate::camera::GameCamera;
use crate::orbital_view::{SavedCameras, ViewMode, toggle_view_mode};
use crate::placement::{BuildingPlacement, ShowStats};

/// All item types in the game, for iteration in UI panels.
const ALL_ITEMS: &[ItemType] = &[
    ItemType::CopperOre,
    ItemType::IronOre,
    ItemType::CopperIngot,
    ItemType::IronIngot,
    ItemType::CopperPlate,
    ItemType::IronPlate,
    ItemType::CopperWire,
    ItemType::BasicCircuit,
    ItemType::HullPlate,
];

/// All building types grouped by category for the palette.
struct BuildingCategory {
    name: &'static str,
    buildings: &'static [(BuildingType, &'static str, &'static str, [f32; 3])],
}

/// Building definitions with hotkey labels and colors matching placement.rs.
const CATEGORIES: &[BuildingCategory] = &[
    BuildingCategory {
        name: "Extraction",
        buildings: &[(
            BuildingType::MiningDrill,
            "Mining Drill",
            "[1]",
            [0.9, 0.7, 0.1],
        )],
    },
    BuildingCategory {
        name: "Logistics",
        buildings: &[(
            BuildingType::TransportLine,
            "Transport Line",
            "[2]",
            [0.3, 0.6, 0.9],
        )],
    },
    BuildingCategory {
        name: "Processing",
        buildings: &[
            (BuildingType::Smelter, "Smelter", "[3]", [0.9, 0.3, 0.2]),
            (BuildingType::Assembler, "Assembler", "[4]", [0.5, 0.2, 0.8]),
        ],
    },
    BuildingCategory {
        name: "Power",
        buildings: &[(BuildingType::Generator, "Generator", "[5]", [0.9, 0.9, 0.2])],
    },
];

/// Resource tracking whether egui wants the pointer this frame.
/// Other systems (placement, camera) check this to avoid acting on clicks
/// that are meant for UI panels.
#[derive(Resource, Default)]
pub struct EguiWantsPointer(pub bool);

/// System: render the building palette panel on the left side.
/// Clicking a building selects it for placement.
#[allow(clippy::too_many_arguments)]
pub fn building_palette_panel(
    mut contexts: EguiContexts,
    mut selected: ResMut<BuildingPlacement>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut view: ResMut<ViewMode>,
    mut saved_cams: ResMut<SavedCameras>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
    mut warmup: Local<u8>,
) {
    // Skip early frames — egui's begin_pass may not have run yet
    // when the window is first created.
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::SidePanel::left("building_palette")
        .resizable(false)
        .default_width(180.0)
        .show(ctx, |ui| {
            ui.heading("Buildings");
            ui.separator();

            for category in CATEGORIES {
                ui.label(egui::RichText::new(category.name).strong().size(13.0));
                ui.add_space(2.0);

                for &(building_type, name, hotkey, color) in category.buildings {
                    let is_selected = selected.building_type == Some(building_type);

                    ui.horizontal(|ui| {
                        // Colored square as icon
                        let (rect, _response) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            egui::Color32::from_rgb(
                                (color[0] * 255.0) as u8,
                                (color[1] * 255.0) as u8,
                                (color[2] * 255.0) as u8,
                            ),
                        );

                        // Button with name and hotkey
                        let label = format!("{name} {hotkey}");
                        let button = if is_selected {
                            egui::Button::new(egui::RichText::new(label).strong())
                                .fill(egui::Color32::from_rgb(60, 80, 120))
                        } else {
                            egui::Button::new(label)
                        };

                        if ui.add(button).clicked() {
                            selected.building_type = Some(building_type);
                        }
                    });
                }
                ui.add_space(6.0);
            }

            ui.separator();
            if ui.add(egui::Button::new("Deselect [Esc]")).clicked() {
                selected.building_type = None;
            }

            ui.add_space(8.0);
            ui.separator();
            let map_label = match *view {
                ViewMode::Surface => "Galaxy Map [M]",
                ViewMode::System => "Back to Surface [M]",
            };
            if ui.add(egui::Button::new(map_label)).clicked()
                && let Ok((mut xf, mut proj)) = camera_q.single_mut()
            {
                toggle_view_mode(&mut view, &mut saved_cams, &mut xf, &mut proj);
            }
        });

    // Track whether egui wants the pointer
    egui_wants.0 = ctx.wants_pointer_input();
}

/// System: render the resource overview panel on the right side.
/// Shows aggregate inventory counts, power status, and throughput.
#[allow(clippy::too_many_arguments)]
pub fn resource_overview_panel(
    mut contexts: EguiContexts,
    power_grid: Res<PowerGrid>,
    player_skills: Res<PlayerSkills>,
    inventory_q: Query<&Inventory>,
    throughput_q: Query<(&ThroughputTracker, &Building)>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::SidePanel::right("resource_overview")
        .resizable(false)
        .default_width(200.0)
        .show(ctx, |ui| {
            // --- Power Grid Status ---
            ui.heading("Power");
            ui.separator();

            if power_grid.total_generation == 0.0 && power_grid.total_consumption == 0.0 {
                ui.label("No power grid");
            } else {
                let pct = (power_grid.power_ratio * 100.0) as u32;
                ui.label(format!("Generation: {:.0}", power_grid.total_generation));
                ui.label(format!("Consumption: {:.0}", power_grid.total_consumption));

                // Color the percentage based on power health
                let pct_color = if pct >= 100 {
                    egui::Color32::from_rgb(100, 220, 100)
                } else if pct >= 50 {
                    egui::Color32::from_rgb(220, 200, 60)
                } else {
                    egui::Color32::from_rgb(220, 80, 80)
                };
                ui.label(
                    egui::RichText::new(format!("Efficiency: {pct}%"))
                        .color(pct_color)
                        .strong(),
                );
            }

            ui.add_space(10.0);

            // --- Aggregate Inventory ---
            ui.heading("Resources");
            ui.separator();

            // Sum items across all inventories
            let mut totals = std::collections::HashMap::new();
            for inventory in &inventory_q {
                for stack in inventory.contents() {
                    *totals.entry(stack.item).or_insert(0u32) += stack.quantity;
                }
            }

            if totals.is_empty() {
                ui.label("No items");
            } else {
                for &item_type in ALL_ITEMS {
                    if let Some(&count) = totals.get(&item_type) {
                        ui.horizontal(|ui| {
                            // Item color swatch
                            let color = item_type.color();
                            let [r, g, b, _] = color.to_srgba().to_f32_array();
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                1.0,
                                egui::Color32::from_rgb(
                                    (r * 255.0) as u8,
                                    (g * 255.0) as u8,
                                    (b * 255.0) as u8,
                                ),
                            );
                            ui.label(format!("{item_type}: {count}"));
                        });
                    }
                }
            }

            ui.add_space(10.0);

            // --- Throughput Summary ---
            ui.heading("Throughput");
            ui.separator();

            let mut has_throughput = false;
            for (tracker, building) in &throughput_q {
                if tracker.items_per_minute > 0.1 {
                    has_throughput = true;
                    ui.label(format!(
                        "{}: {:.1}/min",
                        building.building_type, tracker.items_per_minute
                    ));
                }
            }
            if !has_throughput {
                ui.label("No activity");
            }

            ui.add_space(10.0);

            // --- Skills ---
            ui.heading("Skills");
            ui.separator();

            let all_skills = [
                SkillType::Mining,
                SkillType::Smelting,
                SkillType::Fabrication,
                SkillType::Logistics,
            ];

            for skill_type in all_skills {
                let level = player_skills.get_level(skill_type);
                ui.horizontal(|ui| {
                    ui.label(format!("{skill_type}:"));
                    ui.label(
                        egui::RichText::new(format!("{level}"))
                            .color(egui::Color32::from_rgb(180, 220, 255)),
                    );
                });
            }
        });

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

/// System: render the statistics dashboard, toggled with Tab.
#[allow(clippy::too_many_arguments)]
pub fn statistics_dashboard(
    mut contexts: EguiContexts,
    show_stats: Res<ShowStats>,
    power_grid: Res<PowerGrid>,
    building_q: Query<&Building>,
    inventory_q: Query<&Inventory>,
    throughput_q: Query<(&ThroughputTracker, &Building)>,
    mut audio_settings: ResMut<AudioSettings>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if !show_stats.0 {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Statistics Dashboard")
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .collapsible(false)
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Press [Tab] to close")
                    .italics()
                    .size(11.0),
            );
            ui.separator();

            // --- Buildings Count ---
            ui.heading("Buildings");
            let mut counts = std::collections::HashMap::new();
            for building in &building_q {
                *counts.entry(building.building_type).or_insert(0u32) += 1;
            }

            if counts.is_empty() {
                ui.label("No buildings placed");
            } else {
                egui::Grid::new("building_counts")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Type").strong());
                        ui.label(egui::RichText::new("Count").strong());
                        ui.end_row();

                        let all_types = [
                            BuildingType::MiningDrill,
                            BuildingType::TransportLine,
                            BuildingType::Smelter,
                            BuildingType::Assembler,
                            BuildingType::Generator,
                        ];
                        for bt in all_types {
                            if let Some(&c) = counts.get(&bt) {
                                ui.label(format!("{bt}"));
                                ui.label(format!("{c}"));
                                ui.end_row();
                            }
                        }
                    });
            }

            ui.add_space(10.0);

            // --- Power Efficiency ---
            ui.heading("Power");
            let pct = (power_grid.power_ratio * 100.0) as u32;
            ui.label(format!("Generation: {:.0}", power_grid.total_generation));
            ui.label(format!("Consumption: {:.0}", power_grid.total_consumption));
            let surplus = power_grid.total_generation - power_grid.total_consumption;
            ui.label(format!("Surplus: {surplus:.0}"));
            ui.label(format!("Efficiency: {pct}%"));

            ui.add_space(10.0);

            // --- Production Rates ---
            ui.heading("Production Rates");

            // Aggregate items across all inventories for a snapshot
            let mut totals = std::collections::HashMap::new();
            for inventory in &inventory_q {
                for stack in inventory.contents() {
                    *totals.entry(stack.item).or_insert(0u32) += stack.quantity;
                }
            }

            egui::Grid::new("production_rates")
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Item").strong());
                    ui.label(egui::RichText::new("Stock").strong());
                    ui.end_row();

                    for &item_type in ALL_ITEMS {
                        let count = totals.get(&item_type).copied().unwrap_or(0);
                        if count > 0 {
                            ui.label(format!("{item_type}"));
                            ui.label(format!("{count}"));
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(10.0);

            // --- Per-building throughput ---
            ui.heading("Building Throughput");
            egui::Grid::new("throughput_detail")
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Building").strong());
                    ui.label(egui::RichText::new("Items/min").strong());
                    ui.end_row();

                    for (tracker, building) in &throughput_q {
                        ui.label(format!("{}", building.building_type));
                        ui.label(format!("{:.1}", tracker.items_per_minute));
                        ui.end_row();
                    }
                });

            ui.add_space(10.0);

            // --- Audio Settings ---
            ui.collapsing("Audio", |ui| {
                ui.checkbox(&mut audio_settings.sfx_enabled, "Sound effects");
                ui.horizontal(|ui| {
                    ui.label("Master volume");
                    // Display the slider as a percentage for friendliness.
                    let mut pct = (audio_settings.master_volume * 100.0).round();
                    if ui
                        .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"))
                        .changed()
                    {
                        audio_settings.master_volume = (pct / 100.0).clamp(0.0, 1.0);
                    }
                });
            });
        });

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}
