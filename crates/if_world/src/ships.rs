// ships.rs: Ships, orbital stations, and inter-body travel.
//
// Phase 3 introduces interplanetary logistics. A `Ship` is an entity that
// carries cargo between `Surface`s and `OrbitalStation`s. Ships have fuel,
// speed, and a simple state machine (`ShipState`) covering docked / traveling
// / idle phases.
//
// The travel system is intentionally minimal: each tick it advances
// `progress` by a rate proportional to `speed / distance` and burns fuel.
// Geometry is reused from the bodies/station orbital data — we do not
// simulate full 2-body physics here. It is a game, not a rocketry sandbox.

use bevy::prelude::*;
use if_common::item::ItemType;
use if_factory::inventory::Inventory;
use serde::{Deserialize, Serialize};

use crate::bodies::{CelestialBody, StarSystem};

// -----------------------------------------------------------------------------
// Locations and ship state machine
// -----------------------------------------------------------------------------

/// A named destination a ship can dock at.
///
/// We key by `String` rather than `Entity` because entities are not stable
/// across save/load. The name comes from either a `CelestialBody::name` (for
/// `Surface`) or an `OrbitalStation::parent_body` (for `Station`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShipLocation {
    /// Docked at the surface of a body (body name).
    Surface(String),
    /// Docked at an orbital station that orbits the named body.
    Station(String),
}

impl ShipLocation {
    /// Name of the body this location sits at/around. Used when resolving
    /// positions from the ECS.
    pub fn body_name(&self) -> &str {
        match self {
            ShipLocation::Surface(n) => n,
            ShipLocation::Station(n) => n,
        }
    }
}

/// A hauler/transport ship.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Ship {
    pub name: String,
    /// Maximum cargo capacity. Mirrors the attached `Inventory::capacity` —
    /// kept here as well for UI display without needing to join components.
    pub cargo_capacity: u32,
    /// Speed in "units per tick" — orbit radius units per simulation tick.
    pub speed: f32,
    /// Fuel units remaining.
    pub fuel: f32,
    /// Max fuel capacity.
    pub fuel_capacity: f32,
}

impl Ship {
    /// Convenience constructor with a full fuel tank.
    pub fn new_full(
        name: impl Into<String>,
        cargo_capacity: u32,
        speed: f32,
        fuel_capacity: f32,
    ) -> Self {
        Self {
            name: name.into(),
            cargo_capacity,
            speed,
            fuel: fuel_capacity,
            fuel_capacity,
        }
    }
}

/// Ship state machine.
///
/// * `Docked` — parked at a station or surface, inactive.
/// * `Traveling` — moving between two locations; progress is 0.0..=1.0.
/// * `Idle` — in space near a body, not moving (e.g., out of fuel or between
///   jobs).
#[derive(Component, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ShipState {
    Docked {
        location: ShipLocation,
    },
    Traveling {
        from: ShipLocation,
        to: ShipLocation,
        /// Progress 0.0..=1.0.
        progress: f32,
    },
    Idle {
        near: ShipLocation,
    },
}

impl ShipState {
    /// True if the ship is currently moving between two locations.
    pub fn is_traveling(&self) -> bool {
        matches!(self, ShipState::Traveling { .. })
    }

    /// True if the ship can accept a new travel order (Docked or Idle).
    pub fn can_depart(&self) -> bool {
        matches!(self, ShipState::Docked { .. } | ShipState::Idle { .. })
    }

    /// Current or nearest known location.
    pub fn current_location(&self) -> &ShipLocation {
        match self {
            ShipState::Docked { location } => location,
            ShipState::Traveling { from, .. } => from,
            ShipState::Idle { near } => near,
        }
    }
}

// -----------------------------------------------------------------------------
// Orbital stations
// -----------------------------------------------------------------------------

