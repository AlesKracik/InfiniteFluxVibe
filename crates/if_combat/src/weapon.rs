// weapon.rs: Weapon definitions, range/falloff math, and firing readiness.
//
// A `Weapon` is a Bevy component: one per turret/launcher. The `WeaponStats`
// struct inside carries the tuneable data (damage, range, heat cost, etc.)
// so we can have hundreds of prefab stat blocks without needing one
// component type per weapon kind. Cooldown is tracked in ticks.

use bevy::prelude::*;
use if_common::item::ItemType;
use serde::{Deserialize, Serialize};

use crate::capacitor::{Capacitor, HeatSinks};
use crate::damage::DamageType;

/// Classification of the weapon, used for UI display and ammo routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponKind {
    /// EM damage, long range, runs very hot, no ammo.
    Laser,
    /// Kinetic damage, medium range, modest heat, consumes ammo.
    Autocannon,
    /// Explosive damage, long range, consumes missiles (ammo).
    Missile,
    /// Kinetic damage, very long range, high damage, heavy cap cost.
    Railgun,
}

/// Immutable tuning data for a weapon. Cloning is cheap — these get copied
/// onto individual `Weapon` components when a module is fit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeaponStats {
    pub kind: WeaponKind,
    pub damage: f32,
    pub damage_type: DamageType,
    /// Range at which the weapon does full damage.
    pub optimal_range: f32,
    /// Distance beyond optimal over which damage halves — the shorter the
    /// falloff, the more punishing the optimal window.
    pub falloff: f32,
    /// Cooldown in ticks between shots.
    pub cooldown: u32,
    /// Capacitor drained per shot.
    pub cap_cost: f32,
    /// Heat generated per shot.
    pub heat_per_shot: f32,
    /// Ammo consumed per shot. Lasers use `None`; missiles/guns use `Some`.
    pub ammo_item: Option<ItemType>,
}

impl WeaponStats {
    /// A reasonable laser preset. EM, hot, no ammo.
    pub fn laser_mk1() -> Self {
        Self {
            kind: WeaponKind::Laser,
            damage: 40.0,
            damage_type: DamageType::EM,
            optimal_range: 1500.0,
            falloff: 500.0,
            cooldown: 30,
            cap_cost: 15.0,
            heat_per_shot: 8.0,
            ammo_item: None,
        }
    }

    /// Autocannon preset. Kinetic, medium range.
    pub fn autocannon_mk1() -> Self {
        Self {
            kind: WeaponKind::Autocannon,
            damage: 25.0,
            damage_type: DamageType::Kinetic,
            optimal_range: 800.0,
            falloff: 400.0,
            cooldown: 15,
            cap_cost: 3.0,
            heat_per_shot: 2.0,
            ammo_item: Some(ItemType::IronPlate),
        }
    }

    /// Missile preset. Long range, explosive, ammo.
    pub fn missile_mk1() -> Self {
        Self {
            kind: WeaponKind::Missile,
            damage: 120.0,
            damage_type: DamageType::Explosive,
            optimal_range: 2500.0,
            falloff: 800.0,
            cooldown: 90,
            cap_cost: 6.0,
            heat_per_shot: 1.0,
            ammo_item: Some(ItemType::BasicCircuit),
        }
    }

    /// Railgun preset. Long range, kinetic, heavy cap draw.
    pub fn railgun_mk1() -> Self {
        Self {
            kind: WeaponKind::Railgun,
            damage: 180.0,
            damage_type: DamageType::Kinetic,
            optimal_range: 3500.0,
            falloff: 900.0,
            cooldown: 120,
            cap_cost: 60.0,
            heat_per_shot: 25.0,
            ammo_item: Some(ItemType::HullPlate),
        }
    }
}

/// A fitted weapon with its current cooldown counter.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Weapon {
    pub stats: WeaponStats,
    /// Ticks remaining until the weapon can fire again.
    pub ticks_until_ready: u32,
}

impl Weapon {
    pub fn new(stats: WeaponStats) -> Self {
        Self {
            stats,
            ticks_until_ready: 0,
        }
    }

    /// True iff cooldown is 0, capacitor has enough charge, and the heat
    /// sink isn't already past its limit. Does not consume any resources.
    pub fn can_fire(&self, cap: &Capacitor, heat: &HeatSinks) -> bool {
        self.ticks_until_ready == 0 && cap.charge >= self.stats.cap_cost && !heat.is_overheated()
    }

    /// Decrement the cooldown counter by one tick. Saturating at zero.
    pub fn tick_cooldown(&mut self) {
        self.ticks_until_ready = self.ticks_until_ready.saturating_sub(1);
    }

    /// Set cooldown after firing.
    pub fn start_cooldown(&mut self) {
        self.ticks_until_ready = self.stats.cooldown;
    }

