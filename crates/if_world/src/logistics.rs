// logistics.rs: Phase 4 interplanetary logistics.
//
// Contains:
//   * Warp travel — inter-system jumps driven by `WarpOrder` messages. Ships
//     pay fuel proportional to the galactic-space distance between systems
//     and spend a few ticks in the `Warping` state before arriving.
//   * Autonomous freight routes — a `FreightRoute` component defines a ring
//     of waypoints a ship visits repeatedly, loading/unloading cargo at each
//     stop. The `freight_route_system` dispatches travel/warp orders so an
//     idle ship on a route makes progress each tick without player input.
//   * Resource-depletion stats — a small telemetry resource that counts nodes
//     that have hit 0 remaining, so the client can surface "your X mine is
//     running out" notifications.
//
// All systems are wired up by `LogisticsPlugin`, which `WorldPlugin` installs
// on its own. Freight routes and warp orders live in this crate (not ships.rs)
// so the core ship state machine stays small and unit-testable in isolation.

use bevy::prelude::*;
use if_common::item::ItemType;
use if_factory::inventory::Inventory;
use if_factory::mining::ResourceNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::galaxy::Galaxy;
use crate::ships::{Ship, ShipLocation, ShipState, TravelOrder};

// -----------------------------------------------------------------------------
// Warp constants & messages
// -----------------------------------------------------------------------------

/// Fuel consumed per light-year of warp travel. Chosen so a full 100-unit tank
/// covers several inter-system hops assuming 50-200 LY separations.
pub const WARP_FUEL_PER_LY: f32 = 0.1;

/// Number of ticks a warp jump takes regardless of distance. Real rocketry
/// would scale this with distance but gameplay-wise a flat "warp animation
/// duration" keeps the UI simple.
pub const WARP_TICKS: u32 = 6;

/// Per-tick progress increment during a warp. Derived so the warp completes
/// in `WARP_TICKS` ticks exactly.
pub const WARP_PROGRESS_PER_TICK: f32 = 1.0 / WARP_TICKS as f32;

/// A request to jump a ship between star systems.
///
/// Same semantics as `TravelOrder` but for inter-system warps. Emitted by the
/// UI (or by the freight route system) and consumed by `process_warp_orders`.
#[derive(bevy::ecs::message::Message, Clone, Debug)]
pub struct WarpOrder {
    pub ship: Entity,
    pub target_system: usize,
}

/// Emitted when a resource node transitions to `remaining == 0`. Useful for
/// UI notifications ("Iron mine #3 depleted"). Internal book-keeping is done
/// by `DepletionStats`.
#[derive(bevy::ecs::message::Message, Clone, Debug)]
pub struct ResourceDepletedEvent {
    pub node: Entity,
    pub resource: ItemType,
}

// -----------------------------------------------------------------------------
// Warp travel
// -----------------------------------------------------------------------------

/// Consume `WarpOrder` messages and transition eligible ships into the
/// `Warping` state. Checks fuel up-front: a ship that can't afford the jump
/// keeps its current state and the order is logged and dropped.
pub fn process_warp_orders(
    mut orders: MessageReader<WarpOrder>,
    mut ships: Query<(&mut Ship, &mut ShipState)>,
    galaxy: Option<Res<Galaxy>>,
) {
    let Some(galaxy) = galaxy else {
        // No galaxy resource means warp is not enabled in this world (e.g., a
        // narrow unit test). Drain the orders to avoid spamming on replay.
        for _ in orders.read() {}
        return;
    };

    for order in orders.read() {
        let Ok((mut ship, mut state)) = ships.get_mut(order.ship) else {
            warn!(
                "warp order for unknown ship entity {:?} — dropping",
                order.ship
            );
            continue;
        };

        if !state.can_depart() {
            warn!(
                "ship '{}' cannot warp right now (state: {:?}) — dropping order",
                ship.name, *state
            );
            continue;
        }

        if ship.system_index == order.target_system {
            // Already there — silently ignore.
            continue;
        }

        let Some(distance) = galaxy.distance(ship.system_index, order.target_system) else {
            warn!(
                "warp order targets unknown system index {} — dropping",
                order.target_system
            );
            continue;
        };

        let fuel_cost = distance * WARP_FUEL_PER_LY;
        if ship.fuel < fuel_cost {
            warn!(
                "ship '{}' needs {:.2} fuel to warp but only has {:.2}",
                ship.name, fuel_cost, ship.fuel
            );
            continue;
        }

        ship.fuel -= fuel_cost;
        *state = ShipState::Warping {
            from_system: ship.system_index,
            to_system: order.target_system,
            progress: 0.0,
        };
    }
}