/// An orbital station: a space-based logistics hub that orbits a body.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct OrbitalStation {
    pub name: String,
    /// Name of the body this station orbits.
    pub parent_body: String,
    /// Orbit radius around the parent body.
    pub orbit_radius: f32,
    /// Current orbital angle (radians).
    pub orbit_angle: f32,
}

impl OrbitalStation {
    /// World-space position of this station given the parent body's position.
    pub fn position(&self, parent_pos: Vec2) -> Vec2 {
        Vec2::new(
            parent_pos.x + self.orbit_radius * self.orbit_angle.cos(),
            parent_pos.y + self.orbit_radius * self.orbit_angle.sin(),
        )
    }
}

// -----------------------------------------------------------------------------
// Travel orders
// -----------------------------------------------------------------------------

/// A request from the UI (or AI) to move a ship to a new location.
///
/// Consumed by `process_travel_orders`. The system only honors the order if
/// the ship is currently `Docked` or `Idle` — otherwise the order is dropped
/// with a warning.
///
/// Bevy 0.18 split the old `Event` system into two: `Event` now refers to
/// observer-triggered events, and `Message` is the reader/writer-based
/// buffered variant. `TravelOrder` is the latter.
#[derive(bevy::ecs::message::Message, Clone, Debug)]
pub struct TravelOrder {
    pub ship: Entity,
    pub destination: ShipLocation,
}

// -----------------------------------------------------------------------------
// Cargo transfer
// -----------------------------------------------------------------------------

/// Move up to `quantity` of `item` from `from` to `to`.
///
/// Limited by (1) how much of `item` `from` actually has and (2) how much
/// space `to` has free. Returns the amount actually transferred.
pub fn transfer_cargo(
    from: &mut Inventory,
    to: &mut Inventory,
    item: ItemType,
    quantity: u32,
) -> u32 {
    if quantity == 0 {
        return 0;
    }
    let available = from.count(item);
    let space = to.space_available();
    let amount = quantity.min(available).min(space);
    if amount == 0 {
        return 0;
    }
    // Remove from source first, then add to destination. Since we already
    // capped `amount` by both bounds, both calls must succeed in full — but
    // we still assert for safety and to surface logic errors loudly.
    let removed = from.try_remove(item, amount);
    debug_assert_eq!(removed, amount);
    let added = to.try_add(item, removed);
    debug_assert_eq!(added, removed);
    added
}

// -----------------------------------------------------------------------------
// Systems
// -----------------------------------------------------------------------------

/// Advance traveling ships.
///
/// Per tick:
/// 1. Compute the euclidean distance between `from` and `to`.
/// 2. Progress increases by `speed / distance` (clamped).
/// 3. Fuel is consumed proportional to the distance traveled this tick.
/// 4. On `progress >= 1.0`, the ship docks at `to`.
/// 5. If fuel runs out mid-flight, the ship becomes `Idle { near: from }`
///    and a warning is logged.
pub fn ship_travel_system(
    mut ships: Query<(Entity, &mut Ship, &mut ShipState)>,
    bodies: Query<&CelestialBody>,
    stations: Query<&OrbitalStation>,
    system: Option<Res<StarSystem>>,
) {
    let positions = BodyPositions::build(system.as_deref(), &bodies);

    for (entity, mut ship, mut state) in &mut ships {
        // Snapshot the data we need out of the enum before we start mutating
        // it — Rust won't let us partially borrow an enum through a `&mut`.
        let (from, to, progress) = match &*state {
            ShipState::Traveling { from, to, progress } => (from.clone(), to.clone(), *progress),
            _ => continue,
        };

        let from_pos = match resolve_location_pos(&from, &positions, &stations) {
            Some(p) => p,
            None => {
                warn!(
                    ship = ?entity,
                    "ship travel: origin location '{}' no longer exists, stranding ship",
                    from.body_name()
                );
                *state = ShipState::Idle { near: from };
                continue;
            }
        };
        let to_pos = match resolve_location_pos(&to, &positions, &stations) {
            Some(p) => p,
            None => {
                warn!(
                    ship = ?entity,
                    "ship travel: destination '{}' no longer exists, stranding ship",
                    to.body_name()
                );
                *state = ShipState::Idle { near: from };
                continue;
            }
        };

        let distance = from_pos.distance(to_pos).max(f32::EPSILON);
        let delta = (ship.speed / distance).clamp(0.0, 1.0);

        // Fuel cost scales with distance actually covered this tick.
        let segment = delta * distance;
        let fuel_cost = segment * FUEL_PER_UNIT;

        if ship.fuel < fuel_cost {
            // Burn what we have, then strand.
            ship.fuel = 0.0;
            warn!(
                ship = ?entity,
                "ship '{}' ran out of fuel mid-transit, drifting near '{}'",
                ship.name,
                from.body_name()
            );
            *state = ShipState::Idle { near: from };
            continue;
        }
        ship.fuel -= fuel_cost;

        let new_progress = progress + delta;
        if new_progress >= 1.0 {
            *state = ShipState::Docked { location: to };
        } else {
            *state = ShipState::Traveling {
                from,
                to,
                progress: new_progress,
            };
        }
    }
}

