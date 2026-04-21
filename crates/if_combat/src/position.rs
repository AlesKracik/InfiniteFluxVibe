// position.rs: Tactical-grid positions for ships in combat.
//
// The world crate tracks ships by `ShipLocation` (docked at body X, en
// route from A to B). Combat needs finer granularity — a 2D position
// within an engagement. `CombatPosition` is a lightweight component ships
// gain when they undock/engage; it sits outside `if_world` so the travel
// system doesn't need to know about combat positions.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// 2D tactical position. Unit is "distance units" — the same units used by
/// weapon ranges. We don't couple to `Transform` so saves can round-trip
/// cleanly without requiring Bevy's rendering types.
#[derive(Component, Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CombatPosition {
    pub x: f32,
    pub y: f32,
}

impl CombatPosition {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn from_vec2(v: Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }

    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    pub fn distance(self, other: CombatPosition) -> f32 {
        self.to_vec2().distance(other.to_vec2())
    }

    pub fn direction_to(self, other: CombatPosition) -> Vec2 {
        let delta = other.to_vec2() - self.to_vec2();
        let len = delta.length();
        if len > f32::EPSILON {
            delta / len
        } else {
            Vec2::ZERO
        }
    }

    /// Move `amount` units toward `target` (not past it).
    pub fn move_toward(&mut self, target: CombatPosition, amount: f32) {
        let dir = self.direction_to(target);
        let dist = self.distance(target);
        let step = amount.min(dist);
        let v = self.to_vec2() + dir * step;
        self.x = v.x;
        self.y = v.y;
    }

    /// Move `amount` units directly away from `target`.
    pub fn move_away(&mut self, target: CombatPosition, amount: f32) {
        let dir = target.direction_to(*self);
        let v = self.to_vec2() + dir * amount;
        self.x = v.x;
        self.y = v.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_euclidean() {
        let a = CombatPosition::new(0.0, 0.0);
        let b = CombatPosition::new(3.0, 4.0);
        assert!((a.distance(b) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn move_toward_does_not_overshoot() {
        let mut a = CombatPosition::new(0.0, 0.0);
        let b = CombatPosition::new(10.0, 0.0);
        a.move_toward(b, 100.0);
        assert!((a.x - 10.0).abs() < 1e-4);
    }

    #[test]
    fn move_away_goes_opposite_direction() {
        let mut a = CombatPosition::new(0.0, 0.0);
        let target = CombatPosition::new(10.0, 0.0);
        a.move_away(target, 5.0);
        assert!(a.x < 0.0);
    }

    #[test]
    fn direction_zero_when_coincident() {
        let a = CombatPosition::new(5.0, 5.0);
        let b = CombatPosition::new(5.0, 5.0);
        let d = a.direction_to(b);
        assert!(d.length() < 1e-4);
    }

    #[test]
    fn combat_position_serde_round_trip() {
        let p = CombatPosition::new(42.5, -7.25);
        let bytes = bincode::serialize(&p).unwrap();
        let restored: CombatPosition = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored, p);
    }
}