/// Advance warping ships one tick at a time. When progress reaches 1.0 the
/// ship arrives at the target system: its `system_index` is updated and it
/// becomes `Idle`. Since we don't know the target system's bodies from this
/// crate (the target may not even be generated yet), the ship arrives in a
/// generic "Idle at Sol" fallback location — the UI/game flow then resolves
/// it on visit.
pub fn warp_travel_system(
    mut ships: Query<(&mut Ship, &mut ShipState)>,
    galaxy: Option<Res<Galaxy>>,
) {
    for (mut ship, mut state) in &mut ships {
        let (from_system, to_system, progress) = match &*state {
            ShipState::Warping {
                from_system,
                to_system,
                progress,
            } => (*from_system, *to_system, *progress),
            _ => continue,
        };

        let new_progress = progress + WARP_PROGRESS_PER_TICK;
        if new_progress >= 1.0 {
            ship.system_index = to_system;
            // Drop the ship into idle near the destination system's home body.
            // We use the system name (if known) as the body anchor — callers
            // can later retarget via a TravelOrder. For unknown systems use a
            // synthetic name; this won't resolve to a position until bodies
            // spawn, but the ship is Idle so no travel math runs on it.
            let target_name = galaxy
                .as_ref()
                .and_then(|g| g.systems.get(to_system))
                .map(|s| s.name.clone())
                .unwrap_or_else(|| format!("System-{to_system}"));
            *state = ShipState::Idle {
                near: ShipLocation::Surface(target_name),
            };
            info!(
                "ship '{}' completed warp from system {} to {}",
                ship.name, from_system, to_system
            );
        } else {
            *state = ShipState::Warping {
                from_system,
                to_system,
                progress: new_progress,
            };
        }
    }
}

// -----------------------------------------------------------------------------
// Freight routes
// -----------------------------------------------------------------------------

/// What a ship does when it arrives at a waypoint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WaypointAction {
    /// Load up to `quantity` of `item` into ship cargo.
    Load { item: ItemType, quantity: u32 },
    /// Unload `item` from ship cargo (up to `quantity`; 0 = unload all).
    Unload { item: ItemType, quantity: u32 },
    /// Just visit (navigation waypoint).
    Visit,
}

/// One step on a freight route.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Waypoint {
    pub location: ShipLocation,
    pub system_index: usize,
    /// Actions to perform at this waypoint.
    pub action: WaypointAction,
}

impl Waypoint {
    pub fn new(location: ShipLocation, system_index: usize, action: WaypointAction) -> Self {
        Self {
            location,
            system_index,
            action,
        }
    }
}

/// An autonomous route: ring of waypoints a ship visits in order forever.
///
/// `ship` is stored as an `Entity`. Entities are not serializable, so the
/// field is skipped by serde and reset to `Entity::PLACEHOLDER` on load — the
/// client is expected to rewire routes by name (or whatever identity scheme
/// it uses) after deserialization.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct FreightRoute {
    /// Ship assigned to this route.
    #[serde(skip, default = "placeholder_entity")]
    pub ship: Entity,
    /// Ordered list of waypoints to visit repeatedly.
    pub waypoints: Vec<Waypoint>,
    /// Current waypoint index.
    pub current: usize,
    /// Is the route active?
    pub active: bool,
}

