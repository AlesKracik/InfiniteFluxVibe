// capacitor.rs: Capacitor (energy) and heat dissipation.
//
// Every active module (weapons, EW, shield boosters) pulls from the ship's
// capacitor. Capacitor recharges continuously; running out means nothing
// fires until a recharge cycle restores enough juice. Heat accumulates as
// modules fire and dissipates passively; once you overheat you must cool
// down before firing again.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Energy pool that weapons and EW modules draw from every activation.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Capacitor {
    pub charge: f32,
    pub max_charge: f32,
    /// Energy restored per tick (continuous recharge, not pulsed).
    pub recharge_rate: f32,
}

impl Default for Capacitor {
    fn default() -> Self {
        Self {
            charge: 500.0,
            max_charge: 500.0,
            recharge_rate: 2.0,
        }
    }
}

impl Capacitor {
    pub fn new(max_charge: f32, recharge_rate: f32) -> Self {
        Self {
            charge: max_charge,
            max_charge,
            recharge_rate,
        }
    }

    /// Attempt to spend `amount` energy. Returns `true` only when the full
    /// amount was available; partial draws are not permitted (so weapons
    /// either fire or don't — no half-shots).
    pub fn try_consume(&mut self, amount: f32) -> bool {
        if amount <= 0.0 {
            return true;
        }
        if self.charge >= amount {
            self.charge -= amount;
            true
        } else {
            false
        }
    }

    /// Advance the recharge cycle by one tick. Caps at `max_charge`.
    pub fn recharge(&mut self) {
        if self.charge < self.max_charge {
            self.charge = (self.charge + self.recharge_rate).min(self.max_charge);
        }
    }

    /// Current fill ratio in `[0.0, 1.0]`. Useful for AI gating (don't fire
    /// below X% capacitor).
    pub fn fraction(&self) -> f32 {
        if self.max_charge <= 0.0 {
            0.0
        } else {
            (self.charge / self.max_charge).clamp(0.0, 1.0)
        }
    }
}

/// Passive heat pool. Modules add heat when fired; heat dissipates every
/// tick. Going above `max_heat` flags the ship as overheated and blocks
/// further firing until it cools back below the threshold.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct HeatSinks {
    pub heat: f32,
    pub max_heat: f32,
    /// Heat removed per tick when below max (applied as a floor at 0.0).
    pub dissipation: f32,
}

impl HeatSinks {
    pub fn new(max_heat: f32, dissipation: f32) -> Self {
        Self {
            heat: 0.0,
            max_heat,
            dissipation,
        }
    }

    /// Add heat. Negative values are ignored.
    pub fn add_heat(&mut self, amount: f32) {
        if amount <= 0.0 {
            return;
        }
        self.heat += amount;
    }

    /// Dissipate one tick's worth of heat. Floors at 0.0.
    pub fn tick_dissipation(&mut self) {
        if self.heat > 0.0 {
            self.heat = (self.heat - self.dissipation).max(0.0);
        }
    }

    /// True once heat has crossed the threshold. Stays true until dissipation
    /// brings it back below `max_heat`.
    pub fn is_overheated(&self) -> bool {
        self.heat >= self.max_heat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_consume_succeeds_with_enough_charge() {
        let mut c = Capacitor::new(100.0, 5.0);
        assert!(c.try_consume(30.0));
        assert!((c.charge - 70.0).abs() < 1e-4);
    }

    #[test]
    fn try_consume_fails_without_enough_charge() {
        let mut c = Capacitor::new(20.0, 5.0);
        assert!(!c.try_consume(30.0));
        // Charge untouched on failure.
        assert!((c.charge - 20.0).abs() < 1e-4);
    }

    #[test]
    fn try_consume_zero_is_always_ok() {
        let mut c = Capacitor::new(0.0, 0.0);
        assert!(c.try_consume(0.0));
        assert!(c.try_consume(-5.0));
    }

    #[test]
    fn recharge_caps_at_max() {
        let mut c = Capacitor::new(100.0, 40.0);
        c.charge = 80.0;
        c.recharge();
        assert!((c.charge - 100.0).abs() < 1e-4);
    }

    #[test]
    fn fraction_reports_fill_ratio() {
        let c = Capacitor::new(100.0, 0.0);
        assert!((c.fraction() - 1.0).abs() < 1e-4);
        let mut c2 = Capacitor::new(200.0, 0.0);
        c2.charge = 50.0;
        assert!((c2.fraction() - 0.25).abs() < 1e-4);
    }

    #[test]
    fn add_heat_accumulates_and_dissipates() {
        let mut h = HeatSinks::new(100.0, 10.0);
        h.add_heat(30.0);
        h.add_heat(20.0);
        assert!((h.heat - 50.0).abs() < 1e-4);
        h.tick_dissipation();
        assert!((h.heat - 40.0).abs() < 1e-4);
    }

    #[test]
    fn is_overheated_triggers_at_max() {
        let mut h = HeatSinks::new(50.0, 1.0);
        assert!(!h.is_overheated());
        h.add_heat(60.0);
        assert!(h.is_overheated());
        // After dissipation brings heat below max, flag clears.
        for _ in 0..20 {
            h.tick_dissipation();
        }
        assert!(!h.is_overheated());
    }

    #[test]
    fn dissipation_floors_at_zero() {
        let mut h = HeatSinks::new(100.0, 5.0);
        h.add_heat(2.0);
        h.tick_dissipation();
        assert!(h.heat.abs() < 1e-4);
        h.tick_dissipation();
        assert!(h.heat.abs() < 1e-4);
    }

    #[test]
    fn negative_heat_input_ignored() {
        let mut h = HeatSinks::new(100.0, 5.0);
        h.add_heat(-30.0);
        assert!(h.heat.abs() < 1e-4);
    }

    #[test]
    fn capacitor_serde_round_trip() {
        let c = Capacitor::new(250.0, 3.5);
        let bytes = bincode::serialize(&c).unwrap();
        let restored: Capacitor = bincode::deserialize(&bytes).unwrap();
        assert!((restored.charge - c.charge).abs() < 1e-4);
        assert!((restored.max_charge - c.max_charge).abs() < 1e-4);
        assert!((restored.recharge_rate - c.recharge_rate).abs() < 1e-4);
    }

    #[test]
    fn heatsinks_serde_round_trip() {
        let h = HeatSinks::new(150.0, 2.0);
        let bytes = bincode::serialize(&h).unwrap();
        let restored: HeatSinks = bincode::deserialize(&bytes).unwrap();
        assert!((restored.max_heat - h.max_heat).abs() < 1e-4);
        assert!((restored.dissipation - h.dissipation).abs() < 1e-4);
    }
}
