// camera.rs: 2D camera with pan and zoom controls.

use bevy::prelude::*;
use if_common::{DEFAULT_GRID_HEIGHT, DEFAULT_GRID_WIDTH, TILE_SIZE};

use crate::orbital_view::ViewMode;

/// Marker component so we can query for our game camera specifically.
/// Bevy might have other cameras (e.g., UI camera). This tag lets us
/// filter: `Query<..., With<GameCamera>>`.
#[derive(Component)]
pub struct GameCamera;

/// Camera movement speed in pixels per second.
const PAN_SPEED: f32 = 500.0;

/// How much each scroll tick changes the zoom.
const ZOOM_SPEED: f32 = 0.1;

/// Min/max zoom levels (OrthographicProjection scale) for Surface view.
/// Scale < 1.0 = zoomed in, scale > 1.0 = zoomed out.
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.0;

/// Min/max zoom levels for System (orbital) view — much wider range so the
/// player can fit a whole solar system on screen.
const SYSTEM_MIN_ZOOM: f32 = 0.05;
const SYSTEM_MAX_ZOOM: f32 = 5.0;

/// Startup system: spawns a 2D camera centered on the grid.
pub fn spawn_camera(mut commands: Commands) {
    // Calculate the center of the grid in world coordinates.
    let center_x = (DEFAULT_GRID_WIDTH as f32 * TILE_SIZE) / 2.0;
    let center_y = (DEFAULT_GRID_HEIGHT as f32 * TILE_SIZE) / 2.0;

    commands.spawn((
        // Camera2d automatically includes an OrthographicProjection
        // via Bevy's #[require] system. We don't need to add it manually.
        Camera2d,
        Transform::from_xyz(center_x, center_y, 0.0),
        GameCamera,
    ));
}

/// Update system: handles WASD/arrow panning and scroll wheel zoom.
///
/// Notice the function signature — Bevy automatically injects these parameters:
/// - `Query` finds entities matching the component filters
/// - `Res<Time>` gives us delta time for frame-rate independent movement
/// - `Res<ButtonInput<KeyCode>>` gives us keyboard state
/// - `EventReader` gives us mouse scroll events
///
/// In Bevy 0.18, OrthographicProjection is wrapped in the `Projection` enum.
/// We query `&mut Projection` and pattern match to get at the orthographic data.
pub fn camera_movement(
    mut camera_q: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scroll_events: MessageReader<bevy::input::mouse::MouseWheel>,
    view: Res<ViewMode>,
) {
    let Ok((mut transform, mut projection)) = camera_q.single_mut() else {
        return;
    };

    // --- Panning ---
    let mut direction = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    // Normalize so diagonal movement isn't faster than cardinal.
    let movement = direction.normalize_or_zero() * PAN_SPEED * time.delta_secs();
    transform.translation.x += movement.x;
    transform.translation.y += movement.y;

    // --- Zoom ---
    // Pattern match on the Projection enum to get the orthographic data.
    // `Projection` is an enum with variants like `Projection::Orthographic(...)`.
    // We destructure it to access the `scale` field.
    let Projection::Orthographic(ref mut ortho) = *projection else {
        return; // Not orthographic — shouldn't happen with Camera2d, but safe.
    };

    // Zoom range depends on view mode: surface is tighter, system is wider.
    let (min_zoom, max_zoom) = match *view {
        ViewMode::Surface => (MIN_ZOOM, MAX_ZOOM),
        ViewMode::System => (SYSTEM_MIN_ZOOM, SYSTEM_MAX_ZOOM),
    };
    for event in scroll_events.read() {
        ortho.scale -= event.y * ZOOM_SPEED;
        ortho.scale = ortho.scale.clamp(min_zoom, max_zoom);
    }
}