fn placeholder_entity() -> Entity {
    Entity::PLACEHOLDER
}

impl FreightRoute {
    pub fn new(ship: Entity, waypoints: Vec<Waypoint>) -> Self {
        Self {
            ship,
            waypoints,
            current: 0,
            active: true,
        }
    }

    /// Waypoint the ship is currently heading for (or is at). `None` if the
    /// route has no waypoints.
    pub fn current_waypoint(&self) -> Option<&Waypoint> {
        self.waypoints.get(self.current)
    }

    /// Advance to the next waypoint (wrapping around).
    pub fn advance(&mut self) {
        if self.waypoints.is_empty() {
            return;
        }
        self.current = (self.current + 1) % self.waypoints.len();
    }
}

/// Dispatch TravelOrder/WarpOrder and perform cargo actions for every active
/// freight route.
///
/// Per tick, for each route:
///   1. If the ship is not at the current waypoint's system, emit a
///      `WarpOrder` (only when the ship is `Docked` or `Idle`).
///   2. Otherwise, if the ship is not docked at the waypoint location, emit
///      a `TravelOrder`.
///   3. Otherwise (ship is docked at the waypoint), perform the waypoint's
///      action and advance to the next waypoint.
///
/// Cargo actions use the ship's `Inventory` and a station inventory found at
/// the waypoint location when available. For `Surface` waypoints without a
/// dedicated inventory in the world, `Load` is a no-op (nothing to pull from)
/// and `Unload` drops items into the void — keeps the system simple for the
/// starter phase.
/// A Bevy `Query` over orbital-station inventories that excludes ship and
/// route entities, so it can be borrowed safely alongside the ship query in
/// `freight_route_system`. Extracted as a type alias to keep system
/// signatures readable (and to satisfy `clippy::type_complexity`).
type StationInventoryQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static crate::ships::OrbitalStation,
        &'static mut Inventory,
    ),
    (Without<FreightRoute>, Without<Ship>),
>;

pub fn freight_route_system(
    mut routes: Query<&mut FreightRoute>,
    mut ships: Query<(&Ship, &ShipState, &mut Inventory), Without<FreightRoute>>,
    mut station_inventories: StationInventoryQuery,
    mut travel_orders: MessageWriter<TravelOrder>,
    mut warp_orders: MessageWriter<WarpOrder>,
) {
    for mut route in &mut routes {
        if !route.active || route.waypoints.is_empty() {
            continue;
        }

        // Snapshot the current waypoint index — we'll borrow &mut via `advance`
        // later, and we need stable indexing meanwhile.
        let current_idx = route.current;
        let waypoint = route.waypoints[current_idx].clone();

        let ship_entity = route.ship;
        let Ok((ship, ship_state, mut ship_inv)) = ships.get_mut(ship_entity) else {
            // Ship entity missing (despawned or not yet wired after load).
            // Skip quietly — client layer is responsible for repairing routes.
            continue;
        };

        // Step 1: wrong system? Emit warp if docked/idle.
        if ship.system_index != waypoint.system_index {
            if ship_state.can_depart() {
                warp_orders.write(WarpOrder {
                    ship: ship_entity,
                    target_system: waypoint.system_index,
                });
            }
            continue;
        }

        // Step 2: right system but wrong location? Emit travel order.
        let at_waypoint = matches!(
            ship_state,
            ShipState::Docked { location } if *location == waypoint.location
        );
        if !at_waypoint {
            if ship_state.can_depart() {
                travel_orders.write(TravelOrder {
                    ship: ship_entity,
                    destination: waypoint.location.clone(),
                });
            }
            continue;
        }

        // Step 3: at the waypoint — perform the action.
        perform_waypoint_action(&waypoint, &mut ship_inv, &mut station_inventories);

        route.advance();
    }
}

