// fitting.rs: Ship module slots, power grid, and CPU.
//
// Fitting a ship means stacking modules into its high/med/low slots while
// respecting two budgets: power grid (for armor, weapons, prop mods) and
// CPU (for electronic gear). We don't own the individual module catalogue
// here — each module type in the future will publish `power_cost` and
// `cpu_cost` that the fitting UI sums up.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Slot/budget configuration for a ship. Sits alongside `Ship` as a
/// component. Numbers mutate as modules are fitted/unfitted.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Default)]
pub struct ShipFit {
    pub high_slots: u32,
    pub med_slots: u32,
    pub low_slots: u32,
    /// Total power grid capacity.
    pub power_grid: f32,
    /// Power consumed by currently fitted modules.
    pub power_used: f32,
    /// CPU capacity.
    pub cpu: f32,
    /// CPU consumed by currently fitted modules.
    pub cpu_used: f32,
}

impl ShipFit {
    pub fn new(high_slots: u32, med_slots: u32, low_slots: u32, power: f32, cpu: f32) -> Self {
        Self {
            high_slots,
            med_slots,
            low_slots,
            power_grid: power,
            power_used: 0.0,
            cpu,
            cpu_used: 0.0,
        }
    }

    /// Remaining power grid capacity.
    pub fn power_free(&self) -> f32 {
        (self.power_grid - self.power_used).max(0.0)
    }

    /// Remaining CPU.
    pub fn cpu_free(&self) -> f32 {
        (self.cpu - self.cpu_used).max(0.0)
    }

    /// Try to reserve `power` and `cpu` for a fitted module. Returns `true`
    /// only if both fit; the reservation is not partial.
    pub fn try_allocate(&mut self, power: f32, cpu: f32) -> bool {
        if power > self.power_free() || cpu > self.cpu_free() {
            return false;
        }
        self.power_used += power.max(0.0);
        self.cpu_used += cpu.max(0.0);
        true
    }

    /// Release a previously-allocated reservation. Clamps at zero so
    /// double-frees can't make usage go negative.
    pub fn release(&mut self, power: f32, cpu: f32) {
        self.power_used = (self.power_used - power.max(0.0)).max(0.0);
        self.cpu_used = (self.cpu_used - cpu.max(0.0)).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fit_has_no_usage() {
        let f = ShipFit::new(4, 3, 2, 100.0, 50.0);
        assert_eq!(f.high_slots, 4);
        assert_eq!(f.med_slots, 3);
        assert_eq!(f.low_slots, 2);
        assert!((f.power_free() - 100.0).abs() < 1e-4);
        assert!((f.cpu_free() - 50.0).abs() < 1e-4);
    }

    #[test]
    fn try_allocate_respects_power_budget() {
        let mut f = ShipFit::new(4, 3, 2, 50.0, 100.0);
        assert!(f.try_allocate(30.0, 10.0));
        assert!((f.power_used - 30.0).abs() < 1e-4);
        assert!((f.cpu_used - 10.0).abs() < 1e-4);
        // Would overflow power.
        assert!(!f.try_allocate(30.0, 10.0));
        // Partial reservation was not performed.
        assert!((f.power_used - 30.0).abs() < 1e-4);
        assert!((f.cpu_used - 10.0).abs() < 1e-4);
    }

    #[test]
    fn try_allocate_respects_cpu_budget() {
        let mut f = ShipFit::new(4, 3, 2, 100.0, 20.0);
        assert!(!f.try_allocate(10.0, 30.0));
        assert!((f.cpu_used - 0.0).abs() < 1e-4);
    }

    #[test]
    fn release_returns_capacity() {
        let mut f = ShipFit::new(4, 3, 2, 100.0, 100.0);
        f.try_allocate(40.0, 20.0);
        f.release(40.0, 20.0);
        assert!((f.power_used - 0.0).abs() < 1e-4);
        assert!((f.cpu_used - 0.0).abs() < 1e-4);
    }

    #[test]
    fn release_does_not_go_negative() {
        let mut f = ShipFit::new(4, 3, 2, 100.0, 100.0);
        f.release(50.0, 50.0);
        assert!((f.power_used - 0.0).abs() < 1e-4);
        assert!((f.cpu_used - 0.0).abs() < 1e-4);
    }

    #[test]
    fn ship_fit_serde_round_trip() {
        let mut f = ShipFit::new(6, 4, 3, 200.0, 80.0);
        f.try_allocate(75.0, 30.0);
        let bytes = bincode::serialize(&f).unwrap();
        let restored: ShipFit = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.high_slots, f.high_slots);
        assert!((restored.power_used - f.power_used).abs() < 1e-4);
        assert!((restored.cpu_used - f.cpu_used).abs() < 1e-4);
    }
}
