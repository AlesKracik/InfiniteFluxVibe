// grid_renderer.rs: Renders the tile grid as colored sprites.
//
// Each tile becomes a Bevy entity with a Sprite and Transform.
// When tile data changes in the Grid resource, we update sprite colors.

use bevy::prelude::*;
use if_common::{GridPosition, TILE_SIZE};
use if_world::grid::Grid;

/// Marker component linking a sprite entity to its grid position.
/// We use this to look up the tile type when updating colors.
#[derive(Component)]
pub struct TileSprite;

/// Startup system: spawns one sprite entity per grid cell.
///
/// This reads the `Grid` resource (inserted by WorldPlugin) and creates
/// a colored quad for every tile. The `.after(spawn_grid)` ordering in
/// main.rs guarantees the Grid exists when this runs.
pub fn spawn_grid_visuals(mut commands: Commands, grid: Res<Grid>) {
    for y in 0..grid.height {
        for x in 0..grid.width {
            let pos = GridPosition::new(x, y);
            let world_pos = pos.to_world();

            // Look up the tile type to get the initial color.
            // We can safely unwrap here because x,y are within bounds.
            let tile = grid.get(x, y).expect("grid coordinates are in bounds");

            commands.spawn((
                // Sprite is Bevy's 2D quad. We set a custom size and color.
                Sprite {
                    color: tile.color(),
                    custom_size: Some(Vec2::splat(TILE_SIZE - 1.0)), // -1.0 for a 1px gap between tiles
                    ..default()
                },
                // Position in world space. Z=0.0 is the base layer.
                Transform::from_xyz(world_pos.x, world_pos.y, 0.0),
                // Our custom components for identification and lookup.
                pos,
                TileSprite,
            ));
        }
    }
}

/// Update system: syncs sprite colors with the current Grid state.
///
/// This runs every frame but is cheap — it only updates colors if the
/// grid data changed. For now it always runs; later we'll add change
/// detection to skip unchanged tiles.
///
/// `Query<..., With<TileSprite>>` — the `With` filter means "only entities
/// that have the TileSprite component." We don't need to read TileSprite's
/// value (it has no data), just use it as a filter.
pub fn update_tile_colors(
    grid: Res<Grid>,
    mut tiles_q: Query<(&GridPosition, &mut Sprite), With<TileSprite>>,
) {
    // Only re-color if the Grid resource changed this frame.
    if !grid.is_changed() {
        return;
    }

    for (pos, mut sprite) in &mut tiles_q {
        if let Some(tile) = grid.get(pos.x, pos.y) {
            sprite.color = tile.color();
        }
    }
}
