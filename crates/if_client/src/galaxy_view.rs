// galaxy_view.rs: Galaxy-scale view — a 2D map of star systems connected by
// warp lanes. Reached from System view via the `M` hotkey (Surface -> System
// -> Galaxy -> Surface).
//
// These types are CLIENT-SIDE placeholders while the simulation agent lands
// the authoritative `Galaxy` / `SystemDescriptor` / `WarpOrder` types in
// `if_world::galaxy`. The orchestrator will wire the two sides together once
// both are stable; until then, nothing in this module touches `if_world`.
//
// Rendering conventions:
//   * Galaxy origin is at (0, 0) in world space.
//   * Star positions span roughly -220..220 on both axes.
//   * Warp lanes render at Z=0.0 (behind stars).
//   * Stars render at Z=0.6.
//   * The active system highlight ring renders at Z=0.5 (under the star sprite).
//   * Every entity carries `GalaxyVisual`; visibility is managed by
//     `apply_galaxy_visibility`.
//
// Placeholder data: 8 systems laid out with a small fixed-seed PRNG, connected
// by warp lanes between each system and its two nearest neighbors.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use crate::camera::GameCamera;
use crate::orbital_view::ViewMode;
use crate::ui_panels::EguiWantsPointer;

// ---------------------------------------------------------------------------
// Placeholder component types (will be replaced by if_world::galaxy imports
// once the simulation agent lands them)
// ---------------------------------------------------------------------------

/// One visible star system on the galaxy map.
#[derive(Component, Clone, Debug)]
pub struct SystemVisualGalaxy {
    pub name: String,
    pub position: Vec2,
    pub index: usize,
    /// True if this is the player's currently-active system (i.e. the one
    /// whose Surface / System view they last entered).
    pub is_active: bool,
}

/// A warp lane connecting two systems. Rendered as a thin line made of small
/// sprites, like the orbit rings in `orbital_view.rs`. The fields aren't read
/// yet from Bevy systems — they'll be consumed once the simulation agent's
/// galaxy data wires up — but they're part of the component's public API so
/// future lookup code (hover tooltips, travel-time estimates) can use them.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)]
pub struct WarpLaneVisual {
    pub from: usize,
    pub to: usize,
}

/// Marker: entity belongs to the Galaxy view. Hidden in Surface / System view.
#[derive(Component, Debug, Clone, Copy)]
pub struct GalaxyVisual;

/// Marker for the yellow ring drawn around the active system. One per active
/// system; re-targeted when the player warps.
#[derive(Component, Debug, Clone, Copy)]
pub struct ActiveSystemRing;

/// Resource: placeholder "which warp order was issued" log. Once the
/// simulation agent adds `WarpOrder`, we'll dispatch real events instead.
#[derive(Resource, Default, Debug)]
pub struct PendingWarp {
    pub target_index: Option<usize>,
}

/// Resource: egui state for the galaxy info panel — which system (if any) is
/// currently selected / hovered-over in the side panel or via click.
#[derive(Resource, Default, Debug)]
pub struct GalaxyUiState {
    pub selected_system: Option<usize>,
}

// ---------------------------------------------------------------------------
// Z layers and sizing
// ---------------------------------------------------------------------------

const Z_LANE: f32 = 0.0;
const Z_RING: f32 = 0.5;
const Z_STAR: f32 = 0.6;

const STAR_SIZE: f32 = 14.0;
const RING_SIZE: f32 = 24.0;
const LANE_SEGMENTS: usize = 24;
const LANE_SEGMENT_SIZE: f32 = 2.0;

/// Hit radius for clicking a star sprite on the galaxy map.
const STAR_CLICK_RADIUS: f32 = 12.0;

// ---------------------------------------------------------------------------
// Placeholder data
// ---------------------------------------------------------------------------

/// Number of placeholder star systems.
pub const PLACEHOLDER_SYSTEM_COUNT: usize = 8;

/// Names for the 8 placeholder systems. The first one ("Sol") is the active
/// system by default — matches the placeholder solar system in
/// `orbital_view.rs`.
const PLACEHOLDER_NAMES: [&str; PLACEHOLDER_SYSTEM_COUNT] = [
    "Sol", "Vega", "Altair", "Rigel", "Canopus", "Antares", "Procyon", "Deneb",
];

/// Tiny xorshift32 so we get reproducible placeholder positions without
/// pulling in a PRNG crate.
fn xorshift32(mut state: u32) -> (u32, u32) {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    (state, state)
}

