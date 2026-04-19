// logistics.rs: Logistics Manager UI — a floating egui window for defining
// freight routes (ship + waypoint list) that run autonomously.
//
// The `L` hotkey toggles the panel. Routes are kept in a client-side
// `LogisticsUiState` resource; once the simulation agent lands the
// authoritative `FreightRoute` component, we'll sync / forward through a
// message. For now the UI lets the player lay out routes but does not
// dispatch them to the simulation.
//
// Design notes:
//   * The UI never queries `if_world::galaxy` directly — it works off
//     placeholder strings the user edits, plus the client-side galaxy /
//     fleet data we already have.
//   * One route being edited at a time (`editing_route: Option<usize>`).
//   * Routes and waypoints are plain Vec<_> so they're trivially savable
//     later if we want to persist them.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use if_common::item::ItemType;

use crate::ship_view::ShipVisual;
use crate::ui_panels::EguiWantsPointer;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// What a waypoint does when a ship arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaypointAction {
    Load,
    Unload,
    Visit,
}

impl WaypointAction {
    pub const ALL: &'static [WaypointAction] = &[
        WaypointAction::Load,
        WaypointAction::Unload,
        WaypointAction::Visit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WaypointAction::Load => "Load",
            WaypointAction::Unload => "Unload",
            WaypointAction::Visit => "Visit",
        }
    }
}

/// One stop along a freight route. All location fields are free strings for
/// now; the simulation agent will wire them to real IDs later.
#[derive(Clone, Debug)]
pub struct Waypoint {
    pub system: String,
    pub body: String,
    pub station: String,
    pub action: WaypointAction,
    pub item: ItemType,
    pub quantity: u32,
}

impl Default for Waypoint {
    fn default() -> Self {
        Self {
            system: "Sol".to_string(),
            body: "Mercurius".to_string(),
            station: String::new(),
            action: WaypointAction::Visit,
            item: ItemType::CopperOre,
            quantity: 10,
        }
    }
}

/// A ship + ordered list of waypoints, with an on/off toggle.
#[derive(Clone, Debug)]
pub struct FreightRouteUi {
    pub name: String,
    /// Ship assignment is a plain display string — either a ship name from
    /// `ShipVisual` or an empty "unassigned" marker. Hooking this to a real
    /// Entity lookup can wait until the simulation agent's FreightRoute type
    /// lands.
    pub ship: String,
    pub waypoints: Vec<Waypoint>,
    pub active: bool,
}

impl FreightRouteUi {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ship: String::new(),
            waypoints: Vec::new(),
            active: false,
        }
    }
}

/// Resource: all logistics-panel state (open flag, routes, which route is
/// currently being edited).
#[derive(Resource, Debug, Default)]
pub struct LogisticsUiState {
    pub open: bool,
    pub routes: Vec<FreightRouteUi>,
    /// Index into `routes` of the route currently being edited. `None` means
    /// no editor is open.
    pub editing_route: Option<usize>,
}

impl LogisticsUiState {
    /// Add a new empty route and open its editor. Returns the new index.
    pub fn new_route(&mut self) -> usize {
        let idx = self.routes.len();
        self.routes
            .push(FreightRouteUi::new(format!("Route {}", idx + 1)));
        self.editing_route = Some(idx);
        idx
    }

    /// Close the editor if the target index no longer exists.
    pub fn clamp_editor(&mut self) {
        if let Some(idx) = self.editing_route
            && idx >= self.routes.len()
        {
            self.editing_route = None;
        }
    }

