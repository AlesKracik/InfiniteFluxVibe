// damage.rs: Damage types, resistances, and the hull/armor/shield model.
//
// Combat in Infinite Flux Vibe layers three defensive pools on every ship:
// shields (recharge over time), armor (stable HP with strong resistances),
// and hull (last line of defense, no resistance — hull damage sticks until
// repairs). Damage flows shields -> armor -> hull; each pool applies its own
// resistance profile before subtracting HP.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The four damage flavors. Different weapons deal different types; different
/// armor/shield layers resist different types. Balancing by damage-type
/// rock-paper-scissors is a classic space-game trope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Kinetic,
    Thermal,
    EM,
    Explosive,
}

/// Percentage damage reduction per damage type. 0.0 = no resistance, 1.0 =
/// immune. Values outside 0..=1 are clamped when applied so silly save files
/// or script errors can't produce negative damage (healing) or amplification.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Resistances {
    pub kinetic: f32,
    pub thermal: f32,
    pub em: f32,
    pub explosive: f32,
}

impl Resistances {
    /// Convenience constructor so callers don't have to name every field.
    pub const fn new(kinetic: f32, thermal: f32, em: f32, explosive: f32) -> Self {
        Self {
            kinetic,
            thermal,
            em,
            explosive,
        }
    }

    /// Raw resistance value for the given damage type (not clamped).
    pub fn resistance(&self, damage_type: DamageType) -> f32 {
        match damage_type {
            DamageType::Kinetic => self.kinetic,
            DamageType::Thermal => self.thermal,
            DamageType::EM => self.em,
            DamageType::Explosive => self.explosive,
        }
    }

    /// Apply resistance to incoming damage. The resistance is clamped to
    /// `[0.0, 1.0]` so no damage-type configuration can heal a target or
    /// amplify damage beyond the weapon's base output.
    pub fn mitigate(&self, damage: f32, damage_type: DamageType) -> f32 {
        damage * (1.0 - self.resistance(damage_type).clamp(0.0, 1.0))
    }
}

/// Aggregate HP pools for a ship.
///
/// The three-layer system (shields, armor, hull) is standard for fleet
/// combat games: shields absorb sustained DPS and self-repair; armor soaks
/// alphas; hull is the last gasp before destruction. We don't track
/// individual hit locations — this is fleet combat, not simulation.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct ShipHealth {
    pub hull: f32,
    pub hull_max: f32,
    pub armor: f32,
    pub armor_max: f32,
    pub shields: f32,
    pub shields_max: f32,
    /// Shield regen per tick when not recently hit.
    pub shield_regen: f32,
    /// Ticks since the last damage event. Reset to 0 on every hit.
    pub ticks_since_hit: u32,
    pub armor_resistances: Resistances,
    pub shield_resistances: Resistances,
}

/// Ticks of combat inactivity before shields start regenerating. At the
/// default 60 tps this is ~5 seconds, which gives a noticeable window where
/// shield-tanked ships must actually disengage to recover.
pub const SHIELD_REGEN_DELAY_TICKS: u32 = 300;

impl Default for ShipHealth {
    fn default() -> Self {
        Self {
            hull: 1000.0,
            hull_max: 1000.0,
            armor: 500.0,
            armor_max: 500.0,
            shields: 800.0,
            shields_max: 800.0,
            shield_regen: 5.0,
            ticks_since_hit: SHIELD_REGEN_DELAY_TICKS,
            armor_resistances: Resistances::default(),
            shield_resistances: Resistances::default(),
        }
    }
}

impl ShipHealth {
    /// Convenience builder used by tests and ship templates. All resistances
    /// default to zero; callers can tweak afterwards.
    pub fn new(hull: f32, armor: f32, shields: f32, shield_regen: f32) -> Self {
        Self {
            hull,
            hull_max: hull,
            armor,
            armor_max: armor,
            shields,
            shields_max: shields,
            shield_regen,
            ticks_since_hit: SHIELD_REGEN_DELAY_TICKS,
            armor_resistances: Resistances::default(),
            shield_resistances: Resistances::default(),
        }
    }

    /// True only once hull has been chewed through entirely. Armor and
    /// shields may be zero without the ship being destroyed.
    pub fn is_destroyed(&self) -> bool {
        self.hull <= 0.0
    }