/// Fuel consumed per unit of distance traveled.
///
/// Chosen so a full tank of the starter ships (~100 fuel) covers multiple
/// inter-planet hops with margin. Tune later.
const FUEL_PER_UNIT: f32 = 0.01;

/// Consume `TravelOrder` messages and transition eligible ships into
/// `Traveling` state.
pub fn process_travel_orders(
    mut orders: MessageReader<TravelOrder>,
    mut ships: Query<(&Ship, &mut ShipState)>,
) {
    for order in orders.read() {
        let Ok((ship, mut state)) = ships.get_mut(order.ship) else {
            warn!(
                "travel order for unknown ship entity {:?} — dropping",
                order.ship
            );
            continue;
        };

        // Source location is whatever the ship currently considers itself at.
        let from = match &*state {
            ShipState::Docked { location } => location.clone(),
            ShipState::Idle { near } => near.clone(),
            ShipState::Traveling { .. } => {
                warn!(
                    "ship '{}' is already traveling; ignoring travel order",
                    ship.name
                );
                continue;
            }
        };

        if from == order.destination {
            // No-op: already there. Keep state unchanged.
            continue;
        }

        *state = ShipState::Traveling {
            from,
            to: order.destination.clone(),
            progress: 0.0,
        };
    }
}

/// Advance orbital stations around their parent bodies.
///
/// Stations keep their own `orbit_angle` rather than reusing `CelestialBody`
/// so the UI can render them distinctly (crosshair/station icon) without
/// downstream code having to special-case certain body entities.
pub fn station_orbital_motion_system(mut stations: Query<&mut OrbitalStation>) {
    const ANGULAR_VELOCITY: f32 = 0.02;
    const TAU: f32 = std::f32::consts::TAU;
    for mut station in &mut stations {
        station.orbit_angle = (station.orbit_angle + ANGULAR_VELOCITY).rem_euclid(TAU);
    }
}

// -----------------------------------------------------------------------------
// Position helpers
// -----------------------------------------------------------------------------

/// Lookup table from body name → absolute world position.
///
/// Positions are computed once per tick from `CelestialBody` components and
/// reused for each ship we inspect. Moons are resolved after planets so their
/// parent position is already known.
struct BodyPositions {
    by_name: std::collections::HashMap<String, Vec2>,
}