/// Execute the cargo action for a waypoint. Extracted so the system body
/// stays readable.
fn perform_waypoint_action(
    waypoint: &Waypoint,
    ship_inv: &mut Inventory,
    station_inventories: &mut StationInventoryQuery,
) {
    match &waypoint.action {
        WaypointAction::Visit => {}
        WaypointAction::Load { item, quantity } => {
            if let ShipLocation::Station(body_name) = &waypoint.location
                && let Some(mut station_inv) = station_inventories
                    .iter_mut()
                    .find(|(s, _)| s.parent_body == *body_name)
                    .map(|(_, inv)| inv)
            {
                crate::ships::transfer_cargo(&mut station_inv, ship_inv, *item, *quantity);
            }
            // Surface load without a station inventory is a no-op.
        }
        WaypointAction::Unload { item, quantity } => {
            let target_qty = if *quantity == 0 {
                ship_inv.count(*item)
            } else {
                *quantity
            };
            if let ShipLocation::Station(body_name) = &waypoint.location
                && let Some(mut station_inv) = station_inventories
                    .iter_mut()
                    .find(|(s, _)| s.parent_body == *body_name)
                    .map(|(_, inv)| inv)
            {
                crate::ships::transfer_cargo(ship_inv, &mut station_inv, *item, target_qty);
                return;
            }
            // Surface unload without a station inventory: items drop into the
            // void (remove from ship). Keeps the system simple until we add a
            // proper planet-side storage model.
            ship_inv.try_remove(*item, target_qty);
        }
    }
}

// -----------------------------------------------------------------------------
// Depletion stats
// -----------------------------------------------------------------------------

/// Aggregate counters for depleted resource nodes. Used for end-game pressure
/// UI ("your colony has drained N nodes this session") and for tutorial
/// triggers when the player exhausts their first deposit.
#[derive(Resource, Default, Clone, Debug)]
pub struct DepletionStats {
    pub total_depleted: u32,
    pub by_resource: HashMap<ItemType, u32>,
}

impl DepletionStats {
    pub fn record(&mut self, item: ItemType) {
        self.total_depleted += 1;
        *self.by_resource.entry(item).or_insert(0) += 1;
    }

    pub fn count_for(&self, item: ItemType) -> u32 {
        self.by_resource.get(&item).copied().unwrap_or(0)
    }
}

/// Tracks which resource-node entities have already been counted as depleted,
/// so we increment stats exactly once per node per session.
#[derive(Resource, Default)]
pub struct DepletionTracker {
    seen: std::collections::HashSet<Entity>,
}

/// Watch every `ResourceNode` and update `DepletionStats` when a node first
/// transitions to `remaining == 0`. Also fires a `ResourceDepletedEvent` so
/// UI layers can react immediately.
pub fn depletion_tracking_system(
    nodes: Query<(Entity, &ResourceNode)>,
    mut stats: ResMut<DepletionStats>,
    mut tracker: ResMut<DepletionTracker>,
    mut events: MessageWriter<ResourceDepletedEvent>,
) {
    for (entity, node) in &nodes {
        if !node.is_depleted() {
            continue;
        }
        if tracker.seen.insert(entity) {
            stats.record(node.resource);
            events.write(ResourceDepletedEvent {
                node: entity,
                resource: node.resource,
            });
        }
    }
}

// -----------------------------------------------------------------------------
// Refuel helper
// -----------------------------------------------------------------------------

/// Top up a ship's fuel at a station, pulling "fuel" items (currently modelled
/// by `ItemType::CopperIngot` as a placeholder) from the station inventory.
///
/// Each ingot yields `FUEL_PER_INGOT` fuel. Returns the fuel amount added.
/// A future revision will introduce a real `Fuel` ItemType; the helper's
/// signature should be stable across that change.
pub const FUEL_PER_INGOT: f32 = 25.0;

