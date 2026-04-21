// if_combat: Fleet combat systems for Infinite Flux Vibe.
//
// This crate owns the simulation side of combat: damage, health, capacitor,
// heat, weapons, fitting, targeting, and fleet commands. It deliberately
// knows nothing about rendering, UI, or networking — downstream crates
// subscribe to `ShipDestroyedEvent`/`FleetCommand` messages and read the
// components to visualize or replicate state.
//
// The top-level entry point is `CombatPlugin`, which registers:
//   * message: `FleetCommand`
//   * message: `ShipDestroyedEvent`
//   * systems: `targeting_system`, `silica_ai_system`,
//     `combat_tick_system`, `loot_decay_system`
//
// Tuning lives inline with each module (weapon presets, resist defaults,
// wreck lifetime). The AI is intentionally simple — we ship working code.

pub mod ai;
pub mod capacitor;
pub mod damage;
pub mod fitting;
pub mod fleet;
pub mod loot;
pub mod position;
pub mod targeting;
pub mod tick;
pub mod weapon;

pub use ai::{PlayerShip, SilicaSwarmAI, drone_would_fire, silica_ai_system};
pub use capacitor::{Capacitor, HeatSinks};
pub use damage::{DamageType, Resistances, SHIELD_REGEN_DELAY_TICKS, ShipHealth};
pub use fitting::ShipFit;
pub use fleet::{Fleet, FleetCommand};
pub use loot::{
    DEFAULT_LOOT_LIFETIME_TICKS, LOOT_SURVIVAL_FRACTION, LootContainer, ShipDestroyedEvent,
    compute_drop, loot_decay_system,
};
pub use position::CombatPosition;
pub use targeting::{Targeting, targeting_system};
pub use tick::{CombatPlugin, combat_tick_system};
pub use weapon::{Weapon, WeaponKind, WeaponStats};
