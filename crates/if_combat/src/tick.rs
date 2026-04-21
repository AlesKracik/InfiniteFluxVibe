// tick.rs: The central combat update.
//
// `combat_tick_system` runs every frame and:
// 1. Decrements weapon cooldowns.
// 2. Recharges capacitor, dissipates heat, ticks shield regen.
// 3. For each weapon that can fire against a locked target, computes
//    distance-adjusted damage and routes it through `ShipHealth`.
// 4. Kills ships whose hull hits zero, emitting `ShipDestroyedEvent` and
//    spawning a `LootContainer`.
//
// We intentionally avoid coupling to `if_world::Ship` here — combat only
// needs position + HP + weapons + cap/heat. That makes the system easier
// to unit-test and lets clients reuse it for non-ship combatants later.

use bevy::prelude::*;
use if_factory::inventory::Inventory;

use crate::ai::silica_ai_system;
use crate::capacitor::{Capacitor, HeatSinks};
use crate::damage::ShipHealth;
use crate::fleet::FleetCommand;
use crate::loot::{
    DEFAULT_LOOT_LIFETIME_TICKS, LOOT_SURVIVAL_FRACTION, LootContainer, ShipDestroyedEvent,
    compute_drop, loot_decay_system,
};
use crate::position::CombatPosition;
use crate::targeting::{Targeting, targeting_system};
use crate::weapon::Weapon;

/// Pull the trigger: consume capacitor and ammo, add heat, start cooldown,
/// return the actual damage dealt (0 if something blocked the shot).
///
/// Tracking quality is currently fixed at 1.0 — future iterations can
/// factor in relative angular velocity vs a turret tracking stat.
fn try_fire_weapon(
    weapon: &mut Weapon,
    cap: &mut Capacitor,
    heat: &mut HeatSinks,
    ammo: Option<&mut Inventory>,
    distance: f32,
) -> f32 {
    if !weapon.can_fire(cap, heat) {
        return 0.0;
    }
    // Ammo check up-front — don't burn cap if we're empty.
    if let Some(item) = weapon.stats.ammo_item {
        let needed = 1;
        match ammo.as_deref() {
            Some(inv) if inv.count(item) >= needed => {}
            _ => return 0.0,
        }
    }
    if !cap.try_consume(weapon.stats.cap_cost) {
        return 0.0;
    }
    if let Some(item) = weapon.stats.ammo_item
        && let Some(inv) = ammo
    {
        let removed = inv.try_remove(item, 1);
        if removed == 0 {
            // Theoretically shouldn't happen given the check above, but
            // refund cap if it did.
            cap.charge += weapon.stats.cap_cost;
            return 0.0;
        }
    }
    heat.add_heat(weapon.stats.heat_per_shot);
    weapon.start_cooldown();
    weapon.damage_at_range(distance, 1.0)
}