pub fn refuel_at_station(ship: &mut Ship, station_inventory: &mut Inventory) -> f32 {
    let headroom = (ship.fuel_capacity - ship.fuel).max(0.0);
    if headroom <= 0.0 {
        return 0.0;
    }
    let ingots_needed = (headroom / FUEL_PER_INGOT).ceil() as u32;
    let ingots_available = station_inventory.count(ItemType::CopperIngot);
    let ingots_consumed = ingots_needed.min(ingots_available);
    if ingots_consumed == 0 {
        return 0.0;
    }
    let removed = station_inventory.try_remove(ItemType::CopperIngot, ingots_consumed);
    let fuel_yield = removed as f32 * FUEL_PER_INGOT;
    // Cap to headroom — partial ingot "burn" just leaves residual energy on
    // the ground for the next refill; acceptable for a starter placeholder.
    ship.refuel(fuel_yield)
}

// -----------------------------------------------------------------------------
// Startup & plugin
// -----------------------------------------------------------------------------

/// Startup system: initialize the `Galaxy` resource with a deterministic seed
/// unless one has already been provided (e.g., from a save file, or from a
/// test harness that pre-populated a custom galaxy).
///
/// Using a fixed seed keeps the galaxy identical between runs — essential for
/// debugging. Tests can call `World::insert_resource(Galaxy{..})` before the
/// first `update()` and this system will detect the existing resource and
/// leave it alone.
pub fn init_galaxy_system(mut commands: Commands, existing: Option<Res<Galaxy>>) {
    const GALAXY_SEED: u32 = 2026;
    // Heuristic: a pre-populated galaxy will have at least one system. The
    // default-constructed `Galaxy` (from `init_resource::<Galaxy>`) has zero
    // systems; we overwrite that. This lets `init_resource` keep acting as a
    // fallback without clobbering explicit test fixtures.
    if existing.as_ref().is_some_and(|g| !g.systems.is_empty()) {
        return;
    }
    commands.insert_resource(crate::galaxy::generate_galaxy(GALAXY_SEED));
}

/// Registers every Phase 4 logistics system and resource.
///
/// Depends on `ShipsPlugin` for the `TravelOrder` message type; installs it
/// automatically when missing so callers can drop in `LogisticsPlugin` on its
/// own.
pub struct LogisticsPlugin;