fn rand_unit(state: &mut u32) -> f32 {
    let (next, _) = xorshift32(*state);
    *state = next;
    // Map to [-1.0, 1.0].
    (next as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Generate 8 placeholder system positions on a 2D plane using a fixed seed.
/// Stars are spaced so the whole galaxy fits in roughly a 440x440 square.
pub fn placeholder_system_positions() -> Vec<Vec2> {
    let mut state: u32 = 0xDEADBEEF;
    let mut out = Vec::with_capacity(PLACEHOLDER_SYSTEM_COUNT);

    // First system anchored at origin for a recognisable "home" position.
    out.push(Vec2::ZERO);

    for _ in 1..PLACEHOLDER_SYSTEM_COUNT {
        // Spread over roughly -200..200 on each axis. We also nudge any point
        // that lands too close to an existing one so clicks stay unambiguous.
        let mut pos = Vec2::new(rand_unit(&mut state) * 200.0, rand_unit(&mut state) * 200.0);
        for &existing in &out {
            if (pos - existing).length() < 60.0 {
                // Push it along the direction away from `existing`.
                let away = (pos - existing).normalize_or_zero() * 60.0;
                pos = existing + away;
            }
        }
        out.push(pos);
    }
    out
}

/// For each system, return the indices of its two nearest neighbors (excluding
/// itself). These form the galaxy's warp-lane graph. The returned edges are
/// unique — an edge `(a, b)` appears at most once, normalized so `a < b`.
pub fn placeholder_warp_lanes(positions: &[Vec2]) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for (i, p) in positions.iter().enumerate() {
        // Sort by distance, skip self.
        let mut dists: Vec<(usize, f32)> = positions
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(j, q)| (j, (p - q).length()))
            .collect();
        dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for (j, _) in dists.iter().take(2) {
            let (lo, hi) = if i < *j { (i, *j) } else { (*j, i) };
            if !edges.contains(&(lo, hi)) {
                edges.push((lo, hi));
            }
        }
    }
    edges
}

// ---------------------------------------------------------------------------
// Math helpers (free-standing so they're testable without a Bevy App)
// ---------------------------------------------------------------------------

