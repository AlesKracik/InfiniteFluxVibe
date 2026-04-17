// ship_view.rs: Ship and orbital-station visuals in System view, plus the
// Fleet / Travel / Cargo egui UI for dispatching ships between bodies.
//
// These types and systems are the CLIENT-SIDE placeholders while the
// simulation agent lands the authoritative Ship / OrbitalStation / ShipState
// types in if_world. The orchestrator will wire the two sides together later;
// for now, everything ship-related lives here so the UX can be designed,
// tested, and iterated on independently.
//
// Rendering conventions (matching orbital_view.rs):
//   * System origin (star) is at (0, 0).
//   * Ships render at Z=0.7 (above orbit lines and bodies).
//   * Stations render at Z=0.6 (above bodies but below ships).
//   * All ship/station entities carry the `SystemVisual` marker so they
//     are hidden in Surface view.
//
// Travel model (placeholder):
//   A ship in transit has a `TravelTarget` component with a linear
//   interpolation between `from` and `to`. Progress advances every frame in
//   `animate_ships`; on arrival the component is removed and the ship is
//   marked docked at its destination. This will be replaced by the
//   simulation agent's ShipState state machine later.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use if_common::item::ItemType;

use crate::orbital_view::{OrbitalBodyVisual, OrbitalKind, SystemVisual, ViewMode};
use crate::ui_panels::EguiWantsPointer;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const Z_STATION: f32 = 0.6;
const Z_SHIP: f32 = 0.7;

/// Sprite side length (world units) for a ship.
const SHIP_SIZE: f32 = 10.0;
/// Sprite side length (world units) for a station.
const STATION_SIZE: f32 = 14.0;

/// How close (world units) a traveling ship has to get to its destination
/// before we consider it arrived.
const ARRIVAL_EPSILON: f32 = 1.0;

/// Default travel speed, in "progress units per second" (1.0 == reaches
/// destination in 1s regardless of distance). Low enough to be watchable.
const DEFAULT_TRAVEL_SPEED: f32 = 0.15;

/// Items that can live in a ship cargo hold — matches the UI-visible subset
/// in ui_panels.rs for consistency.
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

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// A ship visible in the system view. Placeholder until the simulation
/// agent's Ship type is wired in.
#[derive(Component, Clone, Debug)]
pub struct ShipVisual {
    pub name: String,
    /// Current position in system-space.
    pub position: Vec2,
    /// Color (derived from ship state — docked vs. traveling).
    pub color: Color,
    /// Maximum cargo slots (sum across all item types).
    pub cargo_capacity: u32,
    /// Fuel percentage (0.0 to 1.0).
    pub fuel: f32,
    /// Docked-at location name ("in transit" otherwise).
    pub docked_at: Option<String>,
}

impl ShipVisual {
    /// Total cargo quantity currently in the hold, summed over all items.
    pub fn cargo_used(cargo: &ShipCargo) -> u32 {
        cargo.items.iter().map(|(_, q)| *q).sum()
    }

    /// Cargo fill percentage in [0.0, 1.0].
    pub fn cargo_fill(&self, cargo: &ShipCargo) -> f32 {
        if self.cargo_capacity == 0 {
            0.0
        } else {
            (Self::cargo_used(cargo) as f32 / self.cargo_capacity as f32).clamp(0.0, 1.0)
        }
    }
}

/// Ship cargo, kept separate from `ShipVisual` so transfer systems can borrow
/// it mutably without re-aliasing the visual data.
#[derive(Component, Clone, Debug, Default)]
pub struct ShipCargo {
    /// (item, quantity). Kept as a Vec so UI ordering is stable.
    pub items: Vec<(ItemType, u32)>,
}

impl ShipCargo {
    /// Get (or insert 0) the quantity for an item.
    pub fn get(&self, item: ItemType) -> u32 {
        self.items
            .iter()
            .find(|(i, _)| *i == item)
            .map(|(_, q)| *q)
            .unwrap_or(0)
    }

