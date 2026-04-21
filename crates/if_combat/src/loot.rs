// loot.rs: Ship destruction and floating wreckage.
//
// When a ship's hull hits zero it explodes, emits a `ShipDestroyedEvent`,
// and we spawn a `LootContainer` at its position. The container decays
// after a configurable lifetime; players that haven't scavenged by then
// lose the loot (otherwise wrecks would litter systems forever).

use bevy::ecs::message::Message;
use bevy::prelude::*;
use if_common::item::ItemType;
use serde::{Deserialize, Serialize};

/// Default wreck lifetime in ticks. At 60 tps this is ~30 seconds — long
/// enough to scoop in-combat, short enough not to pollute systems.
pub const DEFAULT_LOOT_LIFETIME_TICKS: u32 = 1800;

/// Fraction of a destroyed ship's inventory that turns into loot. The rest
/// is assumed to have been destroyed along with the ship. Tuneable; the
/// spec suggests "a fraction thereof".
pub const LOOT_SURVIVAL_FRACTION: f32 = 0.5;

/// Fired when a ship is destroyed. Listeners may award kill credit, log
/// to a combat journal, or spawn visual FX. Messages in Bevy 0.18 aren't
/// serialized — we carry the `Entity` directly so consumers can look up
/// the target before it finishes despawning.
#[derive(Message, Clone, Debug)]
pub struct ShipDestroyedEvent {
    pub ship: Entity,
    pub position: Vec2,
    pub loot: Vec<(ItemType, u32)>,
}

/// Floating wreckage entity produced by a destruction. Decays in `lifetime`
/// ticks — once `lifetime` reaches zero, `loot_decay_system` despawns the
/// entity and the items are lost.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct LootContainer {
    pub items: Vec<(ItemType, u32)>,
    /// Ticks until decay.
    pub lifetime: u32,
}

impl LootContainer {
    pub fn new(items: Vec<(ItemType, u32)>, lifetime: u32) -> Self {
        Self { items, lifetime }
    }
}

/// Tick-down every wreck and despawn it when `lifetime` hits zero.
pub fn loot_decay_system(
    mut commands: Commands,
    mut containers: Query<(Entity, &mut LootContainer)>,
) {
    for (entity, mut c) in &mut containers {
        if c.lifetime == 0 {
            commands.entity(entity).despawn();
            continue;
        }
        c.lifetime -= 1;
    }
}

/// Pick a survival fraction of a source inventory's stack list. Returns a
/// vector of `(item, qty)` pairs where quantities are rounded down and
/// zero-quantity stacks are omitted.
pub fn compute_drop(source: &[(ItemType, u32)], fraction: f32) -> Vec<(ItemType, u32)> {
    let f = fraction.clamp(0.0, 1.0);
    source
        .iter()
        .filter_map(|(item, qty)| {
            let dropped = ((*qty as f32) * f).floor() as u32;
            if dropped > 0 {
                Some((*item, dropped))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loot_container_decays_over_lifetime() {
        let mut app = App::new();
        app.add_systems(Update, loot_decay_system);
        let wreck = app
            .world_mut()
            .spawn(LootContainer::new(vec![(ItemType::IronOre, 10)], 3))
            .id();
        app.update();
        assert!(app.world().get::<LootContainer>(wreck).is_some());
        app.update();
        assert!(app.world().get::<LootContainer>(wreck).is_some());
        app.update();
        // Lifetime now 0, but not despawned until next tick enters with 0.
        assert!(app.world().get::<LootContainer>(wreck).is_some());
        app.update();
        // Entity despawned.
        assert!(app.world().get_entity(wreck).is_err());
    }

    #[test]
    fn compute_drop_scales_by_fraction() {
        let src = vec![
            (ItemType::IronOre, 10),
            (ItemType::CopperOre, 7),
            (ItemType::HullPlate, 1),
        ];
        let half = compute_drop(&src, 0.5);
        // 10 -> 5, 7 -> 3 (floor), 1 -> 0 (dropped from list).
        assert_eq!(half.len(), 2);
        assert_eq!(half[0], (ItemType::IronOre, 5));
        assert_eq!(half[1], (ItemType::CopperOre, 3));
    }

    #[test]
    fn compute_drop_clamps_fraction() {
        let src = vec![(ItemType::IronOre, 10)];
        let over = compute_drop(&src, 5.0);
        assert_eq!(over[0], (ItemType::IronOre, 10));
        let under = compute_drop(&src, -1.0);
        assert!(under.is_empty());
    }

    #[test]
    fn compute_drop_zero_fraction_empty() {
        let src = vec![(ItemType::IronOre, 100)];
        assert!(compute_drop(&src, 0.0).is_empty());
    }

    #[test]
    fn loot_container_serde_round_trip() {
        let c = LootContainer::new(vec![(ItemType::BasicCircuit, 2)], 1500);
        let bytes = bincode::serialize(&c).unwrap();
        let restored: LootContainer = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.items.len(), 1);
        assert_eq!(restored.items[0], (ItemType::BasicCircuit, 2));
        assert_eq!(restored.lifetime, 1500);
    }
}
