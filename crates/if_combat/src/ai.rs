// ai.rs: Silica Swarm NPC AI.
//
// The Silica Swarm is Infinite Flux Vibe's faceless antagonist — machine-
// mind drones that patrol the void and attack anything with a player
// transponder. This module ships the simplest possible behavior loop:
// pick the nearest player, close to `preferred_range`, and open fire.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::capacitor::{Capacitor, HeatSinks};
use crate::damage::ShipHealth;
use crate::position::CombatPosition;
use crate::targeting::Targeting;
use crate::weapon::Weapon;

/// Tag component identifying a ship that carries a player (so the AI knows
/// what to shoot at). Internally just a marker — clients add it to ships
/// that represent their character.
#[derive(Component, Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct PlayerShip;

/// Silica Swarm AI state. Aggression scales the hunt radius and firing
/// cadence later — for now it's used as a threshold below which the drone
/// simply idles.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct SilicaSwarmAI {
    pub aggression: f32,
    pub preferred_range: f32,
    #[serde(skip)]
    pub current_target: Option<Entity>,
    /// How fast this drone can close/withdraw per tick.
    pub speed: f32,
}

impl Default for SilicaSwarmAI {
    fn default() -> Self {
        Self {
            aggression: 1.0,
            preferred_range: 1000.0,
            current_target: None,
            speed: 20.0,
        }
    }
}

impl SilicaSwarmAI {
    pub fn new(aggression: f32, preferred_range: f32, speed: f32) -> Self {
        Self {
            aggression,
            preferred_range,
            current_target: None,
            speed,
        }
    }
}