impl BodyPositions {
    fn build(system: Option<&StarSystem>, bodies: &Query<&CelestialBody>) -> Self {
        let mut by_name = std::collections::HashMap::new();

        // First pass: any body that has no parent. These sit at the origin
        // offset only by their own orbit (for the star this is 0, for any
        // stray rootless body it's just their angle/radius).
        //
        // Second pass: bodies with a parent whose position we already know.
        // We repeat until nothing new lands to handle long parent chains
        // (planet → moon → sub-moon if we ever add it).
        //
        // Bevy `Query` doesn't let us iterate by entity id here without
        // another lookup, so we pull everything into a local Vec first.
        let entries: Vec<&CelestialBody> = if let Some(sys) = system {
            sys.bodies
                .iter()
                .filter_map(|e| bodies.get(*e).ok())
                .collect()
        } else {
            bodies.iter().collect()
        };

        // Seed: bodies without a parent (stars, rootless test bodies).
        let mut remaining: Vec<&CelestialBody> = Vec::new();
        for b in entries.iter().copied() {
            if b.parent.is_none() {
                by_name.insert(b.name.clone(), b.position(Vec2::ZERO));
            } else {
                remaining.push(b);
            }
        }

        // Resolve children by parent-name. We do not know the parent name
        // from an Entity alone, so we look it up via the query.
        let mut progress = true;
        while progress && !remaining.is_empty() {
            progress = false;
            remaining.retain(|b| {
                let parent_name = b
                    .parent
                    .and_then(|p| bodies.get(p).ok())
                    .map(|parent| parent.name.clone());
                let Some(parent_name) = parent_name else {
                    return true;
                };
                if let Some(parent_pos) = by_name.get(&parent_name).copied() {
                    by_name.insert(b.name.clone(), b.position(parent_pos));
                    progress = true;
                    false
                } else {
                    true
                }
            });
        }

        // Anything left in `remaining` has an unresolvable parent. Place it
        // at its own orbit offset (treating parent as origin) so at least
        // distances are sane.
        for b in remaining {
            by_name
                .entry(b.name.clone())
                .or_insert_with(|| b.position(Vec2::ZERO));
        }

        Self { by_name }
    }

    fn body(&self, name: &str) -> Option<Vec2> {
        self.by_name.get(name).copied()
    }
}

/// Resolve a `ShipLocation` to its absolute world-space position.
fn resolve_location_pos(
    loc: &ShipLocation,
    positions: &BodyPositions,
    stations: &Query<&OrbitalStation>,
) -> Option<Vec2> {
    match loc {
        ShipLocation::Surface(name) => positions.body(name),
        ShipLocation::Station(body_name) => {
            let parent_pos = positions.body(body_name)?;
            // Find the station orbiting this body (first match is fine;
            // multiple stations per body are allowed but each is named).
            let station = stations.iter().find(|s| s.parent_body == *body_name)?;
            Some(station.position(parent_pos))
        }
    }
}

// -----------------------------------------------------------------------------
// Plugin registration
// -----------------------------------------------------------------------------

/// Bevy plugin that registers the ship/station systems and the `TravelOrder`
/// event.
///
/// Kept separate from `WorldPlugin` so that tests and alternate clients can
/// pull just the body/orbit simulation without dragging in ships.
pub struct ShipsPlugin;

impl Plugin for ShipsPlugin {
    fn build(&self, app: &mut App) {
        // `add_message` replaces Bevy 0.17's `add_event` for reader/writer-
        // based (buffered) messages. Observer-style events use the new
        // `Event` trait and don't need this registration.
        app.add_message::<TravelOrder>().add_systems(
            Update,
            (
                station_orbital_motion_system,
                process_travel_orders,
                ship_travel_system,
            )
                .chain(),
        );
    }
}