    /// Compute the actual damage this weapon would deal at `distance`, using
    /// the classic optimal-plus-falloff model:
    ///
    /// * `d <= optimal`: full damage
    /// * `optimal < d <= optimal + 3 * falloff`: damage * 0.5^((d - optimal) / falloff)
    /// * beyond that: 0 (hard cutoff — the shot "misses")
    ///
    /// Then scaled by `tracking_quality` in `[0.0, 1.0]` which models the
    /// turret's ability to land on a fast-moving target. Values outside the
    /// range are clamped.
    pub fn damage_at_range(&self, distance: f32, tracking_quality: f32) -> f32 {
        let tracking = tracking_quality.clamp(0.0, 1.0);
        if distance <= self.stats.optimal_range {
            return self.stats.damage * tracking;
        }
        let max_range = self.stats.optimal_range + 3.0 * self.stats.falloff;
        if distance > max_range || self.stats.falloff <= 0.0 {
            return 0.0;
        }
        let exponent = (distance - self.stats.optimal_range) / self.stats.falloff;
        self.stats.damage * 0.5_f32.powf(exponent) * tracking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_fire_requires_cooldown_cleared() {
        let mut w = Weapon::new(WeaponStats::laser_mk1());
        let cap = Capacitor::new(100.0, 1.0);
        let heat = HeatSinks::new(100.0, 1.0);
        assert!(w.can_fire(&cap, &heat));
        w.start_cooldown();
        assert!(!w.can_fire(&cap, &heat));
    }

    #[test]
    fn can_fire_requires_capacitor_charge() {
        let w = Weapon::new(WeaponStats::laser_mk1());
        let mut cap = Capacitor::new(100.0, 1.0);
        let heat = HeatSinks::new(100.0, 1.0);
        cap.charge = w.stats.cap_cost - 0.1;
        assert!(!w.can_fire(&cap, &heat));
        cap.charge = w.stats.cap_cost;
        assert!(w.can_fire(&cap, &heat));
    }

    #[test]
    fn can_fire_blocked_by_overheat() {
        let w = Weapon::new(WeaponStats::laser_mk1());
        let cap = Capacitor::new(100.0, 1.0);
        let mut heat = HeatSinks::new(50.0, 1.0);
        heat.add_heat(60.0);
        assert!(!w.can_fire(&cap, &heat));
    }

    #[test]
    fn tick_cooldown_saturates_at_zero() {
        let mut w = Weapon::new(WeaponStats::laser_mk1());
        w.tick_cooldown();
        w.tick_cooldown();
        assert_eq!(w.ticks_until_ready, 0);
    }

    #[test]
    fn start_cooldown_sets_ticks_remaining() {
        let mut w = Weapon::new(WeaponStats::laser_mk1());
        w.start_cooldown();
        assert_eq!(w.ticks_until_ready, w.stats.cooldown);
        w.tick_cooldown();
        assert_eq!(w.ticks_until_ready, w.stats.cooldown - 1);
    }

    #[test]
    fn damage_at_range_full_within_optimal() {
        let w = Weapon::new(WeaponStats {
            kind: WeaponKind::Laser,
            damage: 100.0,
            damage_type: DamageType::EM,
            optimal_range: 1000.0,
            falloff: 500.0,
            cooldown: 10,
            cap_cost: 0.0,
            heat_per_shot: 0.0,
            ammo_item: None,
        });
        assert!((w.damage_at_range(0.0, 1.0) - 100.0).abs() < 1e-4);
        assert!((w.damage_at_range(1000.0, 1.0) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn damage_at_range_falls_off_beyond_optimal() {
        let w = Weapon::new(WeaponStats {
            kind: WeaponKind::Laser,
            damage: 100.0,
            damage_type: DamageType::EM,
            optimal_range: 1000.0,
            falloff: 500.0,
            cooldown: 10,
            cap_cost: 0.0,
            heat_per_shot: 0.0,
            ammo_item: None,
        });
        // One falloff unit past optimal = 50%.
        let d1 = w.damage_at_range(1500.0, 1.0);
        assert!((d1 - 50.0).abs() < 1e-2, "expected ~50, got {d1}");
        // Two falloffs = 25%.
        let d2 = w.damage_at_range(2000.0, 1.0);
        assert!((d2 - 25.0).abs() < 1e-2, "expected ~25, got {d2}");
    }

    #[test]
    fn damage_at_range_hard_cutoff() {
        let w = Weapon::new(WeaponStats {
            kind: WeaponKind::Laser,
            damage: 100.0,
            damage_type: DamageType::EM,
            optimal_range: 1000.0,
            falloff: 500.0,
            cooldown: 10,
            cap_cost: 0.0,
            heat_per_shot: 0.0,
            ammo_item: None,
        });
        // Hard zero past optimal + 3 * falloff = 2500.
        assert!(w.damage_at_range(2501.0, 1.0).abs() < 1e-4);
        assert!(w.damage_at_range(5000.0, 1.0).abs() < 1e-4);
    }

    #[test]
    fn damage_at_range_scales_by_tracking_quality() {
        let w = Weapon::new(WeaponStats::laser_mk1());
        let full = w.damage_at_range(100.0, 1.0);
        let half = w.damage_at_range(100.0, 0.5);
        assert!((half - full * 0.5).abs() < 1e-4);
        let clamped = w.damage_at_range(100.0, 2.0); // clamps to 1.0
        assert!((clamped - full).abs() < 1e-4);
        let zeroed = w.damage_at_range(100.0, -1.0); // clamps to 0.0
        assert!(zeroed.abs() < 1e-4);
    }

    #[test]
    fn weapon_serde_round_trip() {
        let w = Weapon::new(WeaponStats::railgun_mk1());
        let bytes = bincode::serialize(&w).unwrap();
        let restored: Weapon = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.stats.kind, w.stats.kind);
        assert!((restored.stats.damage - w.stats.damage).abs() < 1e-4);
        assert_eq!(restored.stats.ammo_item, w.stats.ammo_item);
    }

    #[test]
    fn weapon_kind_round_trip_all_variants() {
        for k in [
            WeaponKind::Laser,
            WeaponKind::Autocannon,
            WeaponKind::Missile,
            WeaponKind::Railgun,
        ] {
            let bytes = bincode::serialize(&k).unwrap();
            let restored: WeaponKind = bincode::deserialize(&bytes).unwrap();
            assert_eq!(restored, k);
        }
    }
}