    /// Add `delta` of `item`. No-op if delta == 0.
    pub fn add(&mut self, item: ItemType, delta: u32) {
        if delta == 0 {
            return;
        }
        if let Some(entry) = self.items.iter_mut().find(|(i, _)| *i == item) {
            entry.1 = entry.1.saturating_add(delta);
        } else {
            self.items.push((item, delta));
        }
    }

    /// Try to remove `delta` of `item`. Returns the amount actually removed.
    pub fn remove(&mut self, item: ItemType, delta: u32) -> u32 {
        if let Some(entry) = self.items.iter_mut().find(|(i, _)| *i == item) {
            let removed = entry.1.min(delta);
            entry.1 -= removed;
            return removed;
        }
        0
    }
}

/// An orbital station — placeholder for the eventual simulation type.
#[derive(Component, Clone, Debug)]
pub struct StationVisual {
    pub name: String,
    pub parent_body_name: String,
    /// Radius of the station's sub-orbit around its parent body, in world
    /// units.
    pub orbit_radius: f32,
    /// Current angle (radians) on that sub-orbit.
    pub orbit_angle: f32,
}

/// Placeholder station-side storage. Mirrors `ShipCargo` so transfer UI is
/// symmetric.
#[derive(Component, Clone, Debug, Default)]
pub struct StationStorage {
    pub items: Vec<(ItemType, u32)>,
}

impl StationStorage {
    pub fn get(&self, item: ItemType) -> u32 {
        self.items
            .iter()
            .find(|(i, _)| *i == item)
            .map(|(_, q)| *q)
            .unwrap_or(0)
    }

    pub fn add(&mut self, item: ItemType, delta: u32) {
        if delta == 0 {
            return;
        }
        if let Some(entry) = self.items.iter_mut().find(|(i, _)| *i == item) {
            entry.1 = entry.1.saturating_add(delta);
        } else {
            self.items.push((item, delta));
        }
    }

    pub fn remove(&mut self, item: ItemType, delta: u32) -> u32 {
        if let Some(entry) = self.items.iter_mut().find(|(i, _)| *i == item) {
            let removed = entry.1.min(delta);
            entry.1 -= removed;
            return removed;
        }
        0
    }
}

/// Pending travel order. A ship with this component is "in transit"; when
/// progress reaches 1.0 the component is removed and the ship is docked at
/// the named destination.
#[derive(Component, Clone, Debug)]
pub struct TravelTarget {
    pub from: Vec2,
    pub to: Vec2,
    /// Current progress in [0.0, 1.0]. Starts at 0.0.
    pub progress: f32,
    /// Progress units per second.
    pub speed: f32,
    /// Destination name — used to set `docked_at` on arrival.
    pub destination_name: String,
}

impl TravelTarget {
    pub fn new(from: Vec2, to: Vec2, destination_name: String, speed: f32) -> Self {
        Self {
            from,
            to,
            progress: 0.0,
            speed,
            destination_name,
        }
    }

    /// Current interpolated position.
    pub fn current_position(&self) -> Vec2 {
        self.from + (self.to - self.from) * self.progress.clamp(0.0, 1.0)
    }

