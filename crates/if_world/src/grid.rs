// grid.rs: The tile-based grid system for planetary surfaces.
//
// The grid is the foundational data structure — everything in the factory
// simulation (drills, belts, machines) lives on grid cells.

use bevy::prelude::*;
use if_common::{DEFAULT_GRID_HEIGHT, DEFAULT_GRID_WIDTH, TileType};
use serde::{Deserialize, Serialize};

use crate::bodies::BodyType;

/// The grid resource — holds all tile data for one planetary surface.
///
/// We store tiles in a **flat Vec** with index math: `index = y * width + x`.
/// Why not `Vec<Vec<TileType>>`? Two reasons:
///   1. Memory locality — a flat Vec is one contiguous block of memory.
///      CPUs love sequential access (cache lines). A Vec<Vec<>> is a Vec
///      of pointers to separate heap allocations scattered in memory.
///   2. Simpler ownership — a flat Vec is one owner. Vec<Vec<>> has nested
///      ownership which complicates borrowing (e.g., borrowing row 0 and
///      row 1 simultaneously requires convincing the borrow checker they
///      don't overlap).
///
/// The tradeoff: we need `index = y * width + x` math everywhere. We wrap
/// that in methods so callers don't think about it.
#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    tiles: Vec<TileType>,
}

impl Grid {
    /// Get a reference to the raw tiles vector (for serialization).
    pub fn tiles(&self) -> &[TileType] {
        &self.tiles
    }

    /// Create a grid from raw data (for deserialization/loading).
    pub fn from_raw(width: u32, height: u32, tiles: Vec<TileType>) -> Self {
        assert_eq!(
            tiles.len(),
            (width * height) as usize,
            "tiles length must match width * height"
        );
        Self {
            width,
            height,
            tiles,
        }
    }
}

impl Grid {
    /// Create a new grid filled with empty tiles.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            // `vec![value; count]` creates a Vec with `count` copies of `value`.
            // This works because TileType implements Copy.
            tiles: vec![TileType::Empty; (width * height) as usize],
        }
    }

    /// Convert (x, y) grid coordinates to a flat index.
    /// Returns None if the coordinates are out of bounds.
    ///
    /// Why `Option<usize>` instead of just `usize`? Because in Rust we
    /// encode failure in the type system. The caller MUST handle the None
    /// case — the compiler won't let them ignore it. This prevents
    /// out-of-bounds panics at runtime.
    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x < self.width && y < self.height {
            Some((y * self.width + x) as usize)
        } else {
            None
        }
    }

    /// Get the tile type at (x, y). Returns None if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> Option<TileType> {
        self.index(x, y).map(|i| self.tiles[i])
    }

    /// Set the tile type at (x, y). Returns false if out of bounds.
    pub fn set(&mut self, x: u32, y: u32, tile: TileType) -> bool {
        if let Some(i) = self.index(x, y) {
            self.tiles[i] = tile;
            true
        } else {
            false
        }
    }
}