    /// Apply incoming damage, cascading through shields -> armor -> hull.
    /// Returns the total HP actually removed across all layers *after*
    /// resistances are applied. Negative or zero inputs are no-ops.
    ///
    /// Both the shield and armor layers apply their own resistance profile
    /// to the *incoming overflow* (not to the overflow after the previous
    /// layer's resistance). The hull has no resistances — last-ditch HP.
    pub fn apply_damage(&mut self, amount: f32, damage_type: DamageType) -> f32 {
        if amount <= 0.0 {
            return 0.0;
        }

        // Any damage event resets the regen delay counter, whether or not
        // it actually removed HP (e.g. a hit on zero shields still counts as
        // being in combat).
        self.ticks_since_hit = 0;

        let mut dealt = 0.0;

        // Shield layer.
        if self.shields > 0.0 {
            let mitigated = self.shield_resistances.mitigate(amount, damage_type);
            let absorbed = mitigated.min(self.shields);
            self.shields -= absorbed;
            dealt += absorbed;

            if mitigated <= self.shields + absorbed - absorbed {
                // All damage absorbed by shields. Equivalent to `mitigated <= absorbed`
                // once shields still had enough headroom.
            }

            // Compute overflow in post-mitigation units, then translate back
            // to pre-mitigation units so the next layer applies its own
            // resistance to the original damage packet. This matches the
            // typical MMO approach — each layer resists the raw shot, not
            // the leftover after the previous layer.
            let absorbed_fraction = if mitigated > 0.0 {
                absorbed / mitigated
            } else {
                1.0
            };
            let consumed_raw = amount * absorbed_fraction;
            let overflow_raw = (amount - consumed_raw).max(0.0);
            if overflow_raw <= 0.0 {
                return dealt;
            }

            // Armor layer applies its own resistance to the raw overflow.
            let armor_in = self.armor_resistances.mitigate(overflow_raw, damage_type);
            let armor_absorbed = armor_in.min(self.armor);
            self.armor -= armor_absorbed;
            dealt += armor_absorbed;

            let armor_absorbed_fraction = if armor_in > 0.0 {
                armor_absorbed / armor_in
            } else {
                1.0
            };
            let consumed_raw2 = overflow_raw * armor_absorbed_fraction;
            let hull_raw = (overflow_raw - consumed_raw2).max(0.0);
            if hull_raw <= 0.0 {
                return dealt;
            }

            // Hull has no resistances — raw damage goes directly to HP.
            let hull_absorbed = hull_raw.min(self.hull);
            self.hull -= hull_absorbed;
            dealt += hull_absorbed;
            return dealt;
        }

        // Shields already depleted — start at the armor layer.
        if self.armor > 0.0 {
            let armor_in = self.armor_resistances.mitigate(amount, damage_type);
            let armor_absorbed = armor_in.min(self.armor);
            self.armor -= armor_absorbed;
            dealt += armor_absorbed;

            let armor_absorbed_fraction = if armor_in > 0.0 {
                armor_absorbed / armor_in
            } else {
                1.0
            };
            let consumed_raw = amount * armor_absorbed_fraction;
            let hull_raw = (amount - consumed_raw).max(0.0);
            if hull_raw <= 0.0 {
                return dealt;
            }

            let hull_absorbed = hull_raw.min(self.hull);
            self.hull -= hull_absorbed;
            dealt += hull_absorbed;
            return dealt;
        }

        // Both shields and armor gone — pure hull damage.
        let hull_absorbed = amount.min(self.hull);
        self.hull -= hull_absorbed;
        dealt += hull_absorbed;
        dealt
    }

