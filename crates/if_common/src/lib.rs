// if_common: Shared types, components, and constants for Infinite Flux.
//
// This crate is a dependency of every other crate in the workspace.
// It defines the "language" of the game — the types that all systems agree on.

pub mod item;
pub mod recipe;
pub mod skill;

pub mod save;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// --- Grid Constants ---

/// Default grid dimensions for a planetary surface.
/// We start small (32x32) for development. This will grow later.
pub const DEFAULT_GRID_WIDTH: u32 = 32;
pub const DEFAULT_GRID_HEIGHT: u32 = 32;

/// Size of each tile in pixels when rendered.
pub const TILE_SIZE: f32 = 32.0;

// --- Tile Types ---

/// Represents what kind of terrain occupies a grid cell.
///
/// In Rust, an `enum` is not just a set of labels — each variant can carry data.
/// Right now our variants are simple (no data), but later we'll add variants like
/// `ResourceNode { resource: ResourceType, yield_rate: f32 }`.
///
/// `Clone, Copy` — lets us duplicate tile values without borrowing headaches.
/// `Debug` — lets us print tiles with `{:?}` for debugging.
/// `PartialEq` — lets us compare tiles with `==`.
/// `Default` — gives us `TileType::default()` which returns `Empty`.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum TileType {
    #[default]
    Empty,
    Rock,
    CopperDeposit,
    IronDeposit,
}

impl TileType {
    /// Returns a color for rendering this tile type.
    /// We use this in if_client to color the grid quads.
    pub fn color(&self) -> Color {
        match self {
            TileType::Empty => Color::srgb(0.15, 0.15, 0.2),
            TileType::Rock => Color::srgb(0.4, 0.38, 0.35),
            TileType::CopperDeposit => Color::srgb(0.72, 0.45, 0.2),
            TileType::IronDeposit => Color::srgb(0.6, 0.6, 0.65),
        }
    }
}

// --- Grid Position ---

/// A position on the tile grid. This is NOT a pixel position — it's a logical
/// coordinate (column, row). Rendering code converts this to pixels.
///
/// We derive `Component` so Bevy's ECS can attach this to entities.
/// `Hash` lets us use GridPosition as a HashMap key later (for spatial lookups).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridPosition {
    pub x: u32,
    pub y: u32,
}

impl GridPosition {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// Convert grid coordinates to world-space pixel coordinates.
    /// Grid (0,0) maps to the bottom-left of the rendered grid.
    pub fn to_world(&self) -> Vec2 {
        Vec2::new(self.x as f32 * TILE_SIZE, self.y as f32 * TILE_SIZE)
    }
}