/// Return the `n`-th endpoint on a line segment divided into `segments` parts,
/// inclusive of both ends. `n` in [0, segments].
pub fn lane_segment_position(from: Vec2, to: Vec2, segments: usize, n: usize) -> Vec2 {
    let t = if segments == 0 {
        0.0
    } else {
        n.min(segments) as f32 / segments as f32
    };
    from + (to - from) * t
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// Startup system: spawn stars, warp lanes, and the active-system ring.
///
/// All entities are hidden until the player enters Galaxy view.
pub fn spawn_galaxy_visuals(mut commands: Commands) {
    let positions = placeholder_system_positions();
    let lanes = placeholder_warp_lanes(&positions);

    // --- Warp lanes (drawn first so they sit behind stars) ---
    for (from_idx, to_idx) in lanes {
        let from = positions[from_idx];
        let to = positions[to_idx];
        // Each lane entity is a parent that owns a lane marker; the segment
        // sprites are siblings tagged with GalaxyVisual so they're hidden too.
        commands.spawn((
            WarpLaneVisual {
                from: from_idx,
                to: to_idx,
            },
            GalaxyVisual,
            // A dummy transform — the visible sprites are separate entities
            // below. Keeping one component entity per lane lets UI code
            // iterate lanes without iterating every sprite.
            Transform::default(),
            Visibility::Hidden,
        ));

        for n in 0..=LANE_SEGMENTS {
            let pos = lane_segment_position(from, to, LANE_SEGMENTS, n);
            commands.spawn((
                Sprite {
                    color: Color::srgba(0.4, 0.55, 0.8, 0.6),
                    custom_size: Some(Vec2::splat(LANE_SEGMENT_SIZE)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, Z_LANE),
                Visibility::Hidden,
                GalaxyVisual,
            ));
        }
    }

    // --- Stars ---
    for (idx, pos) in positions.iter().enumerate() {
        let name = PLACEHOLDER_NAMES[idx].to_string();
        let is_active = idx == 0;

        commands.spawn((
            Sprite {
                color: Color::srgb(1.0, 0.95, 0.55),
                custom_size: Some(Vec2::splat(STAR_SIZE)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, Z_STAR),
            Visibility::Hidden,
            SystemVisualGalaxy {
                name,
                position: *pos,
                index: idx,
                is_active,
            },
            GalaxyVisual,
        ));

        if is_active {
            commands.spawn((
                Sprite {
                    color: Color::srgba(1.0, 0.9, 0.3, 0.35),
                    custom_size: Some(Vec2::splat(RING_SIZE)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, Z_RING),
                Visibility::Hidden,
                ActiveSystemRing,
                GalaxyVisual,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Visibility
// ---------------------------------------------------------------------------

/// System: hide/show GalaxyVisual entities based on the current ViewMode.
///
/// Runs only on change to stay cheap. Paired with
/// `orbital_view::apply_view_visibility`, which handles the other two modes.
pub fn apply_galaxy_visibility(
    view: Res<ViewMode>,
    mut galaxy_q: Query<&mut Visibility, With<GalaxyVisual>>,
) {
    if !view.is_changed() {
        return;
    }

    let target = match *view {
        ViewMode::Galaxy => Visibility::Visible,
        _ => Visibility::Hidden,
    };
    for mut v in &mut galaxy_q {
        *v = target;
    }
}

/// System: keep the active-system ring's position in sync with whichever
/// system is currently marked active. This is cheap — there's usually exactly
/// one ring and a handful of stars.
pub fn sync_active_ring(
    stars_q: Query<&SystemVisualGalaxy>,
    mut ring_q: Query<&mut Transform, With<ActiveSystemRing>>,
) {
    let Some(active) = stars_q.iter().find(|s| s.is_active) else {
        return;
    };
    for mut xf in &mut ring_q {
        xf.translation.x = active.position.x;
        xf.translation.y = active.position.y;
    }
}

// ---------------------------------------------------------------------------
// Interaction: click a star to select it
// ---------------------------------------------------------------------------

/// System: left-click in Galaxy view selects the nearest star within
/// `STAR_CLICK_RADIUS`. Selection is the galaxy-panel's highlight target;
/// actual warping happens via the "Warp Here" button.
pub fn galaxy_click_to_select(
    mouse: Res<ButtonInput<MouseButton>>,
    view: Res<ViewMode>,
    egui_wants: Res<EguiWantsPointer>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    stars_q: Query<&SystemVisualGalaxy>,
    mut ui_state: ResMut<GalaxyUiState>,
) {
    if *view != ViewMode::Galaxy {
        return;
    }
    if egui_wants.0 {
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = window_q.single() else {
        return;
    };
    let Ok((camera, cam_gxf)) = camera_q.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(world) = camera.viewport_to_world_2d(cam_gxf, cursor) else {
        return;
    };

    let mut hit: Option<usize> = None;
    let mut best = f32::MAX;
    for star in &stars_q {
        let d = (star.position - world).length();
        if d < best && d <= STAR_CLICK_RADIUS {
            best = d;
            hit = Some(star.index);
        }
    }
    if let Some(idx) = hit {
        ui_state.selected_system = Some(idx);
    }
}

// ---------------------------------------------------------------------------
// egui: galaxy info panel
// ---------------------------------------------------------------------------

/// System: right-side panel listing known systems; shown only in Galaxy view.
/// Each entry has a "Visit" button that selects the system, and the selected
/// system additionally shows a "Warp Here" button.
#[allow(clippy::too_many_arguments)]
pub fn galaxy_info_panel(
    mut contexts: EguiContexts,
    view: Res<ViewMode>,
    stars_q: Query<&SystemVisualGalaxy>,
    mut ui_state: ResMut<GalaxyUiState>,
    mut pending: ResMut<PendingWarp>,
    mut egui_wants: ResMut<EguiWantsPointer>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    if *view != ViewMode::Galaxy {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Stable ordering by system index.
    let mut stars: Vec<&SystemVisualGalaxy> = stars_q.iter().collect();
    stars.sort_by_key(|s| s.index);

    let active = stars.iter().find(|s| s.is_active).map(|s| s.name.clone());

    egui::SidePanel::right("galaxy_info_panel")
        .resizable(false)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading("Galaxy Map");
            ui.separator();
            if let Some(name) = &active {
                ui.label(format!("Current system: {name}"));
            } else {
                ui.label("Current system: (none)");
            }
            ui.add_space(4.0);
            ui.separator();
            ui.label("Systems:");

            for star in &stars {
                let prefix = if star.is_active { "* " } else { "  " };
                let selected = ui_state.selected_system == Some(star.index);
                let label = format!("{prefix}{}", star.name);
                let button = if selected {
                    egui::Button::new(egui::RichText::new(label).strong())
                        .fill(egui::Color32::from_rgb(60, 80, 120))
                } else {
                    egui::Button::new(label)
                };
                if ui.add(button).clicked() {
                    ui_state.selected_system = Some(star.index);
                }
            }

            ui.separator();
            if let Some(sel_idx) = ui_state.selected_system {
                if let Some(sel) = stars.iter().find(|s| s.index == sel_idx) {
                    ui.label(format!("Selected: {}", sel.name));
                    ui.label(format!(
                        "Position: ({:.0}, {:.0})",
                        sel.position.x, sel.position.y
                    ));
                    ui.add_space(4.0);
                    if sel.is_active {
                        ui.label("(already here)");
                    } else if ui.button("Warp Here").clicked() {
                        pending.target_index = Some(sel_idx);
                        info!("Warp order: index {sel_idx} ({})", sel.name);
                    }
                }
            } else {
                ui.label("(click a star to select)");
            }
        });

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

/// System: process a pending warp order by flipping `is_active` on the target
/// system. Until the simulation agent wires a real `WarpOrder` message, this
/// is how we reflect the player's warp in the UI.
pub fn apply_pending_warp(
    mut pending: ResMut<PendingWarp>,
    mut stars_q: Query<&mut SystemVisualGalaxy>,
) {
    let Some(target) = pending.target_index.take() else {
        return;
    };
    for mut star in &mut stars_q {
        star.is_active = star.index == target;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_positions_count() {
        let positions = placeholder_system_positions();
        assert_eq!(positions.len(), PLACEHOLDER_SYSTEM_COUNT);
    }

    #[test]
    fn placeholder_positions_are_reproducible() {
        let a = placeholder_system_positions();
        let b = placeholder_system_positions();
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert!((pa - pb).length() < 1e-4);
        }
    }

    #[test]
    fn placeholder_positions_first_is_origin() {
        let positions = placeholder_system_positions();
        assert!(positions[0].length() < 1e-4);
    }

    #[test]
    fn placeholder_positions_are_separated() {
        let positions = placeholder_system_positions();
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let d = (positions[i] - positions[j]).length();
                assert!(
                    d > 1.0,
                    "systems {i} and {j} overlap: d={d} at {:?}/{:?}",
                    positions[i],
                    positions[j]
                );
            }
        }
    }

    #[test]
    fn placeholder_lanes_are_unique_and_valid() {
        let positions = placeholder_system_positions();
        let lanes = placeholder_warp_lanes(&positions);
        for (lo, hi) in &lanes {
            assert!(lo < hi);
            assert!(*hi < positions.len());
        }
        // No duplicates.
        let mut sorted = lanes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), lanes.len());
    }

    #[test]
    fn placeholder_lanes_connect_every_system() {
        let positions = placeholder_system_positions();
        let lanes = placeholder_warp_lanes(&positions);
        for i in 0..positions.len() {
            let has_edge = lanes.iter().any(|(a, b)| *a == i || *b == i);
            assert!(has_edge, "system {i} has no warp lane");
        }
    }

    #[test]
    fn lane_segment_endpoints() {
        let from = Vec2::new(0.0, 0.0);
        let to = Vec2::new(100.0, 0.0);

        let start = lane_segment_position(from, to, 10, 0);
        assert!((start - from).length() < 1e-4);

        let end = lane_segment_position(from, to, 10, 10);
        assert!((end - to).length() < 1e-4);

        let mid = lane_segment_position(from, to, 10, 5);
        assert!((mid - Vec2::new(50.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn lane_segment_zero_segments_collapses_to_from() {
        let p = lane_segment_position(Vec2::new(1.0, 2.0), Vec2::new(9.0, 9.0), 0, 3);
        assert!((p - Vec2::new(1.0, 2.0)).length() < 1e-4);
    }

    #[test]
    fn lane_segment_n_clamped_to_segments() {
        let from = Vec2::ZERO;
        let to = Vec2::new(10.0, 0.0);
        let over = lane_segment_position(from, to, 4, 999);
        // n clamps to `segments`, so we reach `to`.
        assert!((over - to).length() < 1e-4);
    }
}
