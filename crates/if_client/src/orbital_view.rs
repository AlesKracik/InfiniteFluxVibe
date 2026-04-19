// orbital_view.rs: Solar-system (orbital) view — a zoomed-out map of a star
// with orbiting planets. The player toggles between Surface and System view
// with the `M` hotkey.
//
// Placeholder types (OrbitalBodyVisual, OrbitalKind) live here so the client
// can render without depending on the simulation-side celestial body types
// that are being added in parallel. The orchestrator will wire the real
// types through once both sides are stable — at that point
// `spawn_system_visuals` reads the shared component/resource instead of
// using the hard-coded default formation.
//
// Layout conventions:
//   * System origin (star) is at (0, 0) in world space.
//   * Orbit lines render at Z=0.
//   * Body sprites render at Z=0.5.
//   * Labels are drawn by egui (no Z).
//
// Visibility: every visual entity carries either the `SurfaceVisual` or
// `SystemVisual` marker. A single system flips `Visibility::Visible` /
// `Visibility::Hidden` when the view mode changes.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use crate::camera::GameCamera;
use crate::ui_panels::EguiWantsPointer;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Which view mode the client is showing.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Surface,
    System,
    Galaxy,
}

impl ViewMode {
    /// Cycle through the view modes: Surface -> System -> Galaxy -> Surface.
    #[allow(dead_code)] // used by tests + future plumbing
    pub fn toggle(self) -> Self {
        match self {
            ViewMode::Surface => ViewMode::System,
            ViewMode::System => ViewMode::Galaxy,
            ViewMode::Galaxy => ViewMode::Surface,
        }
    }
}

/// Saved camera state for each view so we can restore it on toggle.
#[derive(Resource, Clone, Debug)]
pub struct SavedCameras {
    pub surface: CameraState,
    pub system: CameraState,
    pub galaxy: CameraState,
    /// Has the system view ever been shown? If not, we'll initialize the
    /// system camera on first entry rather than using uninitialized state.
    pub system_initialized: bool,
    /// Has the galaxy view ever been shown? Same rationale as above.
    pub galaxy_initialized: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraState {
    pub translation: Vec3,
    pub scale: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            scale: 1.0,
        }
    }
}

impl Default for SavedCameras {
    fn default() -> Self {
        Self {
            surface: CameraState::default(),
            system: CameraState {
                translation: Vec3::ZERO,
                // At scale 1.5 on a 1280-wide window, the viewport spans
                // ~1920 world units — wide enough to show the whole
                // placeholder system (outer orbit at r=500) with margin.
                scale: 1.5,
            },
            galaxy: CameraState {
                translation: Vec3::ZERO,
                // Galaxy-scale layout uses coordinates in the low-hundreds;
                // scale 1.0 keeps the whole map comfortably visible.
                scale: 1.0,
            },
            system_initialized: false,
            galaxy_initialized: false,
        }
    }
}

/// The currently-focused celestial body. Placeholder until the simulation
/// agent lands a shared type; orchestrator will replace this later.
#[derive(Resource, Default, Clone, Debug)]
pub struct CurrentBody {
    /// Name of the body whose surface the player is viewing. `None` means the
    /// default / starting body.
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marker: an entity that belongs to the surface view. Hidden in System mode.
#[derive(Component)]
pub struct SurfaceVisual;

/// Marker: an entity that belongs to the system view. Hidden in Surface mode.
#[derive(Component)]
pub struct SystemVisual;

/// Orbital body — placeholder visual data. Once the simulation agent's
/// `CelestialBody` type is stable, we'll read from that instead.
#[derive(Component, Clone, Debug)]
pub struct OrbitalBodyVisual {
    pub name: String,
    pub body_kind: OrbitalKind,
    pub orbit_radius: f32,
    pub orbit_angle: f32,
}

/// Visual/physical category of an orbital body.
///
/// `Moon` and `Asteroid` are placeholders for the final celestial-body
/// taxonomy coming from the simulation agent — the client is ready to
/// render them, the simulation just hasn't populated them yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum OrbitalKind {
    Star,
    Planet,
    Moon,
    Asteroid,
}

/// One segment of an orbit line — used so we can query them as a group when
/// we need to hide/show or despawn them.
#[derive(Component)]
pub struct OrbitLineSegment;

// ---------------------------------------------------------------------------
// Math helpers (kept free-standing so they're testable without Bevy App)
// ---------------------------------------------------------------------------

