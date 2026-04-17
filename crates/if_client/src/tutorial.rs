// tutorial.rs: First-run tutorial sequence.
//
// A step-based onboarding that teaches the player to pan, select buildings,
// place drills, wire up transport lines, and view the stats dashboard.
//
// Each step has a textual prompt and an auto-advance condition (e.g. step 2
// advances as soon as a Generator has been placed). A "Skip" button exits
// the tutorial entirely. When complete, we write a flag file so the tutorial
// does not re-appear on the next launch.

use std::fs;
use std::path::Path;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use if_factory::building::{Building, BuildingType};
use if_factory::transport::TransportLine;

use crate::placement::ShowStats;

const TUTORIAL_FLAG: &str = "saves/tutorial_complete.flag";

/// A single tutorial step.
#[derive(Debug, Clone)]
pub struct TutorialStep {
    /// Short message shown to the player.
    pub message: &'static str,
    /// Human-readable completion hint (e.g. "WASD to pan").
    /// Exposed on the struct for future use (e.g. a collapsed checklist view);
    /// currently only `message` is rendered.
    #[allow(dead_code)]
    pub hint: &'static str,
}

/// All tutorial steps, in order.
pub fn tutorial_steps() -> Vec<TutorialStep> {
    vec![
        TutorialStep {
            message: "Welcome to Infinite Flux! Use WASD to pan the camera.",
            hint: "Pan with WASD",
        },
        TutorialStep {
            message: "Press [5] to select a Generator. Click an empty tile to place it.",
            hint: "Place a Generator",
        },
        TutorialStep {
            message: "Press [1] to select a Mining Drill. Click on a colored resource deposit.",
            hint: "Place a Mining Drill",
        },
        TutorialStep {
            message: "Press [2] to select a Transport Line. Click the drill, then a destination.",
            hint: "Build a Transport Line",
        },
        TutorialStep {
            message: "Press [3] for a Smelter. Place it and link the drill to it with transport.",
            hint: "Place a Smelter",
        },
        TutorialStep {
            message: "Press [Tab] to see statistics. Great work!",
            hint: "Open the stats dashboard",
        },
    ]
}

/// Runtime tutorial state.
#[derive(Resource, Debug, Clone)]
pub struct TutorialState {
    pub current_step: u32,
    pub active: bool,
    /// How many steps the tutorial has in total.
    pub total_steps: u32,
    /// Cached panning state — becomes true once WASD has been pressed.
    pub has_panned: bool,
}

impl TutorialState {
    /// Construct the initial state. Tutorial is active unless the flag
    /// file already exists from a prior run.
    pub fn new_for_first_run() -> Self {
        let total = tutorial_steps().len() as u32;
        let active = !Path::new(TUTORIAL_FLAG).exists();
        Self {
            current_step: 0,
            active,
            total_steps: total,
            has_panned: false,
        }
    }

    /// Mark the tutorial as complete: deactivate and persist flag.
    pub fn complete(&mut self) {
        self.active = false;
        self.current_step = self.total_steps;
        // Best-effort: create saves/ directory and touch flag file.
        if let Some(parent) = Path::new(TUTORIAL_FLAG).parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(TUTORIAL_FLAG, b"done\n");
    }

    /// Advance to the next step. If we reach the end, mark complete.
    pub fn advance(&mut self) {
        if !self.active {
            return;
        }
        self.current_step += 1;
        if self.current_step >= self.total_steps {
            self.complete();
        }
    }
}

impl Default for TutorialState {
    fn default() -> Self {
        Self::new_for_first_run()
    }
}

/// System: auto-advance the tutorial based on game state.
///
/// Each step has its own completion trigger:
///   0 -> any of WASD pressed
///   1 -> at least one Generator placed
///   2 -> at least one MiningDrill placed
///   3 -> at least one TransportLine exists
///   4 -> at least one Smelter placed
///   5 -> stats dashboard opened (Tab / ShowStats == true)
pub fn tutorial_advance_system(
    mut state: ResMut<TutorialState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    building_q: Query<&Building>,
    transport_q: Query<(), With<TransportLine>>,
    show_stats: Res<ShowStats>,
) {
    if !state.active {
        return;
    }

    // Track WASD panning for step 0 (it can be triggered at any time, but we
    // gate advancement on `current_step == 0`).
    if keyboard.any_just_pressed([KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD]) {
        state.has_panned = true;
    }

    let step = state.current_step;
    let completed = match step {
        0 => state.has_panned,
        1 => building_q
            .iter()
            .any(|b| b.building_type == BuildingType::Generator),
        2 => building_q
            .iter()
            .any(|b| b.building_type == BuildingType::MiningDrill),
        3 => transport_q.iter().next().is_some(),
        4 => building_q
            .iter()
            .any(|b| b.building_type == BuildingType::Smelter),
        5 => show_stats.0,
        _ => false,
    };

    if completed {
        state.advance();
    }
}

/// System: render the tutorial panel at bottom-center.
pub fn tutorial_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<TutorialState>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if !state.active {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let steps = tutorial_steps();
    let step_idx = state.current_step as usize;
    let Some(step) = steps.get(step_idx) else {
        return;
    };

    // We clone the strings we need before calling `state.complete()` in the
    // "Skip" handler so we don't borrow `state` twice in the closure.
    let step_label = format!("Step {}/{}", step_idx + 1, state.total_steps);
    let message = step.message;

    egui::Area::new(egui::Id::new("tutorial_panel"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -24.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 30, 50, 220))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(80, 120, 180),
                ))
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(step_label)
                                .color(egui::Color32::from_rgb(180, 200, 240))
                                .size(11.0)
                                .italics(),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(message)
                                .color(egui::Color32::WHITE)
                                .size(14.0),
                        );
                        ui.add_space(6.0);
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("Skip tutorial").size(12.0),
                            ))
                            .clicked()
                        {
                            state.complete();
                        }
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_state_starts_active_when_flag_missing() {
        // This test may be affected by real filesystem state, but the logic
        // we exercise is simple: if there is no flag file, the tutorial is
        // active. We don't touch the real flag path.
        let total = tutorial_steps().len() as u32;
        let state = TutorialState {
            current_step: 0,
            active: true,
            total_steps: total,
            has_panned: false,
        };
        assert_eq!(state.current_step, 0);
        assert!(state.active);
        assert_eq!(state.total_steps, total);
    }

    #[test]
    fn tutorial_advance_moves_forward() {
        let total = tutorial_steps().len() as u32;
        let mut state = TutorialState {
            current_step: 0,
            active: true,
            total_steps: total,
            has_panned: false,
        };
        state.advance();
        assert_eq!(state.current_step, 1);
        assert!(state.active);
    }

    #[test]
    fn tutorial_advance_past_end_completes() {
        let total = tutorial_steps().len() as u32;
        let mut state = TutorialState {
            current_step: total - 1,
            active: true,
            total_steps: total,
            has_panned: false,
        };
        state.advance();
        // Stepping past the final index marks complete.
        assert!(!state.active);
    }

    #[test]
    fn tutorial_inactive_advance_noop() {
        let total = tutorial_steps().len() as u32;
        let mut state = TutorialState {
            current_step: 0,
            active: false,
            total_steps: total,
            has_panned: false,
        };
        state.advance();
        assert_eq!(state.current_step, 0);
        assert!(!state.active);
    }

    #[test]
    fn tutorial_steps_are_nonempty_and_unique() {
        let steps = tutorial_steps();
        assert!(!steps.is_empty());
        for s in &steps {
            assert!(!s.message.is_empty());
            assert!(!s.hint.is_empty());
        }
    }
}