/// AI tick. Runs once per frame.
///
/// 1. Each drone scans for the nearest entity tagged `PlayerShip` and sets
///    it as both `current_target` and the `Targeting` target.
/// 2. If a target exists, compute the distance and move `speed` units
///    toward or away to hug `preferred_range` (with a small deadband).
/// 3. When within optimal range and the lock is complete, damage is dealt
///    by `combat_tick_system` elsewhere — the AI just sets up the kinematics.
pub fn silica_ai_system(
    mut drones: Query<
        (
            &mut SilicaSwarmAI,
            &mut CombatPosition,
            &mut Targeting,
            Option<&ShipHealth>,
        ),
        Without<PlayerShip>,
    >,
    players: Query<(Entity, &CombatPosition), With<PlayerShip>>,
) {
    // Cache player positions; we re-use for every drone.
    let player_list: Vec<(Entity, CombatPosition)> = players.iter().map(|(e, p)| (e, *p)).collect();

    for (mut ai, mut pos, mut tgt, health) in &mut drones {
        // Dead drones have nothing to do. `ShipHealth` is optional so test
        // setups without health still exercise the AI.
        if let Some(h) = health
            && h.is_destroyed()
        {
            continue;
        }

        // Find nearest player.
        let Some((target_entity, target_pos)) = player_list
            .iter()
            .map(|(e, p)| (*e, *p, pos.distance(*p)))
            .min_by(|(_, _, a), (_, _, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(e, p, _)| (e, p))
        else {
            // No players around: clear target and idle.
            ai.current_target = None;
            tgt.clear();
            continue;
        };

        // Re-acquire if the target changed.
        if ai.current_target != Some(target_entity) {
            ai.current_target = Some(target_entity);
            tgt.set_target(target_entity);
        } else if tgt.target != Some(target_entity) {
            // Something else cleared the targeting component — re-lock.
            tgt.set_target(target_entity);
        }

        // Kinematics: hold `preferred_range`.
        let distance = pos.distance(target_pos);
        let deadband = ai.preferred_range * 0.05; // 5% hysteresis
        if distance > ai.preferred_range + deadband {
            pos.move_toward(target_pos, ai.speed);
        } else if distance < ai.preferred_range - deadband {
            pos.move_away(target_pos, ai.speed);
        }
    }
}

/// Lightweight helper for tests: decide whether a drone in the given state
/// *would* fire this tick, without actually needing the combat tick system.
/// Not wired into the plugin — just a pure function we can unit-test.
pub fn drone_would_fire(
    weapon: &Weapon,
    cap: &Capacitor,
    heat: &HeatSinks,
    targeting: &Targeting,
    distance: f32,
) -> bool {
    if !targeting.is_locked() {
        return false;
    }
    if !weapon.can_fire(cap, heat) {
        return false;
    }
    distance <= weapon.stats.optimal_range + 3.0 * weapon.stats.falloff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::WeaponStats;

    #[test]
    fn ai_picks_nearest_player() {
        let mut app = App::new();
        app.add_systems(Update, silica_ai_system);

        let far = app
            .world_mut()
            .spawn((PlayerShip, CombatPosition::new(5000.0, 0.0)))
            .id();
        let near = app
            .world_mut()
            .spawn((PlayerShip, CombatPosition::new(500.0, 0.0)))
            .id();
        let drone = app
            .world_mut()
            .spawn((
                SilicaSwarmAI::new(1.0, 1000.0, 10.0),
                CombatPosition::new(0.0, 0.0),
                Targeting::new(20),
            ))
            .id();

        app.update();

        let ai = app.world().get::<SilicaSwarmAI>(drone).unwrap();
        assert_eq!(ai.current_target, Some(near));
        let tgt = app.world().get::<Targeting>(drone).unwrap();
        assert_eq!(tgt.target, Some(near));
        // Distant ship intentionally not chosen.
        assert_ne!(ai.current_target, Some(far));
    }

    #[test]
    fn ai_approaches_when_too_far() {
        let mut app = App::new();
        app.add_systems(Update, silica_ai_system);

        app.world_mut()
            .spawn((PlayerShip, CombatPosition::new(2000.0, 0.0)));
        let drone = app
            .world_mut()
            .spawn((
                SilicaSwarmAI::new(1.0, 500.0, 50.0),
                CombatPosition::new(0.0, 0.0),
                Targeting::new(20),
            ))
            .id();

        app.update();
        let pos = app.world().get::<CombatPosition>(drone).unwrap();
        // Moved toward the player by roughly `speed`.
        assert!(pos.x > 0.0);
        assert!(pos.x <= 50.1);
    }

    #[test]
    fn ai_backs_off_when_too_close() {
        let mut app = App::new();
        app.add_systems(Update, silica_ai_system);

        app.world_mut()
            .spawn((PlayerShip, CombatPosition::new(100.0, 0.0)));
        let drone = app
            .world_mut()
            .spawn((
                SilicaSwarmAI::new(1.0, 1000.0, 30.0),
                CombatPosition::new(50.0, 0.0),
                Targeting::new(20),
            ))
            .id();

        app.update();
        let pos = app.world().get::<CombatPosition>(drone).unwrap();
        // Moved directly away from the player (x decreases).
        assert!(pos.x < 50.0);
    }

    #[test]
    fn ai_holds_position_within_deadband() {
        let mut app = App::new();
        app.add_systems(Update, silica_ai_system);

        app.world_mut()
            .spawn((PlayerShip, CombatPosition::new(1000.0, 0.0)));
        let drone = app
            .world_mut()
            .spawn((
                SilicaSwarmAI::new(1.0, 1000.0, 10.0),
                // Within 5% of preferred range.
                CombatPosition::new(10.0, 0.0),
                Targeting::new(20),
            ))
            .id();

        app.update();
        let pos = app.world().get::<CombatPosition>(drone).unwrap();
        assert!((pos.x - 10.0).abs() < 1e-4);
    }

    #[test]
    fn ai_idles_without_players() {
        let mut app = App::new();
        app.add_systems(Update, silica_ai_system);

        let drone = app
            .world_mut()
            .spawn((
                SilicaSwarmAI::new(1.0, 1000.0, 10.0),
                CombatPosition::new(0.0, 0.0),
                Targeting::new(20),
            ))
            .id();

        app.update();
        let ai = app.world().get::<SilicaSwarmAI>(drone).unwrap();
        assert!(ai.current_target.is_none());
        let tgt = app.world().get::<Targeting>(drone).unwrap();
        assert!(tgt.target.is_none());
    }

    #[test]
    fn drone_would_fire_requires_lock_and_readiness() {
        let w = Weapon::new(WeaponStats::laser_mk1());
        let cap = Capacitor::new(100.0, 1.0);
        let heat = HeatSinks::new(100.0, 1.0);
        let mut tgt = Targeting::new(1);
        // No target -> no fire.
        assert!(!drone_would_fire(&w, &cap, &heat, &tgt, 100.0));
        tgt.set_target(Entity::from_raw_u32(1).unwrap());
        tgt.lock_progress = 1.0;
        // Within range -> fire.
        assert!(drone_would_fire(&w, &cap, &heat, &tgt, 100.0));
        // Way past max range -> no fire.
        assert!(!drone_would_fire(&w, &cap, &heat, &tgt, 10_000.0));
    }
}