/// Startup helper: spawn two ships at the home planet surface and one
/// orbital station.
///
/// Callers hand us the home planet's name (obtained after
/// `spawn_star_system` has run) so that `ShipLocation::Surface(..)` matches
/// the body names exactly.
pub fn spawn_starter_ships_and_station(commands: &mut Commands, home_body_name: &str) {
    // Station orbiting the home planet.
    commands.spawn((
        OrbitalStation {
            name: format!("{home_body_name} Orbital"),
            parent_body: home_body_name.to_string(),
            orbit_radius: 25.0,
            orbit_angle: 0.0,
        },
        Inventory::new(500),
    ));

    // Starter cargo ships docked on the home surface.
    for i in 0..2 {
        commands.spawn((
            Ship::new_full(format!("Hauler-{}", i + 1), 100, 5.0, 100.0),
            ShipState::Docked {
                location: ShipLocation::Surface(home_body_name.to_string()),
            },
            Inventory::new(100),
        ));
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bodies::{BodyType, Environment};

    fn spawn_body(app: &mut App, name: &str, radius: f32, angle: f32) -> Entity {
        app.world_mut()
            .spawn(CelestialBody {
                name: name.into(),
                body_type: BodyType::Planet,
                orbit_radius: radius,
                orbit_angle: angle,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id()
    }

    fn spawn_orbiting_body(
        app: &mut App,
        name: &str,
        parent: Entity,
        radius: f32,
        angle: f32,
    ) -> Entity {
        app.world_mut()
            .spawn(CelestialBody {
                name: name.into(),
                body_type: BodyType::Planet,
                orbit_radius: radius,
                orbit_angle: angle,
                parent: Some(parent),
                environment: Environment::earth_like(),
            })
            .id()
    }

    fn register_star_system(app: &mut App, bodies: Vec<Entity>, star: Option<Entity>) {
        app.world_mut().insert_resource(StarSystem {
            name: "Test".into(),
            bodies,
            star,
        });
    }

    #[test]
    fn transfer_cargo_respects_source_and_destination_limits() {
        let mut src = Inventory::new(100);
        let mut dst = Inventory::new(10);
        src.try_add(ItemType::CopperOre, 50);

        // Destination capacity (10) is the bottleneck.
        let moved = transfer_cargo(&mut src, &mut dst, ItemType::CopperOre, 20);
        assert_eq!(moved, 10);
        assert_eq!(src.count(ItemType::CopperOre), 40);
        assert_eq!(dst.count(ItemType::CopperOre), 10);

        // Source now has plenty but dest is full → 0 moves.
        let moved = transfer_cargo(&mut src, &mut dst, ItemType::CopperOre, 5);
        assert_eq!(moved, 0);
    }

    #[test]
    fn transfer_cargo_limited_by_source_stock() {
        let mut src = Inventory::new(100);
        let mut dst = Inventory::new(100);
        src.try_add(ItemType::IronOre, 3);

        let moved = transfer_cargo(&mut src, &mut dst, ItemType::IronOre, 10);
        assert_eq!(moved, 3);
        assert_eq!(src.count(ItemType::IronOre), 0);
        assert_eq!(dst.count(ItemType::IronOre), 3);
    }

    #[test]
    fn transfer_cargo_zero_is_noop() {
        let mut src = Inventory::new(10);
        let mut dst = Inventory::new(10);
        src.try_add(ItemType::CopperOre, 5);
        let moved = transfer_cargo(&mut src, &mut dst, ItemType::CopperOre, 0);
        assert_eq!(moved, 0);
        assert_eq!(src.count(ItemType::CopperOre), 5);
        assert_eq!(dst.count(ItemType::CopperOre), 0);
    }

    #[test]
    fn ship_state_can_depart_helpers() {
        let docked = ShipState::Docked {
            location: ShipLocation::Surface("A".into()),
        };
        let idle = ShipState::Idle {
            near: ShipLocation::Surface("A".into()),
        };
        let traveling = ShipState::Traveling {
            from: ShipLocation::Surface("A".into()),
            to: ShipLocation::Surface("B".into()),
            progress: 0.3,
        };
        assert!(docked.can_depart());
        assert!(idle.can_depart());
        assert!(!traveling.can_depart());
        assert!(traveling.is_traveling());
    }

    #[test]
    fn travel_system_advances_progress_over_time() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);

        // Two bodies 200 units apart.
        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        let mut b_body = CelestialBody {
            name: "B".into(),
            body_type: BodyType::Planet,
            orbit_radius: 200.0,
            orbit_angle: 0.0,
            parent: None,
            environment: Environment::earth_like(),
        };
        b_body.orbit_radius = 200.0;
        let b = app.world_mut().spawn(b_body).id();
        register_star_system(&mut app, vec![a, b], Some(a));

        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("Test", 10, 10.0, 1000.0),
                ShipState::Traveling {
                    from: ShipLocation::Surface("A".into()),
                    to: ShipLocation::Surface("B".into()),
                    progress: 0.0,
                },
                Inventory::new(10),
            ))
            .id();

        // First tick: speed 10 / distance 200 = 0.05 progress per tick.
        app.update();
        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        match state {
            ShipState::Traveling { progress, .. } => {
                assert!(
                    (progress - 0.05).abs() < 1e-4,
                    "expected ~0.05 after 1 tick, got {progress}"
                );
            }
            other => panic!("expected still traveling, got {other:?}"),
        }

        // After enough ticks we should dock.
        for _ in 0..25 {
            app.update();
        }
        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        assert!(
            matches!(state, ShipState::Docked { .. }),
            "expected docked, got {state:?}"
        );
    }

    #[test]
    fn fuel_consumption_scales_with_distance() {
        let mut app_short = App::new();
        app_short.add_plugins(ShipsPlugin);
        let a = spawn_body(&mut app_short, "A", 0.0, 0.0);
        let b = app_short
            .world_mut()
            .spawn(CelestialBody {
                name: "B".into(),
                body_type: BodyType::Planet,
                orbit_radius: 50.0,
                orbit_angle: 0.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();
        register_star_system(&mut app_short, vec![a, b], Some(a));
        let ship_short = app_short
            .world_mut()
            .spawn((
                Ship::new_full("S1", 10, 5.0, 1000.0),
                ShipState::Traveling {
                    from: ShipLocation::Surface("A".into()),
                    to: ShipLocation::Surface("B".into()),
                    progress: 0.0,
                },
                Inventory::new(10),
            ))
            .id();

        let mut app_long = App::new();
        app_long.add_plugins(ShipsPlugin);
        let a2 = spawn_body(&mut app_long, "A", 0.0, 0.0);
        let b2 = app_long
            .world_mut()
            .spawn(CelestialBody {
                name: "B".into(),
                body_type: BodyType::Planet,
                orbit_radius: 500.0,
                orbit_angle: 0.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();
        register_star_system(&mut app_long, vec![a2, b2], Some(a2));
        let ship_long = app_long
            .world_mut()
            .spawn((
                Ship::new_full("S2", 10, 5.0, 1000.0),
                ShipState::Traveling {
                    from: ShipLocation::Surface("A".into()),
                    to: ShipLocation::Surface("B".into()),
                    progress: 0.0,
                },
                Inventory::new(10),
            ))
            .id();

        app_short.update();
        app_long.update();

        let short_fuel = app_short.world().get::<Ship>(ship_short).unwrap().fuel;
        let long_fuel = app_long.world().get::<Ship>(ship_long).unwrap().fuel;

        let short_burn = 1000.0 - short_fuel;
        let long_burn = 1000.0 - long_fuel;

        // Both ships travel `speed` units per tick when distance >= speed,
        // so per-tick fuel burn is identical. This test therefore asserts
        // the opposite: per-tick burn is distance-independent, but total
        // burn over the full trip scales with distance. Simulate both to
        // completion and compare total burn.
        let mut total_short = short_burn;
        loop {
            let st = app_short
                .world()
                .get::<ShipState>(ship_short)
                .unwrap()
                .clone();
            if matches!(st, ShipState::Docked { .. }) {
                break;
            }
            let before = app_short.world().get::<Ship>(ship_short).unwrap().fuel;
            app_short.update();
            let after = app_short.world().get::<Ship>(ship_short).unwrap().fuel;
            total_short += before - after;
        }

        let mut total_long = long_burn;
        loop {
            let st = app_long
                .world()
                .get::<ShipState>(ship_long)
                .unwrap()
                .clone();
            if matches!(st, ShipState::Docked { .. }) {
                break;
            }
            let before = app_long.world().get::<Ship>(ship_long).unwrap().fuel;
            app_long.update();
            let after = app_long.world().get::<Ship>(ship_long).unwrap().fuel;
            total_long += before - after;
        }

        assert!(
            total_long > total_short * 5.0,
            "longer trip should burn significantly more fuel: short={total_short}, long={total_long}"
        );
    }

    #[test]
    fn travel_order_transitions_docked_to_traveling() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);

        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        let b = app
            .world_mut()
            .spawn(CelestialBody {
                name: "B".into(),
                body_type: BodyType::Planet,
                orbit_radius: 100.0,
                orbit_angle: 0.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();
        register_star_system(&mut app, vec![a, b], Some(a));

        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("T", 10, 1.0, 1000.0),
                ShipState::Docked {
                    location: ShipLocation::Surface("A".into()),
                },
                Inventory::new(10),
            ))
            .id();

        app.world_mut().write_message(TravelOrder {
            ship,
            destination: ShipLocation::Surface("B".into()),
        });

        app.update();

        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        match state {
            ShipState::Traveling { from, to, progress } => {
                assert_eq!(from, ShipLocation::Surface("A".into()));
                assert_eq!(to, ShipLocation::Surface("B".into()));
                // One tick of motion already happened in the same schedule.
                assert!(progress > 0.0);
            }
            other => panic!("expected traveling, got {other:?}"),
        }
    }

    #[test]
    fn travel_order_ignored_when_already_traveling() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);

        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        let b = app
            .world_mut()
            .spawn(CelestialBody {
                name: "B".into(),
                body_type: BodyType::Planet,
                orbit_radius: 100.0,
                orbit_angle: 0.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();
        let c = app
            .world_mut()
            .spawn(CelestialBody {
                name: "C".into(),
                body_type: BodyType::Planet,
                orbit_radius: 200.0,
                orbit_angle: 1.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();
        register_star_system(&mut app, vec![a, b, c], Some(a));

        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("T", 10, 0.1, 1000.0),
                ShipState::Traveling {
                    from: ShipLocation::Surface("A".into()),
                    to: ShipLocation::Surface("B".into()),
                    progress: 0.1,
                },
                Inventory::new(10),
            ))
            .id();

        app.world_mut().write_message(TravelOrder {
            ship,
            destination: ShipLocation::Surface("C".into()),
        });

        app.update();

        // The destination must not have been replaced.
        match app.world().get::<ShipState>(ship).unwrap().clone() {
            ShipState::Traveling { to, .. } => {
                assert_eq!(to, ShipLocation::Surface("B".into()));
            }
            other => panic!("expected still traveling to B, got {other:?}"),
        }
    }

    #[test]
    fn out_of_fuel_strands_ship() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);

        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        let b = app
            .world_mut()
            .spawn(CelestialBody {
                name: "B".into(),
                body_type: BodyType::Planet,
                orbit_radius: 1000.0,
                orbit_angle: 0.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();
        register_star_system(&mut app, vec![a, b], Some(a));

        // 1 fuel isn't enough to cover even one tick at speed 5 on a 1000-unit
        // trip: 5 * 0.01 = 0.05 fuel per tick. Use very low fuel instead.
        let ship = app
            .world_mut()
            .spawn((
                Ship {
                    name: "Empty".into(),
                    cargo_capacity: 10,
                    speed: 5.0,
                    fuel: 0.01, // less than 0.05 required per tick
                    fuel_capacity: 100.0,
                },
                ShipState::Traveling {
                    from: ShipLocation::Surface("A".into()),
                    to: ShipLocation::Surface("B".into()),
                    progress: 0.0,
                },
                Inventory::new(10),
            ))
            .id();

        app.update();

        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        assert!(
            matches!(state, ShipState::Idle { .. }),
            "ship should be stranded, got {state:?}"
        );
        let fuel = app.world().get::<Ship>(ship).unwrap().fuel;
        assert_eq!(fuel, 0.0);
    }

    #[test]
    fn station_resolves_from_parent_body_position() {
        // Station orbits body B which itself orbits body A.
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);

        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        let b = spawn_orbiting_body(&mut app, "B", a, 300.0, 0.0);
        register_star_system(&mut app, vec![a, b], Some(a));

        app.world_mut().spawn(OrbitalStation {
            name: "B-Station".into(),
            parent_body: "B".into(),
            orbit_radius: 20.0,
            orbit_angle: 0.0,
        });

        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("S", 10, 5.0, 1000.0),
                ShipState::Traveling {
                    from: ShipLocation::Surface("A".into()),
                    to: ShipLocation::Station("B".into()),
                    progress: 0.0,
                },
                Inventory::new(10),
            ))
            .id();

        // Several ticks must make progress; destination is ~320 units away
        // (300 + 20). With speed 5 that's ~0.015625 per tick.
        app.update();
        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        match state {
            ShipState::Traveling { progress, .. } => {
                assert!(progress > 0.0 && progress < 0.1);
            }
            ShipState::Docked { location } => {
                // Tiny distances could dock instantly, but not here.
                panic!("unexpected dock on first tick at location {location:?}");
            }
            other => panic!("unexpected state {other:?}"),
        }
    }

    #[test]
    fn docked_ship_ignored_by_travel_system() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);
        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        register_star_system(&mut app, vec![a], Some(a));

        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("S", 10, 5.0, 100.0),
                ShipState::Docked {
                    location: ShipLocation::Surface("A".into()),
                },
                Inventory::new(10),
            ))
            .id();

        for _ in 0..5 {
            app.update();
        }
        let fuel = app.world().get::<Ship>(ship).unwrap().fuel;
        assert_eq!(fuel, 100.0, "docked ships shouldn't burn fuel");
    }

    #[test]
    fn station_orbital_motion_advances_angle() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);

        let station = app
            .world_mut()
            .spawn(OrbitalStation {
                name: "St".into(),
                parent_body: "A".into(),
                orbit_radius: 10.0,
                orbit_angle: 0.0,
            })
            .id();

        app.update();
        app.update();

        let after = app
            .world()
            .get::<OrbitalStation>(station)
            .unwrap()
            .orbit_angle;
        assert!(after > 0.0);
    }

    #[test]
    fn ship_serde_round_trip() {
        let ship = Ship::new_full("Hauler", 50, 3.0, 200.0);
        let bytes = bincode::serialize(&ship).unwrap();
        let restored: Ship = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.name, ship.name);
        assert_eq!(restored.cargo_capacity, ship.cargo_capacity);
        assert!((restored.speed - ship.speed).abs() < 1e-6);
        assert!((restored.fuel - ship.fuel).abs() < 1e-6);
        assert!((restored.fuel_capacity - ship.fuel_capacity).abs() < 1e-6);
    }

    #[test]
    fn ship_state_serde_round_trip() {
        let state = ShipState::Traveling {
            from: ShipLocation::Surface("A".into()),
            to: ShipLocation::Station("B".into()),
            progress: 0.42,
        };
        let bytes = bincode::serialize(&state).unwrap();
        let restored: ShipState = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored, state);
    }

    #[test]
    fn orbital_station_serde_round_trip() {
        let s = OrbitalStation {
            name: "Terra Orbital".into(),
            parent_body: "Terra".into(),
            orbit_radius: 25.0,
            orbit_angle: 1.5,
        };
        let bytes = bincode::serialize(&s).unwrap();
        let restored: OrbitalStation = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.name, s.name);
        assert_eq!(restored.parent_body, s.parent_body);
        assert!((restored.orbit_radius - s.orbit_radius).abs() < 1e-6);
        assert!((restored.orbit_angle - s.orbit_angle).abs() < 1e-6);
    }
}