/// Startup system: generate a star system and install the home planet's grid.
///
/// This replaces the old hand-placed `spawn_grid`. We now procedurally generate
/// a small star system (Sol by default, seed 42 for reproducibility). Each
/// planet owns its own `PlanetSurface` component; the "home planet" — the
/// first generated planet — also has its surface mirrored into the top-level
/// `Grid` resource so all existing rendering/factory systems (which read
/// `Res<Grid>`) continue to work unchanged.
///
/// We also install a `CurrentBody(Entity)` resource pointing at the home
/// planet's entity, and a `StarSystem` resource holding every entity in the
/// system (star + planets + moons).
pub fn spawn_star_system(mut commands: Commands) {
    use crate::bodies::{CurrentBody, StarSystem};
    use crate::generation::generate_star_system;

    const SYSTEM_SEED: u32 = 42;

    let generated = generate_star_system(SYSTEM_SEED);

    let mut all_bodies: Vec<Entity> = Vec::with_capacity(generated.len());
    let mut star_entity: Option<Entity> = None;
    let mut home_entity: Option<Entity> = None;
    let mut home_grid: Option<Grid> = None;

    // Track the most recent planet so moons can be parented to it. Our
    // generator emits planets followed (optionally) by their single moon, so
    // the immediately-preceding planet is always the right parent for a moon.
    let mut last_planet_entity: Option<Entity> = None;

    for (i, (body, surface)) in generated.into_iter().enumerate() {
        let is_star = i == 0;
        let body_type = body.body_type;

        // Decide the parent: star has none; planets have the star; moons have
        // the most-recent planet.
        let parent = if is_star {
            None
        } else if body_type == BodyType::Moon {
            last_planet_entity.or(star_entity)
        } else {
            star_entity
        };

        let mut patched = body;
        patched.parent = parent;

        let mut cmd = commands.spawn(patched);
        if let Some(s) = surface.as_ref() {
            cmd.insert(s.clone());
        }
        let entity = cmd.id();

        if is_star {
            star_entity = Some(entity);
        } else if body_type != BodyType::Moon {
            // First planet (first body that is neither the star nor a moon)
            // becomes the home planet.
            if home_entity.is_none() {
                home_entity = Some(entity);
                if let Some(surface) = surface.as_ref() {
                    home_grid = Some(surface.grid.clone());
                }
            }
            last_planet_entity = Some(entity);
        }

        all_bodies.push(entity);
    }

    // Fallback: if for some reason generation didn't produce a home planet
    // (shouldn't happen with the 3–5 guarantee), fall back to a blank grid
    // so the rest of the app boots cleanly.
    let grid = home_grid.unwrap_or_else(|| Grid::new(DEFAULT_GRID_WIDTH, DEFAULT_GRID_HEIGHT));
    commands.insert_resource(grid);

    commands.insert_resource(StarSystem {
        name: "Sol".to_string(),
        bodies: all_bodies,
        star: star_entity,
    });

    if let Some(home) = home_entity {
        commands.insert_resource(CurrentBody(home));
    }
}

/// Backwards-compatible alias used by existing call sites.
///
/// Older entry points added `if_world::grid::spawn_grid` to their Startup
/// schedule. Those now want the full star-system bootstrap. Keeping the old
/// name around means downstream crates don't have to change the moment this
/// lands.
pub use spawn_star_system as spawn_grid;

// --- Tests ---
// These live in the same file, gated behind #[cfg(test)].
// `cargo test` compiles this module; `cargo build` ignores it entirely.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_is_all_empty() {
        let grid = Grid::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(grid.get(x, y), Some(TileType::Empty));
            }
        }
    }

    #[test]
    fn set_and_get_tile() {
        let mut grid = Grid::new(4, 4);
        assert!(grid.set(2, 3, TileType::CopperDeposit));
        assert_eq!(grid.get(2, 3), Some(TileType::CopperDeposit));
    }

    #[test]
    fn out_of_bounds_get_returns_none() {
        let grid = Grid::new(4, 4);
        assert_eq!(grid.get(4, 0), None); // x == width is out of bounds
        assert_eq!(grid.get(0, 4), None); // y == height is out of bounds
        assert_eq!(grid.get(100, 100), None);
    }

    #[test]
    fn out_of_bounds_set_returns_false() {
        let mut grid = Grid::new(4, 4);
        assert!(!grid.set(4, 0, TileType::Rock));
        assert!(!grid.set(0, 4, TileType::Rock));
    }

    #[test]
    fn grid_dimensions_correct() {
        let grid = Grid::new(8, 16);
        assert_eq!(grid.width, 8);
        assert_eq!(grid.height, 16);
    }

    #[test]
    fn flat_index_math_is_correct() {
        // Verify that (x, y) maps correctly in the flat array.
        // For a 4-wide grid: (3, 2) should be index 2*4+3 = 11
        let mut grid = Grid::new(4, 4);
        grid.set(3, 2, TileType::IronDeposit);
        // The only IronDeposit should be at flat index 11
        assert_eq!(grid.tiles[11], TileType::IronDeposit);
    }
}
