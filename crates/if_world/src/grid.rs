// grid.rs: The tile-based grid system for planetary surfaces.
//
// The grid is the foundational data structure — everything in the factory
// simulation (drills, belts, machines) lives on grid cells.

use bevy::prelude::*;
use if_common::{DEFAULT_GRID_HEIGHT, DEFAULT_GRID_WIDTH, TileType};
use serde::{Deserialize, Serialize};

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
#[derive(Resource, Serialize, Deserialize)]
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

/// Startup system: creates the grid resource and scatters some resource deposits.
///
/// In Bevy, a "system" is just a function whose parameters are automatically
/// injected by the ECS scheduler. `Commands` lets us insert resources and
/// spawn entities. Bevy sees the function signature and knows what to provide.
pub fn spawn_grid(mut commands: Commands) {
    let mut grid = Grid::new(DEFAULT_GRID_WIDTH, DEFAULT_GRID_HEIGHT);

    // Scatter some resource deposits for visual variety.
    // Later this will be procedural generation — for now, hand-placed.
    grid.set(5, 5, TileType::CopperDeposit);
    grid.set(6, 5, TileType::CopperDeposit);
    grid.set(5, 6, TileType::CopperDeposit);
    grid.set(10, 10, TileType::IronDeposit);
    grid.set(11, 10, TileType::IronDeposit);
    grid.set(10, 11, TileType::IronDeposit);
    grid.set(20, 3, TileType::Rock);
    grid.set(21, 3, TileType::Rock);
    grid.set(20, 4, TileType::Rock);
    grid.set(21, 4, TileType::Rock);

    // `insert_resource` makes the Grid available to all systems via `Res<Grid>`.
    commands.insert_resource(grid);
}

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
