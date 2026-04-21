// targeting.rs: Target locks.
//
// Every ship that wants to engage a hostile must first achieve a target
// lock — a short timer during which the ship's sensors hand-shake with the
// target. This gates weapon fire so that targets always have a chance to
// break line-of-sight (in future phases) before being vaporized.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Targeting/lock state. A ship with no target has `target: None`. Setting
/// a new target starts `lock_progress` from 0.0; `targeting_system` advances
/// it over `lock_time` ticks until it reaches 1.0 and the ship can fire.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Targeting {
    #[serde(skip)]
    pub target: Option<Entity>,
    /// Lock progress in `[0.0, 1.0]`. 1.0 means locked and ready to shoot.
    pub lock_progress: f32,
    /// Ticks required to reach a full lock from scratch.
    pub lock_time: u32,
}

impl Default for Targeting {
    fn default() -> Self {
        Self {
            target: None,
            lock_progress: 0.0,
            lock_time: 60,
        }
    }
}

impl Targeting {
    pub fn new(lock_time: u32) -> Self {
        Self {
            target: None,
            lock_progress: 0.0,
            lock_time,
        }
    }

    /// Switch target. Resets lock progress to zero, even if the new target
    /// is the same entity (sensors re-acquire).
    pub fn set_target(&mut self, target: Entity) {
        self.target = Some(target);
        self.lock_progress = 0.0;
    }

    /// Drop target entirely.
    pub fn clear(&mut self) {
        self.target = None;
        self.lock_progress = 0.0;
    }

    /// True once the lock is fully established.
    pub fn is_locked(&self) -> bool {
        self.target.is_some() && self.lock_progress >= 1.0
    }

    /// Advance the lock by one tick. Callers typically run this from
    /// `targeting_system` but the method is exposed so tests and AI code
    /// can drive it directly.
    pub fn tick(&mut self) {
        if self.target.is_none() || self.lock_time == 0 {
            // Instant-lock ships finish on the first tick with a target.
            if self.target.is_some() {
                self.lock_progress = 1.0;
            }
            return;
        }
        let step = 1.0 / self.lock_time as f32;
        self.lock_progress = (self.lock_progress + step).min(1.0);
    }
}

/// Advance every ship's target lock by one tick and clear locks whose
/// target has despawned. Running this as a Bevy system keeps the ECS in
/// sync with entity lifetimes — when a target ship is removed (destroyed
/// or unspawned for any reason) its attackers lose the lock automatically.
///
/// We detect "target despawned" by looking up the entity via `Entities`.
/// `Entities::contains` returns true for live entities and false for any
/// entity id that has been recycled or never existed.
pub fn targeting_system(
    mut targeters: Query<&mut Targeting>,
    entities: &bevy::ecs::entity::Entities,
) {
    for mut t in &mut targeters {
        if let Some(target) = t.target
            && !entities.contains(target)
        {
            t.clear();
            continue;
        }
        t.tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_targeting_has_no_lock() {
        let t = Targeting::new(30);
        assert!(t.target.is_none());
        assert!((t.lock_progress - 0.0).abs() < 1e-4);
        assert!(!t.is_locked());
    }

    #[test]
    fn set_target_resets_progress() {
        let mut t = Targeting::new(30);
        t.lock_progress = 0.5;
        let e = Entity::from_raw_u32(42).unwrap();
        t.set_target(e);
        assert_eq!(t.target, Some(e));
        assert!((t.lock_progress - 0.0).abs() < 1e-4);
    }

    #[test]
    fn tick_advances_progress_linearly() {
        let mut t = Targeting::new(4);
        let e = Entity::from_raw_u32(1).unwrap();
        t.set_target(e);
        t.tick();
        assert!((t.lock_progress - 0.25).abs() < 1e-4);
        t.tick();
        t.tick();
        t.tick();
        assert!((t.lock_progress - 1.0).abs() < 1e-4);
        assert!(t.is_locked());
    }

    #[test]
    fn tick_caps_at_one() {
        let mut t = Targeting::new(2);
        let e = Entity::from_raw_u32(1).unwrap();
        t.set_target(e);
        for _ in 0..10 {
            t.tick();
        }
        assert!((t.lock_progress - 1.0).abs() < 1e-4);
    }

    #[test]
    fn tick_noop_without_target() {
        let mut t = Targeting::new(10);
        for _ in 0..5 {
            t.tick();
        }
        assert!((t.lock_progress - 0.0).abs() < 1e-4);
    }

    #[test]
    fn clear_drops_target_and_progress() {
        let mut t = Targeting::new(10);
        let e = Entity::from_raw_u32(7).unwrap();
        t.set_target(e);
        t.tick();
        t.tick();
        t.clear();
        assert!(t.target.is_none());
        assert!((t.lock_progress - 0.0).abs() < 1e-4);
    }

    #[test]
    fn targeting_system_clears_lock_on_despawn() {
        let mut app = App::new();
        let target = app.world_mut().spawn_empty().id();
        let attacker = app
            .world_mut()
            .spawn({
                let mut t = Targeting::new(4);
                t.set_target(target);
                t
            })
            .id();
        // Schedule the targeting system so we exercise the Bevy wiring.
        app.add_systems(Update, targeting_system);
        app.update();
        {
            let t = app.world().get::<Targeting>(attacker).unwrap();
            assert_eq!(t.target, Some(target));
            assert!(t.lock_progress > 0.0);
        }
        // Despawn the target and run again — the system should clear.
        app.world_mut().despawn(target);
        app.update();
        let t = app.world().get::<Targeting>(attacker).unwrap();
        assert!(t.target.is_none());
        assert!((t.lock_progress - 0.0).abs() < 1e-4);
    }

    #[test]
    fn targeting_serde_skips_entity() {
        let mut t = Targeting::new(20);
        t.set_target(Entity::from_raw_u32(99).unwrap());
        t.lock_progress = 0.6;
        let bytes = bincode::serialize(&t).unwrap();
        let restored: Targeting = bincode::deserialize(&bytes).unwrap();
        // Entity skipped -> None after round-trip.
        assert!(restored.target.is_none());
        assert!((restored.lock_progress - 0.6).abs() < 1e-4);
        assert_eq!(restored.lock_time, 20);
    }
}