/// Compute the world position of a body on a circular orbit.
///
/// `radius` is in world units; `angle` is in radians. The returned point is
/// relative to the system origin (the star at (0, 0)).
pub fn orbital_position(radius: f32, angle: f32) -> Vec2 {
    Vec2::new(radius * angle.cos(), radius * angle.sin())
}

/// Angular velocity (radians / second) for a body at a given orbit radius.
///
/// This is a game-feel approximation — not real physics. Inner bodies sweep
/// faster, outer bodies slow down. We avoid dividing by zero for the star
/// at radius 0.
pub fn angular_velocity_for_radius(radius: f32) -> f32 {
    if radius <= f32::EPSILON {
        0.0
    } else {
        // Tuned so the innermost planet (radius 100) completes ~one revolution
        // in ~60 seconds: 0.1 rad/s.
        1.0 / radius.sqrt()
    }
}

/// Visual radius (sprite half-size) for each body kind.
pub fn visual_radius(kind: OrbitalKind) -> f32 {
    match kind {
        OrbitalKind::Star => 40.0,
        OrbitalKind::Planet => 18.0,
        OrbitalKind::Moon => 8.0,
        OrbitalKind::Asteroid => 4.0,
    }
}

/// Color for each body kind. Picked so they read clearly against a dark
/// background.
pub fn visual_color(kind: OrbitalKind) -> Color {
    match kind {
        OrbitalKind::Star => Color::srgb(1.0, 0.85, 0.3),
        OrbitalKind::Planet => Color::srgb(0.5, 0.7, 1.0),
        OrbitalKind::Moon => Color::srgb(0.8, 0.8, 0.85),
        OrbitalKind::Asteroid => Color::srgb(0.6, 0.5, 0.4),
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Number of small sprites used to approximate each orbit circle. 64 is
/// smooth enough at typical zoom without being wasteful.
const ORBIT_SEGMENTS: usize = 64;

/// Z layers for system-view rendering.
const Z_ORBIT_LINE: f32 = 0.0;
const Z_BODY: f32 = 0.5;

/// Default radii for the five-body placeholder system.
const PLACEHOLDER_ORBITS: &[(f32, &str, [f32; 3])] = &[
    (100.0, "Mercurius", [0.85, 0.55, 0.35]),
    (200.0, "Verdant", [0.35, 0.75, 0.45]),
    (350.0, "Oceanus", [0.35, 0.55, 0.95]),
    (500.0, "Frigidia", [0.75, 0.85, 0.95]),
];

/// Startup system: spawn the star, four planets, and their orbit lines.
///
/// These entities are always present but hidden with `Visibility::Hidden`
/// until the player toggles to System view.
pub fn spawn_system_visuals(mut commands: Commands) {
    // --- Star at center ---
    commands.spawn((
        Sprite {
            color: visual_color(OrbitalKind::Star),
            custom_size: Some(Vec2::splat(visual_radius(OrbitalKind::Star) * 2.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, Z_BODY),
        Visibility::Hidden,
        OrbitalBodyVisual {
            name: "Sol".to_string(),
            body_kind: OrbitalKind::Star,
            orbit_radius: 0.0,
            orbit_angle: 0.0,
        },
        SystemVisual,
    ));

    // --- Planets and their orbit circles ---
    for (idx, &(radius, name, color)) in PLACEHOLDER_ORBITS.iter().enumerate() {
        // Stagger starting angles so the planets don't line up.
        let start_angle = (idx as f32) * 0.9;
        let pos = orbital_position(radius, start_angle);

        commands.spawn((
            Sprite {
                color: Color::srgb(color[0], color[1], color[2]),
                custom_size: Some(Vec2::splat(visual_radius(OrbitalKind::Planet) * 2.0)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, Z_BODY),
            Visibility::Hidden,
            OrbitalBodyVisual {
                name: name.to_string(),
                body_kind: OrbitalKind::Planet,
                orbit_radius: radius,
                orbit_angle: start_angle,
            },
            SystemVisual,
        ));

        // Orbit line: a ring of small sprites. Cheap and works without
        // having to wire in a custom mesh. The segments are static, so we
        // only spawn them once.
        spawn_orbit_ring(&mut commands, radius);
    }
}

/// Spawn `ORBIT_SEGMENTS` tiny square sprites arranged on a circle of
/// `radius`. This is a stand-in for a proper line mesh — good enough for a
/// first pass. Segments are hidden by default and only shown in System view.
fn spawn_orbit_ring(commands: &mut Commands, radius: f32) {
    for i in 0..ORBIT_SEGMENTS {
        let angle = (i as f32) * std::f32::consts::TAU / (ORBIT_SEGMENTS as f32);
        let pos = orbital_position(radius, angle);
        commands.spawn((
            Sprite {
                color: Color::srgba(0.4, 0.4, 0.5, 0.6),
                custom_size: Some(Vec2::splat(2.0)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, Z_ORBIT_LINE),
            Visibility::Hidden,
            OrbitLineSegment,
            SystemVisual,
        ));
    }
}

/// System: advance each orbiting body's angle by `delta * angular_velocity`.
///
/// The star (radius 0) stays put. We do not update Transform here — that's
/// the job of `update_orbital_positions`, which only runs in System view.
pub fn animate_orbits(time: Res<Time>, mut bodies_q: Query<&mut OrbitalBodyVisual>) {
    let dt = time.delta_secs();
    for mut body in &mut bodies_q {
        let omega = angular_velocity_for_radius(body.orbit_radius);
        body.orbit_angle = (body.orbit_angle + omega * dt).rem_euclid(std::f32::consts::TAU);
    }
}

/// System: sync body Transforms with their `orbit_angle`/`orbit_radius`.
///
/// Split from `animate_orbits` so we can still update positions when the
/// game is paused (e.g. view refresh) without advancing the simulation.
pub fn update_orbital_positions(mut bodies_q: Query<(&OrbitalBodyVisual, &mut Transform)>) {
    for (body, mut transform) in &mut bodies_q {
        let pos = orbital_position(body.orbit_radius, body.orbit_angle);
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
    }
}

/// Cycle through Surface -> System -> Galaxy -> Surface, saving the outgoing
/// camera state and restoring the incoming view's saved state.
///
/// Shared by the `M` hotkey and the "Galaxy Map" egui button so the two
/// paths behave identically.
pub fn toggle_view_mode(
    view: &mut ViewMode,
    saved: &mut SavedCameras,
    transform: &mut Transform,
    projection: &mut Projection,
) {
    let current_scale = match *projection {
        Projection::Orthographic(ref ortho) => ortho.scale,
        _ => 1.0,
    };
    let current_state = CameraState {
        translation: transform.translation,
        scale: current_scale,
    };

    match *view {
        ViewMode::Surface => {
            saved.surface = current_state;
            if !saved.system_initialized {
                saved.system.translation = Vec3::ZERO;
                saved.system_initialized = true;
            }
            *view = ViewMode::System;
            transform.translation = saved.system.translation;
            if let Projection::Orthographic(ref mut ortho) = *projection {
                ortho.scale = saved.system.scale;
            }
        }
        ViewMode::System => {
            saved.system = current_state;
            saved.system_initialized = true;
            if !saved.galaxy_initialized {
                saved.galaxy.translation = Vec3::ZERO;
                saved.galaxy_initialized = true;
            }
            *view = ViewMode::Galaxy;
            transform.translation = saved.galaxy.translation;
            if let Projection::Orthographic(ref mut ortho) = *projection {
                ortho.scale = saved.galaxy.scale;
            }
        }
        ViewMode::Galaxy => {
            saved.galaxy = current_state;
            saved.galaxy_initialized = true;
            *view = ViewMode::Surface;
            transform.translation = saved.surface.translation;
            if let Projection::Orthographic(ref mut ortho) = *projection {
                ortho.scale = saved.surface.scale;
            }
        }
    }
}

/// System: `M` toggles view mode; `Esc` does NOT — existing Esc behavior is
/// deselect-building, and we want to keep it predictable.
pub fn view_mode_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<ViewMode>,
    mut saved: ResMut<SavedCameras>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyM) {
        return;
    }

    let Ok((mut transform, mut projection)) = camera_q.single_mut() else {
        return;
    };

    toggle_view_mode(&mut view, &mut saved, &mut transform, &mut projection);
}

/// System: apply the current ViewMode to entity Visibility.
///
/// Runs only on change to avoid re-touching every sprite every frame. Galaxy
/// mode hides both Surface and System entities; the Galaxy-specific visibility
/// for `GalaxyVisual` entities lives in `galaxy_view::apply_galaxy_visibility`.
pub fn apply_view_visibility(
    view: Res<ViewMode>,
    mut surface_q: Query<&mut Visibility, (With<SurfaceVisual>, Without<SystemVisual>)>,
    mut system_q: Query<&mut Visibility, (With<SystemVisual>, Without<SurfaceVisual>)>,
) {
    if !view.is_changed() {
        return;
    }

    let (surface_vis, system_vis) = match *view {
        ViewMode::Surface => (Visibility::Visible, Visibility::Hidden),
        ViewMode::System => (Visibility::Hidden, Visibility::Visible),
        ViewMode::Galaxy => (Visibility::Hidden, Visibility::Hidden),
    };

    for mut v in &mut surface_q {
        *v = surface_vis;
    }
    for mut v in &mut system_q {
        *v = system_vis;
    }
}

/// System: auto-tag any entity that has a `Sprite` or `Text2d` but no view
/// marker yet as `SurfaceVisual`. Runs every frame so newly-spawned
/// buildings, labels, and transport visuals get hidden correctly when the
/// player is in System view.
///
/// This lets us avoid sprinkling `SurfaceVisual` into every spawn site in
/// placement.rs, grid_renderer.rs, etc. The cost is tiny — the query only
/// sees entities the frame they're spawned, then they're filtered out.
#[allow(clippy::type_complexity)]
pub fn auto_tag_surface_visuals(
    mut commands: Commands,
    view: Res<ViewMode>,
    sprites_q: Query<Entity, (With<Sprite>, Without<SurfaceVisual>, Without<SystemVisual>)>,
    texts_q: Query<Entity, (With<Text2d>, Without<SurfaceVisual>, Without<SystemVisual>)>,
) {
    let hide_now = *view != ViewMode::Surface;
    for entity in &sprites_q {
        let mut e = commands.entity(entity);
        e.insert(SurfaceVisual);
        if hide_now {
            e.insert(Visibility::Hidden);
        }
    }
    for entity in &texts_q {
        let mut e = commands.entity(entity);
        e.insert(SurfaceVisual);
        if hide_now {
            e.insert(Visibility::Hidden);
        }
    }
}

// ---------------------------------------------------------------------------
// egui overlay panels
// ---------------------------------------------------------------------------

/// System: floating labels that track each body in screen space. Only
/// renders in System view.
pub fn system_body_labels(
    mut contexts: EguiContexts,
    view: Res<ViewMode>,
    bodies_q: Query<(&OrbitalBodyVisual, &GlobalTransform)>,
    camera_q: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    window_q: Query<&Window, With<PrimaryWindow>>,
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
    let Ok((camera, cam_xf)) = camera_q.single() else {
        return;
    };
    let Ok(window) = window_q.single() else {
        return;
    };

    // Physical window height in logical pixels; egui works in logical pixels
    // and Bevy's viewport projection returns logical pixels as well.
    let _win_h = window.height();

    egui::Area::new(egui::Id::new("system_body_labels"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            for (body, gxf) in &bodies_q {
                let world = gxf.translation();
                let Ok(screen) = camera.world_to_viewport(cam_xf, world) else {
                    continue;
                };
                // Offset the label above the sprite.
                let offset = visual_radius(body.body_kind) + 8.0;
                let pos = egui::pos2(screen.x, screen.y - offset);
                ui.painter().text(
                    pos,
                    egui::Align2::CENTER_BOTTOM,
                    &body.name,
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_rgb(220, 230, 255),
                );
            }
        });
}

/// System: right-side info panel listing planets. Only shown in System view.
/// Clicking a planet focuses the camera and (for planets) switches to
/// Surface view for that body.
#[allow(clippy::too_many_arguments)]
pub fn system_info_panel(
    mut contexts: EguiContexts,
    mut view: ResMut<ViewMode>,
    mut saved: ResMut<SavedCameras>,
    mut current_body: ResMut<CurrentBody>,
    bodies_q: Query<&OrbitalBodyVisual>,
    mut camera_q: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
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

    egui::SidePanel::right("system_info_panel")
        .resizable(false)
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.heading("System Map");
            ui.separator();
            ui.label("Click a body to visit.");
            ui.add_space(4.0);

            // Stable ordering: star first, then planets by radius.
            let mut sorted: Vec<&OrbitalBodyVisual> = bodies_q.iter().collect();
            sorted.sort_by(|a, b| {
                a.orbit_radius
                    .partial_cmp(&b.orbit_radius)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for body in sorted {
                let label = match body.body_kind {
                    OrbitalKind::Star => format!("* {} (star)", body.name),
                    OrbitalKind::Planet => {
                        format!("o {} (r={:.0})", body.name, body.orbit_radius)
                    }
                    OrbitalKind::Moon => format!(". {} (moon)", body.name),
                    OrbitalKind::Asteroid => format!("~ {} (asteroid)", body.name),
                };
                if ui.button(label).clicked() && body.body_kind == OrbitalKind::Planet {
                    // Switch back to Surface for this body. The simulation
                    // agent will hook `CurrentBody` up later; for now we
                    // just record the name and flip the mode.
                    current_body.name = Some(body.name.clone());

                    if let Ok((mut xf, mut proj)) = camera_q.single_mut() {
                        // Force toggle to Surface (we know we're in System
                        // here, but use the helper so camera state is
                        // preserved consistently).
                        toggle_view_mode(&mut view, &mut saved, &mut xf, &mut proj);
                    }
                }
            }
        });

    egui_wants.0 = egui_wants.0 || ctx.wants_pointer_input();
}

/// System: click-in-world detection for planets. If the player clicks a
/// planet sprite in System view (not on the egui panel), switch to Surface.
#[allow(clippy::too_many_arguments)]
pub fn system_click_to_visit(
    mouse: Res<ButtonInput<MouseButton>>,
    egui_wants: Res<EguiWantsPointer>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    bodies_q: Query<(&OrbitalBodyVisual, &GlobalTransform)>,
    mut view: ResMut<ViewMode>,
    mut saved: ResMut<SavedCameras>,
    mut current_body: ResMut<CurrentBody>,
    mut camera_q: Query<
        (&Camera, &GlobalTransform, &mut Transform, &mut Projection),
        With<GameCamera>,
    >,
) {
    if *view != ViewMode::System {
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
    let Ok((camera, cam_gxf, mut cam_xf, mut cam_proj)) = camera_q.single_mut() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(world) = camera.viewport_to_world_2d(cam_gxf, cursor) else {
        return;
    };

    // Find the nearest planet within its visual radius.
    let mut hit: Option<&OrbitalBodyVisual> = None;
    for (body, gxf) in &bodies_q {
        if body.body_kind != OrbitalKind::Planet {
            continue;
        }
        let p = gxf.translation().truncate();
        if (p - world).length() <= visual_radius(body.body_kind) {
            hit = Some(body);
            break;
        }
    }
    let Some(body) = hit else {
        return;
    };

    current_body.name = Some(body.name.clone());
    toggle_view_mode(&mut view, &mut saved, &mut cam_xf, &mut cam_proj);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_mode_toggle_cycles() {
        let mut m = ViewMode::default();
        assert_eq!(m, ViewMode::Surface);
        m = m.toggle();
        assert_eq!(m, ViewMode::System);
        m = m.toggle();
        assert_eq!(m, ViewMode::Galaxy);
        m = m.toggle();
        assert_eq!(m, ViewMode::Surface);
    }

    #[test]
    fn orbital_position_zero_angle() {
        let p = orbital_position(100.0, 0.0);
        assert!((p.x - 100.0).abs() < 1e-4);
        assert!(p.y.abs() < 1e-4);
    }

    #[test]
    fn orbital_position_quarter_turn() {
        let p = orbital_position(200.0, std::f32::consts::FRAC_PI_2);
        assert!(p.x.abs() < 1e-3, "x should be ~0, got {}", p.x);
        assert!((p.y - 200.0).abs() < 1e-3, "y should be ~200, got {}", p.y);
    }

    #[test]
    fn orbital_position_half_turn() {
        let p = orbital_position(350.0, std::f32::consts::PI);
        assert!((p.x + 350.0).abs() < 1e-3);
        assert!(p.y.abs() < 1e-3);
    }

    #[test]
    fn angular_velocity_zero_for_star() {
        assert_eq!(angular_velocity_for_radius(0.0), 0.0);
    }

    #[test]
    fn angular_velocity_decreases_with_radius() {
        let inner = angular_velocity_for_radius(100.0);
        let outer = angular_velocity_for_radius(500.0);
        assert!(
            inner > outer,
            "inner ({inner}) should orbit faster than outer ({outer})"
        );
    }
}