    /// Delete the route at `idx` and close the editor if it was pointing to
    /// the deleted route. Returns true if deletion happened.
    pub fn delete_route(&mut self, idx: usize) -> bool {
        if idx >= self.routes.len() {
            return false;
        }
        self.routes.remove(idx);
        match self.editing_route {
            Some(e) if e == idx => self.editing_route = None,
            Some(e) if e > idx => self.editing_route = Some(e - 1),
            _ => {}
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Items / actions exposed to the UI
// ---------------------------------------------------------------------------

const UI_ITEMS: &[ItemType] = &[
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

// ---------------------------------------------------------------------------
// Hotkey
// ---------------------------------------------------------------------------

/// System: `L` toggles the Logistics panel open/closed.
pub fn logistics_hotkey_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<LogisticsUiState>,
) {
    if keyboard.just_pressed(KeyCode::KeyL) {
        state.open = !state.open;
    }
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

/// System: the main Logistics window. Shows a list of routes and, when one is
/// being edited, an inline editor below.
#[allow(clippy::too_many_arguments)]
pub fn logistics_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<LogisticsUiState>,
    ships_q: Query<&ShipVisual>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if !state.open {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Snapshot known ship names so the dropdown stays stable through the UI
    // closure. Sort alphabetically for predictable ordering.
    let mut ship_names: Vec<String> = ships_q.iter().map(|s| s.name.clone()).collect();
    ship_names.sort();

    let mut close = false;
    let mut new_route_clicked = false;
    let mut delete_route_idx: Option<usize> = None;
    let mut edit_route_idx: Option<usize> = None;
    let mut toggle_active_idx: Option<usize> = None;

    egui::Window::new("Logistics Manager")
        .collapsible(true)
        .resizable(true)
        .default_width(480.0)
        .default_height(420.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New Route").clicked() {
                    new_route_clicked = true;
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
            ui.separator();

            if state.routes.is_empty() {
                ui.label("(no routes yet — click \"New Route\" to create one)");
            } else {
                ui.label("Routes:");
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for (idx, route) in state.routes.iter().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&route.name).strong());
                                    ui.label(format!("({} waypoints)", route.waypoints.len()));
                                    let active_label =
                                        if route.active { "Active" } else { "Paused" };
                                    if ui.small_button(active_label).clicked() {
                                        toggle_active_idx = Some(idx);
                                    }
                                    if ui.small_button("Edit").clicked() {
                                        edit_route_idx = Some(idx);
                                    }
                                    if ui.small_button("Delete").clicked() {
                                        delete_route_idx = Some(idx);
                                    }
                                });
                                let ship_label = if route.ship.is_empty() {
                                    "(unassigned)".to_string()
                                } else {
                                    format!("Ship: {}", route.ship)
                                };
                                ui.label(ship_label);
                            });
                        }
                    });
            }

            ui.separator();

            // --- Route editor (inline) ---
            if let Some(edit_idx) = state.editing_route
                && let Some(route) = state.routes.get_mut(edit_idx)
            {
                ui.heading(format!("Editing: {}", route.name));

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut route.name);
                });

                ui.horizontal(|ui| {
                    ui.label("Ship:");
                    let current = if route.ship.is_empty() {
                        "(unassigned)".to_string()
                    } else {
                        route.ship.clone()
                    };
                    egui::ComboBox::from_id_salt("logistics_ship_picker")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut route.ship, String::new(), "(unassigned)");
                            for name in &ship_names {
                                ui.selectable_value(&mut route.ship, name.clone(), name);
                            }
                        });
                });

                ui.checkbox(&mut route.active, "Activate this route");

                ui.separator();
                ui.label("Waypoints:");

                let mut remove_waypoint: Option<usize> = None;
                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;

                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        let len = route.waypoints.len();
                        for (wi, wp) in route.waypoints.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("#{}", wi + 1));
                                    if ui.small_button("^").clicked() && wi > 0 {
                                        move_up = Some(wi);
                                    }
                                    if ui.small_button("v").clicked() && wi + 1 < len {
                                        move_down = Some(wi);
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        remove_waypoint = Some(wi);
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("System:");
                                    ui.text_edit_singleline(&mut wp.system);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Body:");
                                    ui.text_edit_singleline(&mut wp.body);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Station:");
                                    ui.text_edit_singleline(&mut wp.station);
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Action:");
                                    egui::ComboBox::from_id_salt(("wp_action", wi))
                                        .selected_text(wp.action.label())
                                        .show_ui(ui, |ui| {
                                            for &a in WaypointAction::ALL {
                                                ui.selectable_value(&mut wp.action, a, a.label());
                                            }
                                        });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Item:");
                                    egui::ComboBox::from_id_salt(("wp_item", wi))
                                        .selected_text(wp.item.to_string())
                                        .show_ui(ui, |ui| {
                                            for &it in UI_ITEMS {
                                                ui.selectable_value(
                                                    &mut wp.item,
                                                    it,
                                                    it.to_string(),
                                                );
                                            }
                                        });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Quantity:");
                                    let mut q = wp.quantity as i32;
                                    if ui
                                        .add(egui::DragValue::new(&mut q).range(0..=10_000))
                                        .changed()
                                    {
                                        wp.quantity = q.max(0) as u32;
                                    }
                                });
                            });
                            ui.add_space(2.0);
                        }
                    });

                if let Some(wi) = remove_waypoint {
                    route.waypoints.remove(wi);
                }
                if let Some(wi) = move_up {
                    route.waypoints.swap(wi, wi - 1);
                }
                if let Some(wi) = move_down {
                    route.waypoints.swap(wi, wi + 1);
                }

                ui.horizontal(|ui| {
                    if ui.button("Add Waypoint").clicked() {
                        route.waypoints.push(Waypoint::default());
                    }
                    if ui.button("Close Editor").clicked() {
                        // Defer actual close until after borrow ends.
                        edit_route_idx = Some(usize::MAX);
                    }
                });
            }
        });

    if new_route_clicked {
        state.new_route();
    }
    if let Some(idx) = edit_route_idx {
        if idx == usize::MAX {
            state.editing_route = None;
        } else {
            state.editing_route = Some(idx);
        }
    }
    if let Some(idx) = toggle_active_idx
        && let Some(r) = state.routes.get_mut(idx)
    {
        r.active = !r.active;
    }
    if let Some(idx) = delete_route_idx {
        state.delete_route(idx);
    }
    state.clamp_editor();

    if close {
        state.open = false;
        state.editing_route = None;
    }

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_route_opens_editor() {
        let mut state = LogisticsUiState::default();
        assert!(state.routes.is_empty());
        assert_eq!(state.editing_route, None);

        let idx = state.new_route();
        assert_eq!(idx, 0);
        assert_eq!(state.routes.len(), 1);
        assert_eq!(state.editing_route, Some(0));
    }

    #[test]
    fn delete_route_closes_editor_when_target() {
        let mut state = LogisticsUiState::default();
        state.new_route();
        assert_eq!(state.editing_route, Some(0));

        let deleted = state.delete_route(0);
        assert!(deleted);
        assert!(state.routes.is_empty());
        assert_eq!(state.editing_route, None);
    }

    #[test]
    fn delete_route_shifts_editor_index() {
        let mut state = LogisticsUiState::default();
        state.new_route(); // idx 0 (editor now at 0)
        state.new_route(); // idx 1 (editor now at 1)
        state.new_route(); // idx 2 (editor now at 2)

        // Delete the middle one — editor at 2 should shift to 1.
        assert!(state.delete_route(1));
        assert_eq!(state.routes.len(), 2);
        assert_eq!(state.editing_route, Some(1));
    }

    #[test]
    fn delete_route_out_of_range_is_noop() {
        let mut state = LogisticsUiState::default();
        assert!(!state.delete_route(0));
        assert!(!state.delete_route(999));
    }

    #[test]
    fn clamp_editor_closes_if_out_of_range() {
        let mut state = LogisticsUiState::default();
        state.editing_route = Some(5);
        state.clamp_editor();
        assert_eq!(state.editing_route, None);

        state.new_route();
        state.editing_route = Some(0);
        state.clamp_editor();
        assert_eq!(state.editing_route, Some(0));
    }

    #[test]
    fn new_route_has_default_name_and_no_waypoints() {
        let mut state = LogisticsUiState::default();
        state.new_route();
        state.new_route();
        assert_eq!(state.routes[0].name, "Route 1");
        assert_eq!(state.routes[1].name, "Route 2");
        assert!(state.routes[0].waypoints.is_empty());
        assert!(!state.routes[0].active);
        assert!(state.routes[0].ship.is_empty());
    }

    #[test]
    fn waypoint_default_is_sane() {
        let wp = Waypoint::default();
        assert_eq!(wp.action, WaypointAction::Visit);
        assert_eq!(wp.quantity, 10);
    }

    #[test]
    fn waypoint_action_has_three_variants() {
        assert_eq!(WaypointAction::ALL.len(), 3);
    }
}