/// Top-level combat update. See module docstring for flow.
///
/// Uses a single `ParamSet` to access the attacker side (weapons, cap, heat,
/// targeting, position, optional ammo inventory) and the target side
/// (health, position, optional loot inventory) because an entity may
/// appear on both sides — ships shoot *and* get shot — so Bevy rightly
/// refuses two overlapping mutable queries as disjoint params.
#[allow(clippy::type_complexity)]
pub fn combat_tick_system(
    mut commands: Commands,
    mut queries: ParamSet<(
        // Attackers.
        Query<(
            Entity,
            &'static mut Weapon,
            &'static mut Capacitor,
            &'static mut HeatSinks,
            &'static Targeting,
            &'static CombatPosition,
            Option<&'static mut Inventory>,
        )>,
        // Everyone with health (both target list and regen ticker).
        Query<(
            Entity,
            &'static mut ShipHealth,
            Option<&'static CombatPosition>,
            Option<&'static mut Inventory>,
        )>,
    )>,
    mut destroyed: MessageWriter<ShipDestroyedEvent>,
) {
    // --- 1. Tick cooldowns and capacitor/heat on attackers ---
    for (_, mut weapon, mut cap, mut heat, _, _, _) in &mut queries.p0() {
        weapon.tick_cooldown();
        cap.recharge();
        heat.tick_dissipation();
    }

    // --- 2. Shield regen for every entity with ShipHealth. Running here
    // (before we apply damage this tick) intentionally mirrors "state
    // ticks before new hits are processed," which is what the unit tests
    // in the damage module assume when they call
    // `tick_shield_regen`/`apply_damage` in that order.
    for (_, mut health, _, _) in &mut queries.p1() {
        health.tick_shield_regen();
    }

    // --- 3. Gather hits. We read target positions from the health query
    // after releasing the attacker borrow, which is why we do two passes.
    let mut target_positions: std::collections::HashMap<Entity, Vec2> =
        std::collections::HashMap::new();
    for (e, _, pos, _) in queries.p1().iter() {
        if let Some(p) = pos {
            target_positions.insert(e, p.to_vec2());
        }
    }

    let mut hits: Vec<(Entity, f32, crate::damage::DamageType)> = Vec::new();
    for (_entity, mut weapon, mut cap, mut heat, targeting, pos, inv) in &mut queries.p0() {
        if !targeting.is_locked() {
            continue;
        }
        let Some(target_entity) = targeting.target else {
            continue;
        };
        let Some(target_pos) = target_positions.get(&target_entity).copied() else {
            continue;
        };
        let distance = pos.to_vec2().distance(target_pos);
        let dmg_type = weapon.stats.damage_type;
        let damage = try_fire_weapon(
            &mut weapon,
            &mut cap,
            &mut heat,
            inv.map(|i| i.into_inner()),
            distance,
        );
        if damage > 0.0 {
            hits.push((target_entity, damage, dmg_type));
        }
    }

    // --- 4. Apply damage ---
    let mut destructions: Vec<(Entity, Vec2, Vec<(if_common::item::ItemType, u32)>)> = Vec::new();
    {
        let mut health_q = queries.p1();
        for (target_entity, damage, dmg_type) in hits {
            let Ok((_, mut health, target_pos, target_inv)) = health_q.get_mut(target_entity)
            else {
                continue;
            };
            if health.is_destroyed() {
                continue;
            }
            health.apply_damage(damage, dmg_type);
            if health.is_destroyed() {
                let items: Vec<(if_common::item::ItemType, u32)> = target_inv
                    .as_deref()
                    .map(|inv| {
                        inv.contents()
                            .into_iter()
                            .map(|s| (s.item, s.quantity))
                            .collect()
                    })
                    .unwrap_or_default();
                let loot = compute_drop(&items, LOOT_SURVIVAL_FRACTION);
                let pos = target_pos.map(|p| p.to_vec2()).unwrap_or(Vec2::ZERO);
                destructions.push((target_entity, pos, loot));
            }
        }
    }

    // --- 5. Destruction: despawn target, emit event, spawn wreckage ---
    for (target_entity, pos, loot) in destructions {
        destroyed.write(ShipDestroyedEvent {
            ship: target_entity,
            position: pos,
            loot: loot.clone(),
        });
        if !loot.is_empty() {
            commands.spawn((
                LootContainer::new(loot, DEFAULT_LOOT_LIFETIME_TICKS),
                CombatPosition::from_vec2(pos),
            ));
        }
        commands.entity(target_entity).despawn();
    }
}