    /// Advance shield regeneration by one tick. If `SHIELD_REGEN_DELAY_TICKS`
    /// have elapsed since the last hit, add `shield_regen` to shields, capped
    /// at `shields_max`. The delay counter saturates so it never wraps.
    pub fn tick_shield_regen(&mut self) {
        self.ticks_since_hit = self.ticks_since_hit.saturating_add(1);
        if self.ticks_since_hit < SHIELD_REGEN_DELAY_TICKS {
            return;
        }
        if self.shields < self.shields_max {
            self.shields = (self.shields + self.shield_regen).min(self.shields_max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resistance_returns_per_type_values() {
        let r = Resistances::new(0.1, 0.2, 0.3, 0.4);
        assert!((r.resistance(DamageType::Kinetic) - 0.1).abs() < 1e-6);
        assert!((r.resistance(DamageType::Thermal) - 0.2).abs() < 1e-6);
        assert!((r.resistance(DamageType::EM) - 0.3).abs() < 1e-6);
        assert!((r.resistance(DamageType::Explosive) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn mitigate_reduces_damage_by_resistance_fraction() {
        let r = Resistances::new(0.25, 0.0, 0.0, 0.0);
        assert!((r.mitigate(100.0, DamageType::Kinetic) - 75.0).abs() < 1e-4);
        // No resistance to thermal -> full damage through.
        assert!((r.mitigate(100.0, DamageType::Thermal) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn mitigate_clamps_out_of_range_resistances() {
        let r = Resistances::new(-1.0, 2.0, 0.5, 0.0);
        // Negative resistance treated as 0 -> full damage.
        assert!((r.mitigate(100.0, DamageType::Kinetic) - 100.0).abs() < 1e-4);
        // Over-1 resistance treated as 1.0 -> zero damage (not negative).
        assert!(r.mitigate(100.0, DamageType::Thermal).abs() < 1e-4);
    }

    #[test]
    fn is_destroyed_only_at_zero_hull() {
        let mut h = ShipHealth::new(100.0, 50.0, 50.0, 0.0);
        assert!(!h.is_destroyed());
        h.armor = 0.0;
        h.shields = 0.0;
        assert!(!h.is_destroyed());
        h.hull = 0.0;
        assert!(h.is_destroyed());
        h.hull = -5.0;
        assert!(h.is_destroyed());
    }

    #[test]
    fn shields_absorb_damage_first() {
        let mut h = ShipHealth::new(100.0, 100.0, 200.0, 0.0);
        let dealt = h.apply_damage(50.0, DamageType::Kinetic);
        assert!((dealt - 50.0).abs() < 1e-4);
        assert!((h.shields - 150.0).abs() < 1e-4);
        assert!((h.armor - 100.0).abs() < 1e-4);
        assert!((h.hull - 100.0).abs() < 1e-4);
    }

    #[test]
    fn overflow_cascades_shields_to_armor_to_hull() {
        // Shields 50, armor 30, hull 100 — pummel with 120 raw kinetic.
        // Breakdown: 50 -> shields, 30 -> armor, 40 -> hull. Hull ends at 60.
        let mut h = ShipHealth::new(100.0, 30.0, 50.0, 0.0);
        let dealt = h.apply_damage(120.0, DamageType::Kinetic);
        assert!(h.shields.abs() < 1e-4, "shields: {}", h.shields);
        assert!(h.armor.abs() < 1e-4, "armor: {}", h.armor);
        assert!((h.hull - 60.0).abs() < 1e-4);
        assert!((dealt - 120.0).abs() < 1e-4);
    }

    #[test]
    fn overflow_past_hull_floors_at_zero() {
        // Overkill: 200 damage vs shield 50 / armor 30 / hull 100. Hull
        // should floor at 0 and `dealt` should be capped at actual HP lost.
        let mut h = ShipHealth::new(100.0, 30.0, 50.0, 0.0);
        let dealt = h.apply_damage(200.0, DamageType::Kinetic);
        assert!(h.shields.abs() < 1e-4);
        assert!(h.armor.abs() < 1e-4);
        assert!(h.hull.abs() < 1e-4);
        assert!(h.is_destroyed());
        assert!((dealt - 180.0).abs() < 1e-4);
    }

    #[test]
    fn shield_resistances_reduce_incoming_damage() {
        let mut h = ShipHealth::new(100.0, 0.0, 100.0, 0.0);
        h.shield_resistances = Resistances::new(0.5, 0.0, 0.0, 0.0);
        // 100 kinetic -> 50 to shields after 50% resist.
        let dealt = h.apply_damage(100.0, DamageType::Kinetic);
        assert!((dealt - 50.0).abs() < 1e-4);
        assert!((h.shields - 50.0).abs() < 1e-4);
    }

    #[test]
    fn armor_resistances_apply_when_shields_depleted() {
        let mut h = ShipHealth::new(500.0, 100.0, 0.0, 0.0);
        h.armor_resistances = Resistances::new(0.5, 0.0, 0.0, 0.0);
        let dealt = h.apply_damage(100.0, DamageType::Kinetic);
        // 100 raw -> 50 after armor resist, all absorbed by 100 armor.
        assert!((dealt - 50.0).abs() < 1e-4);
        assert!((h.armor - 50.0).abs() < 1e-4);
        assert!((h.hull - 500.0).abs() < 1e-4);
    }

    #[test]
    fn hull_has_no_resistance() {
        let mut h = ShipHealth::new(100.0, 0.0, 0.0, 0.0);
        h.armor_resistances = Resistances::new(0.9, 0.9, 0.9, 0.9);
        h.shield_resistances = Resistances::new(0.9, 0.9, 0.9, 0.9);
        let dealt = h.apply_damage(40.0, DamageType::Kinetic);
        assert!((dealt - 40.0).abs() < 1e-4);
        assert!((h.hull - 60.0).abs() < 1e-4);
    }

    #[test]
    fn damage_types_match_correct_resistance_fields() {
        // EM shot against a thermal-heavy resistance profile should not be
        // reduced at all.
        let mut h = ShipHealth::new(100.0, 0.0, 100.0, 0.0);
        h.shield_resistances = Resistances::new(0.0, 0.9, 0.0, 0.0);
        let dealt = h.apply_damage(50.0, DamageType::EM);
        assert!((dealt - 50.0).abs() < 1e-4);
        // Same damage on thermal -> 90% reduction.
        let mut h2 = ShipHealth::new(100.0, 0.0, 100.0, 0.0);
        h2.shield_resistances = Resistances::new(0.0, 0.9, 0.0, 0.0);
        let dealt2 = h2.apply_damage(50.0, DamageType::Thermal);
        assert!((dealt2 - 5.0).abs() < 1e-4);
    }

    #[test]
    fn apply_damage_rejects_non_positive() {
        let mut h = ShipHealth::new(100.0, 0.0, 100.0, 0.0);
        assert_eq!(h.apply_damage(0.0, DamageType::Kinetic), 0.0);
        assert_eq!(h.apply_damage(-5.0, DamageType::Kinetic), 0.0);
        // Shields untouched.
        assert!((h.shields - 100.0).abs() < 1e-4);
    }

    #[test]
    fn shield_regen_waits_for_delay() {
        let mut h = ShipHealth::new(100.0, 0.0, 100.0, 10.0);
        h.shields = 50.0;
        h.ticks_since_hit = 0;

        // Just shy of the delay — no regen.
        for _ in 0..SHIELD_REGEN_DELAY_TICKS - 1 {
            h.tick_shield_regen();
        }
        assert!((h.shields - 50.0).abs() < 1e-4);

        // One more tick crosses the threshold.
        h.tick_shield_regen();
        assert!(h.shields > 50.0);
    }

    #[test]
    fn shield_regen_caps_at_max() {
        let mut h = ShipHealth::new(100.0, 0.0, 100.0, 30.0);
        h.shields = 90.0;
        h.ticks_since_hit = SHIELD_REGEN_DELAY_TICKS;
        for _ in 0..10 {
            h.tick_shield_regen();
        }
        assert!((h.shields - 100.0).abs() < 1e-4);
    }

    #[test]
    fn damage_resets_shield_regen_timer() {
        let mut h = ShipHealth::new(100.0, 0.0, 100.0, 10.0);
        h.ticks_since_hit = SHIELD_REGEN_DELAY_TICKS + 10;
        h.apply_damage(5.0, DamageType::Kinetic);
        assert_eq!(h.ticks_since_hit, 0);
    }

    #[test]
    fn ship_health_serde_round_trip() {
        let mut h = ShipHealth::new(1000.0, 500.0, 800.0, 5.0);
        h.armor_resistances = Resistances::new(0.5, 0.2, 0.1, 0.3);
        h.shield_resistances = Resistances::new(0.0, 0.4, 0.6, 0.2);
        let bytes = bincode::serialize(&h).unwrap();
        let restored: ShipHealth = bincode::deserialize(&bytes).unwrap();
        assert!((restored.hull - h.hull).abs() < 1e-4);
        assert_eq!(restored.armor_resistances, h.armor_resistances);
        assert_eq!(restored.shield_resistances, h.shield_resistances);
    }
}