impl Plugin for LogisticsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::ships::ShipsPlugin>() {
            app.add_plugins(crate::ships::ShipsPlugin);
        }
        app.init_resource::<Galaxy>()
            .init_resource::<DepletionStats>()
            .init_resource::<DepletionTracker>()
            .add_message::<WarpOrder>()
            .add_message::<ResourceDepletedEvent>()
            .add_systems(Startup, init_galaxy_system)
            .add_systems(
                Update,
                (
                    // Freight routes dispatch travel/warp orders first, so
                    // the order processors below pick them up in the same
                    // tick (avoiding a one-tick lag between "idle at
                    // waypoint" and "actually departing").
                    freight_route_system,
                    process_warp_orders,
                    warp_travel_system,
                    depletion_tracking_system,
                )
                    .chain(),
            );
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bodies::{BodyType, CelestialBody, Environment, StarSystem};
    use crate::galaxy::SystemDescriptor;
    use crate::ships::{OrbitalStation, ShipsPlugin};

    fn build_world_with_galaxy() -> (App, Vec<usize>) {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);
        app.add_plugins(LogisticsPlugin);
        // Replace the auto-generated galaxy with a tiny known one so distance
        // math is predictable in tests.
        let mut galaxy = Galaxy::default();
        galaxy
            .systems
            .push(SystemDescriptor::new("A", Vec2::new(0.0, 0.0), 1));
        galaxy
            .systems
            .push(SystemDescriptor::new("B", Vec2::new(100.0, 0.0), 2));
        galaxy
            .systems
            .push(SystemDescriptor::new("C", Vec2::new(0.0, 200.0), 3));
        galaxy.add_warp_lane(0, 1);
        galaxy.add_warp_lane(1, 2);
        app.insert_resource(galaxy);
        (app, vec![0, 1, 2])
    }

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

    #[test]
    fn refuel_respects_capacity() {
        let mut ship = Ship::new_full("S", 10, 1.0, 100.0);
        ship.fuel = 30.0;
        // Top up: only 70 more should fit.
        let added = ship.refuel(200.0);
        assert_eq!(added, 70.0);
        assert_eq!(ship.fuel, 100.0);
        // Already full — no-op.
        let added = ship.refuel(10.0);
        assert_eq!(added, 0.0);
        // Negative amount — no-op.
        let added = ship.refuel(-5.0);
        assert_eq!(added, 0.0);
    }

    #[test]
    fn refuel_at_station_consumes_ingots() {
        let mut ship = Ship::new_full("S", 10, 1.0, 100.0);
        ship.fuel = 0.0;
        let mut station = Inventory::new(50);
        station.try_add(ItemType::CopperIngot, 10);
        let added = refuel_at_station(&mut ship, &mut station);
        // Needs 100 fuel / 25 per ingot = 4 ingots.
        assert!(added > 0.0);
        assert_eq!(station.count(ItemType::CopperIngot), 10 - 4);
        assert_eq!(ship.fuel, 100.0);
    }

    #[test]
    fn warp_order_consumes_fuel_proportional_to_distance() {
        let (mut app, _) = build_world_with_galaxy();
        // Home at system 0 with enough fuel for a 100-LY hop (10 fuel).
        let ship = app
            .world_mut()
            .spawn((
                Ship {
                    name: "Jumper".into(),
                    cargo_capacity: 10,
                    speed: 1.0,
                    fuel: 50.0,
                    fuel_capacity: 100.0,
                    system_index: 0,
                },
                ShipState::Docked {
                    location: ShipLocation::Surface("Sol".into()),
                },
                Inventory::new(10),
            ))
            .id();

        app.world_mut().write_message(WarpOrder {
            ship,
            target_system: 1,
        });

        // First tick processes the order (and also advances warp by one step).
        app.update();

        let fuel = app.world().get::<Ship>(ship).unwrap().fuel;
        // Cost = 100 LY * 0.1 fuel/LY = 10.
        assert!(
            (fuel - 40.0).abs() < 1e-3,
            "expected 40 fuel remaining, got {fuel}"
        );
    }

    #[test]
    fn warp_fails_without_enough_fuel() {
        let (mut app, _) = build_world_with_galaxy();
        let ship = app
            .world_mut()
            .spawn((
                Ship {
                    name: "Dry".into(),
                    cargo_capacity: 10,
                    speed: 1.0,
                    fuel: 1.0, // not nearly enough for a 200-LY jump
                    fuel_capacity: 100.0,
                    system_index: 0,
                },
                ShipState::Docked {
                    location: ShipLocation::Surface("Sol".into()),
                },
                Inventory::new(10),
            ))
            .id();

        // target_system=2 is at distance sqrt(100^2 + 200^2) ~ 223 from sys 0.
        app.world_mut().write_message(WarpOrder {
            ship,
            target_system: 2,
        });
        app.update();

        // Ship should still be docked and retain (most of) its fuel.
        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        assert!(
            matches!(state, ShipState::Docked { .. }),
            "ship should still be docked: {state:?}"
        );
        let fuel = app.world().get::<Ship>(ship).unwrap().fuel;
        assert!((fuel - 1.0).abs() < 1e-6);
    }

    #[test]
    fn warp_completes_after_expected_ticks() {
        let (mut app, _) = build_world_with_galaxy();
        let ship = app
            .world_mut()
            .spawn((
                Ship {
                    name: "Jumper".into(),
                    cargo_capacity: 10,
                    speed: 1.0,
                    fuel: 100.0,
                    fuel_capacity: 100.0,
                    system_index: 0,
                },
                ShipState::Docked {
                    location: ShipLocation::Surface("Sol".into()),
                },
                Inventory::new(10),
            ))
            .id();
        app.world_mut().write_message(WarpOrder {
            ship,
            target_system: 1,
        });

        // WARP_TICKS + 1 extra (the first tick both starts the warp and
        // advances progress once).
        for _ in 0..(WARP_TICKS + 2) as usize {
            app.update();
        }

        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        assert!(
            matches!(state, ShipState::Idle { .. }),
            "expected Idle after warp, got {state:?}"
        );
        let system_idx = app.world().get::<Ship>(ship).unwrap().system_index;
        assert_eq!(system_idx, 1);
    }

    #[test]
    fn freight_route_loads_at_station_and_advances() {
        // Setup: one system with a body and a station orbiting it. Ship is
        // docked at the station. Waypoint 0 = load 10 CopperOre from station.
        // Waypoint 1 = visit surface. After a handful of ticks the ship
        // should have loaded and be traveling toward waypoint 1.
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);
        app.add_plugins(LogisticsPlugin);

        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        app.world_mut().insert_resource(StarSystem {
            name: "Solo".into(),
            bodies: vec![a],
            star: Some(a),
        });

        let mut station_inv = Inventory::new(200);
        station_inv.try_add(ItemType::CopperOre, 50);
        app.world_mut().spawn((
            OrbitalStation {
                name: "A-Orbital".into(),
                parent_body: "A".into(),
                orbit_radius: 20.0,
                orbit_angle: 0.0,
            },
            station_inv,
        ));

        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("R", 100, 5.0, 100.0),
                ShipState::Docked {
                    location: ShipLocation::Station("A".into()),
                },
                Inventory::new(100),
            ))
            .id();

        let waypoints = vec![
            Waypoint::new(
                ShipLocation::Station("A".into()),
                0,
                WaypointAction::Load {
                    item: ItemType::CopperOre,
                    quantity: 10,
                },
            ),
            Waypoint::new(ShipLocation::Surface("A".into()), 0, WaypointAction::Visit),
        ];
        app.world_mut().spawn(FreightRoute::new(ship, waypoints));

        // Single tick: ship is at waypoint 0 → load + advance.
        app.update();

        let cargo = app
            .world()
            .get::<Inventory>(ship)
            .unwrap()
            .count(ItemType::CopperOre);
        assert_eq!(cargo, 10, "ship should have loaded 10 CopperOre");

        // Route should now be pointing at waypoint 1 (Visit surface).
        let mut q = app.world_mut().query::<&FreightRoute>();
        let route = q.single(app.world()).unwrap();
        assert_eq!(route.current, 1);
    }

    #[test]
    fn freight_route_unloads_at_station() {
        let mut app = App::new();
        app.add_plugins(ShipsPlugin);
        app.add_plugins(LogisticsPlugin);

        let a = spawn_body(&mut app, "A", 0.0, 0.0);
        app.world_mut().insert_resource(StarSystem {
            name: "Solo".into(),
            bodies: vec![a],
            star: Some(a),
        });

        let station_inv = Inventory::new(500);
        app.world_mut().spawn((
            OrbitalStation {
                name: "A-Orbital".into(),
                parent_body: "A".into(),
                orbit_radius: 20.0,
                orbit_angle: 0.0,
            },
            station_inv,
        ));

        let mut ship_inv = Inventory::new(100);
        ship_inv.try_add(ItemType::IronOre, 30);
        let ship = app
            .world_mut()
            .spawn((
                Ship::new_full("R", 100, 5.0, 100.0),
                ShipState::Docked {
                    location: ShipLocation::Station("A".into()),
                },
                ship_inv,
            ))
            .id();

        app.world_mut().spawn(FreightRoute::new(
            ship,
            vec![Waypoint::new(
                ShipLocation::Station("A".into()),
                0,
                WaypointAction::Unload {
                    item: ItemType::IronOre,
                    quantity: 0, // 0 = unload all
                },
            )],
        ));

        app.update();

        let ship_left = app
            .world()
            .get::<Inventory>(ship)
            .unwrap()
            .count(ItemType::IronOre);
        assert_eq!(ship_left, 0);
    }

    #[test]
    fn freight_route_dispatches_warp_when_wrong_system() {
        let (mut app, _) = build_world_with_galaxy();

        let a = spawn_body(&mut app, "Sol", 0.0, 0.0);
        app.world_mut().insert_resource(StarSystem {
            name: "Solo".into(),
            bodies: vec![a],
            star: Some(a),
        });

        let ship = app
            .world_mut()
            .spawn((
                Ship {
                    name: "R".into(),
                    cargo_capacity: 10,
                    speed: 1.0,
                    fuel: 100.0,
                    fuel_capacity: 100.0,
                    system_index: 0,
                },
                ShipState::Docked {
                    location: ShipLocation::Surface("Sol".into()),
                },
                Inventory::new(10),
            ))
            .id();

        // Waypoint is in system 1 (target system). Ship is in system 0 →
        // route must emit a warp order.
        app.world_mut().spawn(FreightRoute::new(
            ship,
            vec![Waypoint::new(
                ShipLocation::Surface("B".into()),
                1,
                WaypointAction::Visit,
            )],
        ));

        app.update();

        // After one tick the ship should have transitioned into Warping.
        let state = app.world().get::<ShipState>(ship).unwrap().clone();
        assert!(
            matches!(state, ShipState::Warping { .. }),
            "expected Warping, got {state:?}"
        );
    }

    #[test]
    fn depletion_stats_increment_once_per_node() {
        let mut app = App::new();
        // LogisticsPlugin auto-installs ShipsPlugin (which registers the
        // TravelOrder message the freight route system depends on).
        app.add_plugins(LogisticsPlugin);

        // Two depleted nodes, one active.
        app.world_mut().spawn(ResourceNode {
            resource: ItemType::CopperOre,
            yield_per_tick: 0.1,
            remaining: 0,
        });
        app.world_mut().spawn(ResourceNode {
            resource: ItemType::IronOre,
            yield_per_tick: 0.1,
            remaining: 0,
        });
        let active = app
            .world_mut()
            .spawn(ResourceNode {
                resource: ItemType::CopperOre,
                yield_per_tick: 0.1,
                remaining: 100,
            })
            .id();

        // Several ticks — stats should only tick up once per depleted node.
        for _ in 0..5 {
            app.update();
        }

        let stats = app.world().resource::<DepletionStats>();
        assert_eq!(stats.total_depleted, 2);
        assert_eq!(stats.count_for(ItemType::CopperOre), 1);
        assert_eq!(stats.count_for(ItemType::IronOre), 1);

        // Deplete the third node and re-run → total must become 3, still
        // one increment for this node.
        app.world_mut()
            .get_mut::<ResourceNode>(active)
            .unwrap()
            .remaining = 0;
        for _ in 0..5 {
            app.update();
        }
        let stats = app.world().resource::<DepletionStats>();
        assert_eq!(stats.total_depleted, 3);
        assert_eq!(stats.count_for(ItemType::CopperOre), 2);
    }

    #[test]
    fn freight_route_serde_round_trip() {
        let route = FreightRoute::new(
            Entity::PLACEHOLDER,
            vec![
                Waypoint::new(
                    ShipLocation::Station("Sol".into()),
                    0,
                    WaypointAction::Load {
                        item: ItemType::CopperOre,
                        quantity: 50,
                    },
                ),
                Waypoint::new(
                    ShipLocation::Surface("Alpha".into()),
                    1,
                    WaypointAction::Unload {
                        item: ItemType::IronOre,
                        quantity: 0,
                    },
                ),
            ],
        );
        let bytes = bincode::serialize(&route).unwrap();
        let restored: FreightRoute = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.waypoints, route.waypoints);
        assert_eq!(restored.current, route.current);
        assert_eq!(restored.active, route.active);
        // Ship is skipped and falls back to PLACEHOLDER.
        assert_eq!(restored.ship, Entity::PLACEHOLDER);
    }
}