/// The Bevy plugin for all combat systems and messages. Callers add this
/// alongside `WorldPlugin` and get the full combat loop.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FleetCommand>()
            .add_message::<ShipDestroyedEvent>()
            .add_systems(
                Update,
                (
                    targeting_system,
                    silica_ai_system,
                    combat_tick_system,
                    loot_decay_system,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::{DamageType, Resistances};
    use crate::weapon::WeaponStats;
    use if_common::item::ItemType;

    /// Build a very close-range weapon that uses no cap and no ammo and
    /// has zero cooldown so tests land hits on the first update.
    fn instant_kill_laser() -> WeaponStats {
        WeaponStats {
            kind: crate::weapon::WeaponKind::Laser,
            damage: 10_000.0,
            damage_type: DamageType::EM,
            optimal_range: 10_000.0,
            falloff: 1.0,
            cooldown: 0,
            cap_cost: 0.0,
            heat_per_shot: 0.0,
            ammo_item: None,
        }
    }

    #[test]
    fn plugin_registers_messages_and_systems() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);
        // Writing a `FleetCommand` must succeed without panicking (it would
        // panic if the message wasn't registered).
        app.world_mut().write_message(FleetCommand::Disengage);
        // Same for ShipDestroyedEvent.
        app.world_mut().write_message(ShipDestroyedEvent {
            ship: Entity::from_raw_u32(1).unwrap(),
            position: Vec2::ZERO,
            loot: Vec::new(),
        });
    }

    #[test]
    fn combat_tick_ticks_cooldowns_and_regen() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);
        // An attacker with no target — cooldowns/cap/heat still tick.
        let mut w = Weapon::new(WeaponStats::laser_mk1());
        w.start_cooldown();
        let mut cap = Capacitor::new(100.0, 5.0);
        cap.charge = 50.0;
        let mut heat = HeatSinks::new(100.0, 5.0);
        heat.add_heat(40.0);
        let attacker = app
            .world_mut()
            .spawn((
                w,
                cap,
                heat,
                Targeting::new(20),
                CombatPosition::new(0.0, 0.0),
            ))
            .id();
        app.update();
        let cap = app.world().get::<Capacitor>(attacker).unwrap();
        let heat = app.world().get::<HeatSinks>(attacker).unwrap();
        let w = app.world().get::<Weapon>(attacker).unwrap();
        assert!(cap.charge > 50.0, "cap should recharge");
        assert!(heat.heat < 40.0, "heat should dissipate");
        assert_eq!(w.ticks_until_ready, w.stats.cooldown - 1);
    }

    #[test]
    fn locked_weapon_in_range_deals_damage() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);

        let target = app
            .world_mut()
            .spawn((
                ShipHealth::new(500.0, 0.0, 0.0, 0.0),
                CombatPosition::new(100.0, 0.0),
                Inventory::new(10),
            ))
            .id();
        let mut targeting = Targeting::new(1);
        targeting.set_target(target);
        targeting.lock_progress = 1.0;
        app.world_mut().spawn((
            Weapon::new(instant_kill_laser()),
            Capacitor::new(10.0, 0.0),
            HeatSinks::new(10.0, 0.0),
            targeting,
            CombatPosition::new(0.0, 0.0),
        ));

        app.update();
        // Target should be destroyed and despawned.
        assert!(
            app.world().get_entity(target).is_err(),
            "target should despawn"
        );
    }

    #[test]
    fn out_of_range_shot_deals_zero() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);

        let target = app
            .world_mut()
            .spawn((
                ShipHealth::new(500.0, 0.0, 0.0, 0.0),
                CombatPosition::new(50_000.0, 0.0),
                Inventory::new(10),
            ))
            .id();
        let mut targeting = Targeting::new(1);
        targeting.set_target(target);
        targeting.lock_progress = 1.0;
        app.world_mut().spawn((
            Weapon::new(WeaponStats::laser_mk1()),
            Capacitor::new(1000.0, 0.0),
            HeatSinks::new(1000.0, 0.0),
            targeting,
            CombatPosition::new(0.0, 0.0),
        ));

        app.update();
        let h = app.world().get::<ShipHealth>(target).unwrap();
        // Untouched hull.
        assert!((h.hull - 500.0).abs() < 1e-4);
    }

    #[test]
    fn unlocked_weapon_does_not_fire() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);

        let target = app
            .world_mut()
            .spawn((
                ShipHealth::new(500.0, 0.0, 0.0, 0.0),
                CombatPosition::new(100.0, 0.0),
                Inventory::new(10),
            ))
            .id();
        // Long lock time so a single tick doesn't finish the lock even
        // after `targeting_system` runs before `combat_tick_system`.
        let mut targeting = Targeting::new(100);
        targeting.set_target(target);
        // progress stays near 0 after one tick (100 steps required).
        app.world_mut().spawn((
            Weapon::new(instant_kill_laser()),
            Capacitor::new(10.0, 0.0),
            HeatSinks::new(10.0, 0.0),
            targeting,
            CombatPosition::new(0.0, 0.0),
        ));

        app.update();
        assert!(app.world().get_entity(target).is_ok());
        let h = app.world().get::<ShipHealth>(target).unwrap();
        assert!((h.hull - 500.0).abs() < 1e-4);
    }

    #[test]
    fn destruction_spawns_loot_container() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);

        let mut inv = Inventory::new(50);
        inv.try_add(ItemType::HullPlate, 10);
        inv.try_add(ItemType::CopperOre, 20);
        let target = app
            .world_mut()
            .spawn((
                ShipHealth::new(10.0, 0.0, 0.0, 0.0),
                CombatPosition::new(5.0, 5.0),
                inv,
            ))
            .id();
        let mut targeting = Targeting::new(1);
        targeting.set_target(target);
        targeting.lock_progress = 1.0;
        app.world_mut().spawn((
            Weapon::new(instant_kill_laser()),
            Capacitor::new(10.0, 0.0),
            HeatSinks::new(10.0, 0.0),
            targeting,
            CombatPosition::new(0.0, 0.0),
        ));

        app.update();

        let wreck = app
            .world_mut()
            .query::<&LootContainer>()
            .iter(app.world())
            .next()
            .cloned();
        let Some(wreck) = wreck else {
            panic!("no LootContainer spawned");
        };
        // 50% drop of 10 HullPlate -> 5, and 20 CopperOre -> 10.
        let hull = wreck
            .items
            .iter()
            .find(|(it, _)| *it == ItemType::HullPlate)
            .map(|(_, q)| *q);
        let ore = wreck
            .items
            .iter()
            .find(|(it, _)| *it == ItemType::CopperOre)
            .map(|(_, q)| *q);
        assert_eq!(hull, Some(5));
        assert_eq!(ore, Some(10));
    }

    #[test]
    fn shield_resistance_reduces_weapon_damage_through_apply() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);
        let mut h = ShipHealth::new(100.0, 0.0, 500.0, 0.0);
        h.shield_resistances = Resistances::new(0.0, 0.0, 0.9, 0.0); // EM resist
        let target = app
            .world_mut()
            .spawn((h, CombatPosition::new(100.0, 0.0), Inventory::new(10)))
            .id();
        let mut targeting = Targeting::new(1);
        targeting.set_target(target);
        targeting.lock_progress = 1.0;
        app.world_mut().spawn((
            Weapon::new(WeaponStats {
                kind: crate::weapon::WeaponKind::Laser,
                damage: 100.0,
                damage_type: DamageType::EM,
                optimal_range: 10_000.0,
                falloff: 1.0,
                cooldown: 0,
                cap_cost: 0.0,
                heat_per_shot: 0.0,
                ammo_item: None,
            }),
            Capacitor::new(10.0, 0.0),
            HeatSinks::new(10.0, 0.0),
            targeting,
            CombatPosition::new(0.0, 0.0),
        ));

        app.update();

        let h = app.world().get::<ShipHealth>(target).unwrap();
        // 100 raw EM * 0.1 = 10 actual to shields. Shields drop to 490.
        assert!((h.shields - 490.0).abs() < 1e-4);
    }

    #[test]
    fn ammo_consumed_per_shot() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);

        let mut inv_attacker = Inventory::new(10);
        inv_attacker.try_add(ItemType::IronPlate, 3);
        let target = app
            .world_mut()
            .spawn((
                ShipHealth::new(1_000_000.0, 0.0, 0.0, 0.0),
                CombatPosition::new(100.0, 0.0),
                Inventory::new(10),
            ))
            .id();
        let mut targeting = Targeting::new(1);
        targeting.set_target(target);
        targeting.lock_progress = 1.0;
        let attacker = app
            .world_mut()
            .spawn((
                Weapon::new(WeaponStats {
                    kind: crate::weapon::WeaponKind::Autocannon,
                    damage: 1.0,
                    damage_type: DamageType::Kinetic,
                    optimal_range: 10_000.0,
                    falloff: 1.0,
                    cooldown: 0,
                    cap_cost: 0.0,
                    heat_per_shot: 0.0,
                    ammo_item: Some(ItemType::IronPlate),
                }),
                Capacitor::new(10.0, 0.0),
                HeatSinks::new(10.0, 0.0),
                targeting,
                CombatPosition::new(0.0, 0.0),
                inv_attacker,
            ))
            .id();

        app.update();
        let inv = app.world().get::<Inventory>(attacker).unwrap();
        assert_eq!(inv.count(ItemType::IronPlate), 2);
    }

    #[test]
    fn no_ammo_blocks_firing() {
        let mut app = App::new();
        app.add_plugins(CombatPlugin);

        let target = app
            .world_mut()
            .spawn((
                ShipHealth::new(1_000_000.0, 0.0, 0.0, 0.0),
                CombatPosition::new(100.0, 0.0),
                Inventory::new(10),
            ))
            .id();
        let mut targeting = Targeting::new(1);
        targeting.set_target(target);
        targeting.lock_progress = 1.0;
        app.world_mut().spawn((
            Weapon::new(WeaponStats {
                kind: crate::weapon::WeaponKind::Missile,
                damage: 1.0,
                damage_type: DamageType::Explosive,
                optimal_range: 10_000.0,
                falloff: 1.0,
                cooldown: 0,
                cap_cost: 0.0,
                heat_per_shot: 0.0,
                ammo_item: Some(ItemType::BasicCircuit),
            }),
            Capacitor::new(10.0, 0.0),
            HeatSinks::new(10.0, 0.0),
            targeting,
            CombatPosition::new(0.0, 0.0),
            Inventory::new(10), // empty
        ));

        app.update();
        let h = app.world().get::<ShipHealth>(target).unwrap();
        assert!((h.hull - 1_000_000.0).abs() < 1e-2);
    }
}