    /// Tick progress forward. Returns true if we've arrived this tick.
    pub fn advance(&mut self, dt: f32) -> bool {
        self.progress += self.speed * dt;
        if self.progress >= 1.0 {
            self.progress = 1.0;
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// egui-driven UI state: which ship (if any) has the destination picker open,
/// and which ship has its cargo window open.
#[derive(Resource, Default, Debug)]
pub struct FleetUiState {
    /// Entity of the ship whose "Travel to..." picker is open.
    pub travel_picker_for: Option<Entity>,
    /// Entity of the ship whose cargo window is open.
    pub cargo_window_for: Option<Entity>,
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// Startup: spawn 2 ships and 1 station near the first ("home") planet of
/// the placeholder system. If no planet is present yet (tests, etc.) this
/// silently no-ops — we'll try again once the system visuals exist.
pub fn spawn_ship_and_station_visuals(mut commands: Commands, bodies_q: Query<&OrbitalBodyVisual>) {
    // Find the innermost planet ("home"). Matches the convention used by
    // orbital_view.rs's PLACEHOLDER_ORBITS — the first planet by radius.
    let mut home: Option<(String, f32)> = None;
    for body in &bodies_q {
        if body.body_kind != OrbitalKind::Planet {
            continue;
        }
        match &home {
            None => home = Some((body.name.clone(), body.orbit_radius)),
            Some((_, r)) if body.orbit_radius < *r => {
                home = Some((body.name.clone(), body.orbit_radius));
            }
            _ => {}
        }
    }

    let Some((home_name, home_radius)) = home else {
        return;
    };

    // Position ships right at the home planet's initial location. The home
    // planet starts at angle 0.0 in orbital_view.rs (first planet, idx=0
    // => start_angle = 0.0).
    let home_pos = Vec2::new(home_radius, 0.0);

    // --- Station: orbits the home planet on a sub-orbit of +30 units. ---
    let station_suborbit = 30.0;
    let station_angle = 0.5_f32; // offset so station doesn't overlap the planet sprite
    let station_pos = home_pos
        + Vec2::new(
            station_suborbit * station_angle.cos(),
            station_suborbit * station_angle.sin(),
        );

    let mut station_storage = StationStorage::default();
    // Seed with a small amount of each basic item so the cargo UI has
    // something to transfer.
    station_storage.add(ItemType::CopperOre, 50);
    station_storage.add(ItemType::IronOre, 50);
    station_storage.add(ItemType::HullPlate, 10);

    commands.spawn((
        Sprite {
            color: Color::srgb(0.7, 0.7, 0.9),
            custom_size: Some(Vec2::splat(STATION_SIZE)),
            ..default()
        },
        Transform::from_xyz(station_pos.x, station_pos.y, Z_STATION),
        Visibility::Hidden,
        StationVisual {
            name: format!("{home_name} Station"),
            parent_body_name: home_name.clone(),
            orbit_radius: station_suborbit,
            orbit_angle: station_angle,
        },
        station_storage,
        SystemVisual,
    ));

    // --- Two ships, both docked at the home planet initially. ---
    for (idx, ship_name) in ["Falcon", "Corvette"].iter().enumerate() {
        // Slight offset so the two ships don't render on top of each other.
        let offset = Vec2::new(6.0 * idx as f32, -6.0 * idx as f32);
        let ship_pos = home_pos + offset;

        commands.spawn((
            Sprite {
                color: Color::srgb(0.9, 0.85, 0.5),
                custom_size: Some(Vec2::splat(SHIP_SIZE)),
                ..default()
            },
            Transform::from_xyz(ship_pos.x, ship_pos.y, Z_SHIP),
            Visibility::Hidden,
            ShipVisual {
                name: (*ship_name).to_string(),
                position: ship_pos,
                color: Color::srgb(0.9, 0.85, 0.5),
                cargo_capacity: 100,
                fuel: 1.0,
                docked_at: Some(home_name.clone()),
            },
            ShipCargo::default(),
            SystemVisual,
        ));
    }
}

// ---------------------------------------------------------------------------
// Animation
// ---------------------------------------------------------------------------

/// System: update ship Transforms from their `ShipVisual.position`.
///
/// Kept separate from `animate_ships` so we always re-sync after any system
/// that edits `position` — tests can call this directly without setting up
/// time.
pub fn sync_ship_transforms(mut ships_q: Query<(&ShipVisual, &mut Transform)>) {
    for (ship, mut xf) in &mut ships_q {
        xf.translation.x = ship.position.x;
        xf.translation.y = ship.position.y;
    }
}

/// System: advance any `TravelTarget`, interpolate `ShipVisual.position`, and
/// clear the component on arrival (marking the ship docked).
pub fn animate_ships(
    mut commands: Commands,
    time: Res<Time>,
    mut ships_q: Query<(Entity, &mut ShipVisual, &mut TravelTarget)>,
) {
    let dt = time.delta_secs();
    for (entity, mut ship, mut target) in &mut ships_q {
        let arrived = target.advance(dt);
        ship.position = target.current_position();

        if arrived || (ship.position - target.to).length() <= ARRIVAL_EPSILON {
            ship.position = target.to;
            ship.docked_at = Some(target.destination_name.clone());
            // Settled — switch back to the idle docked color.
            ship.color = Color::srgb(0.9, 0.85, 0.5);
            commands.entity(entity).remove::<TravelTarget>();
        }
    }
}

/// System: sync each station's Transform to its parent planet plus its
/// sub-orbit. Runs every frame in System view so stations visibly orbit their
/// planets.
pub fn animate_stations(
    time: Res<Time>,
    bodies_q: Query<(&OrbitalBodyVisual, &GlobalTransform)>,
    mut stations_q: Query<(&mut StationVisual, &mut Transform)>,
) {
    let dt = time.delta_secs();

    // Build a quick parent-body lookup by name. Small N (a handful of bodies),
    // so this is cheap.
    for (mut station, mut xf) in &mut stations_q {
        // Advance the station's sub-orbit at a fixed, visible rate.
        station.orbit_angle = (station.orbit_angle + 0.4 * dt).rem_euclid(std::f32::consts::TAU);

        let mut parent_pos: Option<Vec2> = None;
        for (body, gxf) in &bodies_q {
            if body.name == station.parent_body_name {
                parent_pos = Some(gxf.translation().truncate());
                break;
            }
        }
        let Some(parent) = parent_pos else {
            continue;
        };
        let offset = Vec2::new(
            station.orbit_radius * station.orbit_angle.cos(),
            station.orbit_radius * station.orbit_angle.sin(),
        );
        let pos = parent + offset;
        xf.translation.x = pos.x;
        xf.translation.y = pos.y;
    }
}

// ---------------------------------------------------------------------------
// Fleet panel (System view only)
// ---------------------------------------------------------------------------

/// System: left-side "Fleet" egui panel listing each ship, shown only in
/// System view.
#[allow(clippy::too_many_arguments)]
pub fn fleet_panel(
    mut contexts: EguiContexts,
    view: Res<ViewMode>,
    ships_q: Query<(Entity, &ShipVisual, &ShipCargo)>,
    stations_q: Query<&StationVisual>,
    mut ui_state: ResMut<FleetUiState>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if *view != ViewMode::System {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let ship_count = ships_q.iter().count();
    let station_count = stations_q.iter().count();

    egui::SidePanel::left("fleet_panel")
        .resizable(false)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading("Fleet");
            ui.label(format!("Ships: {ship_count}   Stations: {station_count}"));
            ui.separator();

            if ship_count == 0 {
                ui.label("(no ships)");
                return;
            }

            // Collect into a Vec so ordering is stable across frames.
            let mut ships: Vec<(Entity, &ShipVisual, &ShipCargo)> = ships_q.iter().collect();
            ships.sort_by(|a, b| a.1.name.cmp(&b.1.name));

            for (entity, ship, cargo) in ships {
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new(&ship.name)
                            .strong()
                            .color(egui::Color32::from_rgb(240, 240, 200)),
                    );
                    let location = ship.docked_at.as_deref().unwrap_or("in transit");
                    ui.label(format!("Location: {location}"));

                    let fill = ship.cargo_fill(cargo);
                    ui.label(format!(
                        "Cargo: {:.0}% ({}/{})",
                        fill * 100.0,
                        ShipVisual::cargo_used(cargo),
                        ship.cargo_capacity
                    ));
                    ui.label(format!("Fuel: {:.0}%", ship.fuel * 100.0));

                    ui.horizontal(|ui| {
                        if ui.button("Travel to...").clicked() {
                            ui_state.travel_picker_for = Some(entity);
                        }
                        if ui.button("Cargo").clicked() {
                            ui_state.cargo_window_for = Some(entity);
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

// ---------------------------------------------------------------------------
// Travel destination picker
// ---------------------------------------------------------------------------

/// One selectable destination in the picker.
#[derive(Clone, Debug)]
pub struct Destination {
    pub name: String,
    pub position: Vec2,
}

/// Collect all known destinations (planets + stations) into a de-duplicated,
/// stably-ordered list. The `OrbitalBodyVisual` / `StationVisual` entities
/// hold their current world position via `GlobalTransform`.
pub fn collect_destinations(
    bodies: &[(String, OrbitalKind, Vec2)],
    stations: &[(String, Vec2)],
) -> Vec<Destination> {
    let mut out: Vec<Destination> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (name, kind, pos) in bodies {
        if *kind == OrbitalKind::Star {
            continue;
        }
        if seen.insert(name.clone()) {
            out.push(Destination {
                name: name.clone(),
                position: *pos,
            });
        }
    }
    for (name, pos) in stations {
        if seen.insert(name.clone()) {
            out.push(Destination {
                name: name.clone(),
                position: *pos,
            });
        }
    }
    out
}

/// System: modal-style egui window listing destinations. Clicking one
/// dispatches a travel order on the selected ship.
#[allow(clippy::too_many_arguments)]
pub fn travel_picker_panel(
    mut contexts: EguiContexts,
    view: Res<ViewMode>,
    mut ui_state: ResMut<FleetUiState>,
    bodies_q: Query<(&OrbitalBodyVisual, &GlobalTransform)>,
    stations_q: Query<(&StationVisual, &GlobalTransform)>,
    mut ships_q: Query<(Entity, &mut ShipVisual)>,
    mut commands: Commands,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if *view != ViewMode::System {
        return;
    }
    let Some(target_ship) = ui_state.travel_picker_for else {
        return;
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Build destination list from current world transforms.
    let bodies: Vec<(String, OrbitalKind, Vec2)> = bodies_q
        .iter()
        .map(|(b, gxf)| (b.name.clone(), b.body_kind, gxf.translation().truncate()))
        .collect();
    let stations: Vec<(String, Vec2)> = stations_q
        .iter()
        .map(|(s, gxf)| (s.name.clone(), gxf.translation().truncate()))
        .collect();
    let destinations = collect_destinations(&bodies, &stations);

    // Find the ship's current name (for the window title).
    let ship_name = ships_q
        .iter()
        .find(|(e, _)| *e == target_ship)
        .map(|(_, s)| s.name.clone())
        .unwrap_or_else(|| "Ship".to_string());

    let mut close = false;
    let mut chosen: Option<Destination> = None;

    egui::Window::new(format!("Travel: {ship_name}"))
        .collapsible(false)
        .resizable(false)
        .default_width(260.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label("Select a destination:");
            ui.separator();

            if destinations.is_empty() {
                ui.label("(no destinations known)");
            } else {
                for d in &destinations {
                    if ui.button(&d.name).clicked() {
                        chosen = Some(d.clone());
                    }
                }
            }
            ui.separator();
            if ui.button("Close").clicked() {
                close = true;
            }
        });

    if let Some(dest) = chosen {
        // Dispatch travel: add a TravelTarget and mark ship "in transit".
        if let Some((_, mut ship)) = ships_q.iter_mut().find(|(e, _)| *e == target_ship) {
            let from = ship.position;
            ship.docked_at = None;
            ship.color = Color::srgb(1.0, 0.65, 0.3);
            commands.entity(target_ship).insert(TravelTarget::new(
                from,
                dest.position,
                dest.name.clone(),
                DEFAULT_TRAVEL_SPEED,
            ));
            info!("Ship {ship_name} dispatched to {}", dest.name);
        }
        close = true;
    }

    if close {
        ui_state.travel_picker_for = None;
    }

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

// ---------------------------------------------------------------------------
// Cargo transfer window
// ---------------------------------------------------------------------------

/// Transfer `delta` of `item` between ship and station. `to_ship == true`
/// means station -> ship, false means ship -> station. Returns the quantity
/// actually moved.
pub fn transfer(
    cargo: &mut ShipCargo,
    storage: &mut StationStorage,
    ship_capacity: u32,
    item: ItemType,
    delta: u32,
    to_ship: bool,
) -> u32 {
    if delta == 0 {
        return 0;
    }
    if to_ship {
        // Respect ship capacity.
        let used = ShipVisual::cargo_used(cargo);
        let free = ship_capacity.saturating_sub(used);
        let available = storage.get(item);
        let moved = delta.min(free).min(available);
        if moved > 0 {
            storage.remove(item, moved);
            cargo.add(item, moved);
        }
        moved
    } else {
        let available = cargo.get(item);
        let moved = delta.min(available);
        if moved > 0 {
            cargo.remove(item, moved);
            storage.add(item, moved);
        }
        moved
    }
}

/// System: egui window showing ship cargo on the left and station storage on
/// the right, with transfer buttons between them.
#[allow(clippy::too_many_arguments)]
pub fn cargo_window(
    mut contexts: EguiContexts,
    view: Res<ViewMode>,
    mut ui_state: ResMut<FleetUiState>,
    mut ships_q: Query<(Entity, &ShipVisual, &mut ShipCargo)>,
    mut stations_q: Query<(&StationVisual, &mut StationStorage)>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if *view != ViewMode::System {
        return;
    }
    let Some(target_ship) = ui_state.cargo_window_for else {
        return;
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Pick the first station as the cargo-transfer partner for now — the
    // simulation agent will add proper "which station am I docked at"
    // resolution later.
    let Some((_, mut storage)) = stations_q.iter_mut().next() else {
        ui_state.cargo_window_for = None;
        return;
    };

    let Some((_, ship_info, mut cargo)) = ships_q.iter_mut().find(|(e, _, _)| *e == target_ship)
    else {
        ui_state.cargo_window_for = None;
        return;
    };

    let ship_name = ship_info.name.clone();
    let ship_capacity = ship_info.cargo_capacity;
    let docked_label = ship_info
        .docked_at
        .as_deref()
        .unwrap_or("in transit")
        .to_string();
    let used = ShipVisual::cargo_used(&cargo);

    let mut close = false;

    egui::Window::new(format!("Cargo: {ship_name}"))
        .collapsible(true)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.label(format!("Status: {docked_label}"));
            ui.label(format!("Hold: {used} / {ship_capacity}"));
            ui.separator();

            ui.columns(2, |cols| {
                cols[0].label(egui::RichText::new("Ship").strong());
                cols[1].label(egui::RichText::new("Station").strong());

                for item in ALL_ITEMS {
                    let ship_q = cargo.get(*item);
                    let station_q = storage.get(*item);

                    cols[0].horizontal(|ui| {
                        ui.label(format!("{item}: {ship_q}"));
                        if ui
                            .small_button("->")
                            .on_hover_text("Transfer to station")
                            .clicked()
                        {
                            transfer(&mut cargo, &mut storage, ship_capacity, *item, 1, false);
                        }
                    });

                    cols[1].horizontal(|ui| {
                        if ui
                            .small_button("<-")
                            .on_hover_text("Transfer to ship")
                            .clicked()
                        {
                            transfer(&mut cargo, &mut storage, ship_capacity, *item, 1, true);
                        }
                        ui.label(format!("{item}: {station_q}"));
                    });
                }
            });

            ui.separator();
            if ui.button("Close").clicked() {
                close = true;
            }
        });

    if close {
        ui_state.cargo_window_for = None;
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
    fn travel_target_interpolates_linearly() {
        let mut t = TravelTarget::new(Vec2::ZERO, Vec2::new(100.0, 0.0), "Dest".into(), 0.25);
        // progress starts at 0
        assert!((t.current_position() - Vec2::ZERO).length() < 1e-4);

        // advance 2 seconds at speed 0.25 = progress 0.5
        let arrived = t.advance(2.0);
        assert!(!arrived);
        assert!((t.progress - 0.5).abs() < 1e-4);
        assert!((t.current_position().x - 50.0).abs() < 1e-3);

        // another 2 seconds should arrive and clamp
        let arrived = t.advance(2.0);
        assert!(arrived);
        assert!((t.progress - 1.0).abs() < 1e-4);
        assert!((t.current_position() - Vec2::new(100.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn travel_target_progress_clamps_to_one() {
        let mut t = TravelTarget::new(Vec2::ZERO, Vec2::new(10.0, 0.0), "D".into(), 5.0);
        // Huge dt — progress should clamp, not run past 1.0.
        let arrived = t.advance(100.0);
        assert!(arrived);
        assert!((t.progress - 1.0).abs() < 1e-6);
        let p = t.current_position();
        assert!((p - Vec2::new(10.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn ship_position_interpolation_midpoint() {
        let t = TravelTarget {
            from: Vec2::new(-50.0, 20.0),
            to: Vec2::new(50.0, 80.0),
            progress: 0.5,
            speed: 0.1,
            destination_name: "X".into(),
        };
        let p = t.current_position();
        assert!((p.x - 0.0).abs() < 1e-4);
        assert!((p.y - 50.0).abs() < 1e-4);
    }

    #[test]
    fn destinations_unique_and_exclude_star() {
        let bodies = vec![
            ("Sol".into(), OrbitalKind::Star, Vec2::ZERO),
            ("Home".into(), OrbitalKind::Planet, Vec2::new(100.0, 0.0)),
            ("Home".into(), OrbitalKind::Planet, Vec2::new(100.0, 0.0)), // dup
            ("Outer".into(), OrbitalKind::Planet, Vec2::new(500.0, 0.0)),
        ];
        let stations = vec![("Home Station".into(), Vec2::new(130.0, 0.0))];

        let dests = collect_destinations(&bodies, &stations);
        let names: Vec<&str> = dests.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["Home", "Outer", "Home Station"]);
    }

    #[test]
    fn destinations_station_and_planet_same_name_dedupes() {
        let bodies = vec![("Home".into(), OrbitalKind::Planet, Vec2::new(100.0, 0.0))];
        let stations = vec![("Home".into(), Vec2::new(130.0, 0.0))]; // collides
        let dests = collect_destinations(&bodies, &stations);
        assert_eq!(dests.len(), 1);
    }

    #[test]
    fn ship_cargo_add_and_remove() {
        let mut c = ShipCargo::default();
        c.add(ItemType::CopperOre, 10);
        c.add(ItemType::CopperOre, 5);
        assert_eq!(c.get(ItemType::CopperOre), 15);

        let removed = c.remove(ItemType::CopperOre, 7);
        assert_eq!(removed, 7);
        assert_eq!(c.get(ItemType::CopperOre), 8);

        // Remove more than present — returns only what was there.
        let removed = c.remove(ItemType::CopperOre, 100);
        assert_eq!(removed, 8);
        assert_eq!(c.get(ItemType::CopperOre), 0);
    }

    #[test]
    fn transfer_respects_capacity() {
        let mut cargo = ShipCargo::default();
        let mut storage = StationStorage::default();
        storage.add(ItemType::IronOre, 50);

        // capacity 10 — station has 50, we ask for 20
        let moved = transfer(&mut cargo, &mut storage, 10, ItemType::IronOre, 20, true);
        assert_eq!(moved, 10);
        assert_eq!(cargo.get(ItemType::IronOre), 10);
        assert_eq!(storage.get(ItemType::IronOre), 40);

        // Another transfer to ship: cargo already full.
        let moved = transfer(&mut cargo, &mut storage, 10, ItemType::IronOre, 5, true);
        assert_eq!(moved, 0);
    }

    #[test]
    fn transfer_ship_to_station_limited_by_cargo() {
        let mut cargo = ShipCargo::default();
        cargo.add(ItemType::CopperIngot, 3);
        let mut storage = StationStorage::default();

        let moved = transfer(
            &mut cargo,
            &mut storage,
            100,
            ItemType::CopperIngot,
            10,
            false,
        );
        assert_eq!(moved, 3);
        assert_eq!(cargo.get(ItemType::CopperIngot), 0);
        assert_eq!(storage.get(ItemType::CopperIngot), 3);
    }

    #[test]
    fn cargo_fill_percentage() {
        let ship = ShipVisual {
            name: "S".into(),
            position: Vec2::ZERO,
            color: Color::WHITE,
            cargo_capacity: 100,
            fuel: 1.0,
            docked_at: None,
        };
        let mut c = ShipCargo::default();
        c.add(ItemType::IronOre, 25);
        c.add(ItemType::CopperOre, 25);
        assert!((ship.cargo_fill(&c) - 0.5).abs() < 1e-4);
    }
}
