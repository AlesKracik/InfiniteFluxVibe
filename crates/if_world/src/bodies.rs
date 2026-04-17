// bodies.rs: Celestial bodies, star systems, planetary surfaces.
//
// Phase 3 adds multiple planets to the game. Each planet is its own ECS entity
// with a `CelestialBody` component describing its orbit and environment. Planets
// (and moons) that the player can land on also get a `PlanetSurface` component
// holding their per-body `Grid`.
//
// The `Grid` resource (see grid.rs) still exists — it mirrors whichever body
// the player is currently viewing (tracked by `CurrentBody`). This keeps all
// the existing rendering and factory code untouched: it still reads `Res<Grid>`
// and writes to a single "active" surface.

use bevy::prelude::*;
use if_common::TileType;
use serde::{Deserialize, Serialize};

use crate::grid::Grid;

/// Classification of a celestial body.
///
/// Planets orbit the star. Moons orbit a planet. Asteroids are small rocky
/// bodies (typically in belts) that may or may not have surfaces to land on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BodyType {
    Planet,
    Moon,
    Asteroid,
}

/// Environmental parameters for a celestial body.
///
/// These modifiers are stored here so that future gameplay systems (e.g.,
/// building power consumption on cold worlds, atmospheric mining requirements,
/// low-gravity belt speeds) have a single source of truth.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Environment {
    /// Relative gravity. 1.0 = Earth-like.
    pub gravity: f32,
    /// Atmospheric density. 0.0 = vacuum, 1.0 = Earth-like.
    pub atmosphere: f32,
    /// Temperature modifier. -1.0 = very cold, 0.0 = temperate, 1.0 = very hot.
    pub temperature: f32,
}

impl Environment {
    /// An Earth-like reference environment.
    pub fn earth_like() -> Self {
        Self {
            gravity: 1.0,
            atmosphere: 1.0,
            temperature: 0.0,
        }
    }

    /// A vacuum environment suitable for moons and asteroids.
    pub fn vacuum() -> Self {
        Self {
            gravity: 0.2,
            atmosphere: 0.0,
            temperature: -0.5,
        }
    }
}

/// Marker component placed on every celestial body entity.
///
/// Parent is stored only at runtime — it is skipped during serialization since
/// Bevy `Entity` IDs are not stable across sessions. If a save format wants to
/// preserve parent relationships it should resolve them by `name` on load.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct CelestialBody {
    pub name: String,
    pub body_type: BodyType,
    /// Distance from the parent body (star for planets, planet for moons).
    /// Units are abstract "system map" units; rendering scales them as needed.
    pub orbit_radius: f32,
    /// Current angular position in radians. Updated by `orbital_motion_system`.
    pub orbit_angle: f32,
    /// Optional parent body entity (None for the star itself).
    ///
    /// `Entity` is not serializable, so this field is skipped during
    /// serialization and reset to `None` on load. Higher layers can rebuild
    /// parent links by name if needed.
    #[serde(skip)]
    pub parent: Option<Entity>,
    /// Environmental modifiers for this body.
    pub environment: Environment,
}

impl CelestialBody {
    /// Create a new star (no parent, no orbit).
    pub fn star(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            body_type: BodyType::Planet, // The star itself isn't really a planet;
            // we mark it with `parent == None` + star entry in StarSystem.
            // Callers should not use `body_type` to detect stars —
            // use `StarSystem::star` instead.
            orbit_radius: 0.0,
            orbit_angle: 0.0,
            parent: None,
            environment: Environment {
                gravity: 0.0,
                atmosphere: 0.0,
                temperature: 1.0,
            },
        }
    }

    /// Compute the world-space position of this body given its parent's
    /// position. For stars/root bodies, pass `Vec2::ZERO`.
    pub fn position(&self, parent_pos: Vec2) -> Vec2 {
        Vec2::new(
            parent_pos.x + self.orbit_radius * self.orbit_angle.cos(),
            parent_pos.y + self.orbit_radius * self.orbit_angle.sin(),
        )
    }
}

/// How resources are distributed on a planetary surface.
///
/// This drives `generate_planet_surface` — different distributions produce
/// noticeably different ore ratios, giving each world a unique "flavor".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceDistribution {
    /// Roughly even split between copper, iron, and rock.
    Balanced,
    /// Copper-dominant world (good for early electronics).
    CopperRich,
    /// Iron-dominant world (good for structural materials).
    IronRich,
    /// Sparse resources. Mostly rock and empty tiles.
    Barren,
    /// Resource-rich world with many deposits and little rock.
    Lush,
}

impl ResourceDistribution {
    /// Return the (copper, iron, rock, empty) probability weights for this
    /// distribution. Used by `generate_planet_surface`. The weights do not
    /// need to sum to 1.0 — the generator normalizes.
    pub fn weights(&self) -> DistributionWeights {
        match self {
            ResourceDistribution::Balanced => DistributionWeights {
                copper: 0.15,
                iron: 0.15,
                rock: 0.15,
                empty: 0.55,
            },
            ResourceDistribution::CopperRich => DistributionWeights {
                copper: 0.35,
                iron: 0.10,
                rock: 0.10,
                empty: 0.45,
            },
            ResourceDistribution::IronRich => DistributionWeights {
                copper: 0.10,
                iron: 0.35,
                rock: 0.10,
                empty: 0.45,
            },
            ResourceDistribution::Barren => DistributionWeights {
                copper: 0.02,
                iron: 0.02,
                rock: 0.35,
                empty: 0.61,
            },
            ResourceDistribution::Lush => DistributionWeights {
                copper: 0.25,
                iron: 0.25,
                rock: 0.05,
                empty: 0.45,
            },
        }
    }
}

/// Probability weights for each tile category on a planetary surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistributionWeights {
    pub copper: f32,
    pub iron: f32,
    pub rock: f32,
    pub empty: f32,
}

impl DistributionWeights {
    /// Sum of all weights, used for normalization.
    pub fn total(&self) -> f32 {
        self.copper + self.iron + self.rock + self.empty
    }

    /// Sample a tile type from this distribution using `roll` in `[0.0, 1.0)`.
    pub fn sample(&self, roll: f32) -> TileType {
        let total = self.total().max(f32::EPSILON);
        let scaled = roll.clamp(0.0, 1.0) * total;

        let copper_cut = self.copper;
        let iron_cut = copper_cut + self.iron;
        let rock_cut = iron_cut + self.rock;

        if scaled < copper_cut {
            TileType::CopperDeposit
        } else if scaled < iron_cut {
            TileType::IronDeposit
        } else if scaled < rock_cut {
            TileType::Rock
        } else {
            TileType::Empty
        }
    }
}

/// Surface data for a landable body.
///
/// Attached as a component to the celestial body's entity. `Grid` itself is
/// also used as a Bevy `Resource` for the *currently-viewed* surface, but the
/// canonical storage for each body lives here.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct PlanetSurface {
    pub grid: Grid,
    pub resource_distribution: ResourceDistribution,
}

/// The whole star system for the current game session.
///
/// `bodies` contains every celestial body entity (star + planets + moons).
/// `star` points at the central star entity, if any.
///
/// Like `CelestialBody::parent`, the entity lists are skipped during
/// serialization: they'd be stale across sessions. Saves should persist body
/// definitions separately (e.g., via the name + surface data) and rebuild
/// entities on load.
#[derive(Resource, Clone, Debug, Serialize, Deserialize, Default)]
pub struct StarSystem {
    pub name: String,
    #[serde(skip)]
    pub bodies: Vec<Entity>,
    #[serde(skip)]
    pub star: Option<Entity>,
}

/// The body the player is currently viewing (its surface is mirrored into the
/// `Grid` resource for all existing rendering/factory systems).
#[derive(Resource, Clone, Copy, Debug)]
pub struct CurrentBody(pub Entity);

// --- Systems ---

/// Advance the orbital angle of every celestial body that has a parent.
///
/// This runs every frame (or on a fixed tick) and simply increments
/// `orbit_angle`. Rendering systems can read the updated angle to place the
/// body on the system map. The star (and any body without a parent) is left
/// alone — it sits at the origin.
pub fn orbital_motion_system(mut bodies: Query<&mut CelestialBody>) {
    // Small constant; keeps motion slow enough to be observable without being
    // visually chaotic. Rendering layers can scale time as they wish.
    const ANGULAR_VELOCITY: f32 = 0.01;
    const TAU: f32 = std::f32::consts::TAU;

    for mut body in &mut bodies {
        if body.parent.is_some() {
            body.orbit_angle = (body.orbit_angle + ANGULAR_VELOCITY).rem_euclid(TAU);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_defaults_are_sane() {
        let e = Environment::earth_like();
        assert_eq!(e.gravity, 1.0);
        assert_eq!(e.atmosphere, 1.0);
        assert_eq!(e.temperature, 0.0);

        let v = Environment::vacuum();
        assert_eq!(v.atmosphere, 0.0);
    }

    #[test]
    fn body_position_on_origin_when_no_radius() {
        let body = CelestialBody {
            name: "Test".into(),
            body_type: BodyType::Planet,
            orbit_radius: 0.0,
            orbit_angle: 1.234,
            parent: None,
            environment: Environment::earth_like(),
        };
        let pos = body.position(Vec2::ZERO);
        assert!(pos.length() < 1e-6);
    }

    #[test]
    fn body_position_uses_parent_offset() {
        let body = CelestialBody {
            name: "Moon".into(),
            body_type: BodyType::Moon,
            orbit_radius: 10.0,
            orbit_angle: 0.0,
            parent: None,
            environment: Environment::vacuum(),
        };
        let pos = body.position(Vec2::new(100.0, 0.0));
        // angle 0 + radius 10 from x=100 → x=110, y=0
        assert!((pos.x - 110.0).abs() < 1e-5);
        assert!(pos.y.abs() < 1e-5);
    }

    #[test]
    fn distribution_weights_sample_produces_correct_tiles() {
        let w = DistributionWeights {
            copper: 0.25,
            iron: 0.25,
            rock: 0.25,
            empty: 0.25,
        };

        // 0.0 -> first bucket (copper)
        assert_eq!(w.sample(0.0), TileType::CopperDeposit);
        // 0.3 -> iron
        assert_eq!(w.sample(0.3), TileType::IronDeposit);
        // 0.6 -> rock
        assert_eq!(w.sample(0.6), TileType::Rock);
        // 0.9 -> empty
        assert_eq!(w.sample(0.9), TileType::Empty);
    }

    #[test]
    fn celestial_body_serde_round_trip() {
        let body = CelestialBody {
            name: "Terra".into(),
            body_type: BodyType::Planet,
            orbit_radius: 50.0,
            orbit_angle: 1.5,
            parent: None,
            environment: Environment::earth_like(),
        };
        let bytes = bincode::serialize(&body).unwrap();
        let restored: CelestialBody = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.name, body.name);
        assert_eq!(restored.body_type, body.body_type);
        assert!((restored.orbit_radius - body.orbit_radius).abs() < 1e-6);
        assert!((restored.orbit_angle - body.orbit_angle).abs() < 1e-6);
        assert!(restored.parent.is_none());
        assert_eq!(restored.environment, body.environment);
    }

    #[test]
    fn star_system_serde_round_trip() {
        let sys = StarSystem {
            name: "Sol".into(),
            bodies: Vec::new(),
            star: None,
        };
        let bytes = bincode::serialize(&sys).unwrap();
        let restored: StarSystem = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.name, sys.name);
    }

    #[test]
    fn resource_distribution_serde_round_trip() {
        for d in [
            ResourceDistribution::Balanced,
            ResourceDistribution::CopperRich,
            ResourceDistribution::IronRich,
            ResourceDistribution::Barren,
            ResourceDistribution::Lush,
        ] {
            let bytes = bincode::serialize(&d).unwrap();
            let restored: ResourceDistribution = bincode::deserialize(&bytes).unwrap();
            assert_eq!(restored, d);
        }
    }

    #[test]
    fn orbital_motion_system_updates_orbiting_bodies() {
        let mut app = App::new();
        app.add_systems(Update, orbital_motion_system);

        // Star at origin; no parent — should NOT move.
        let star = app
            .world_mut()
            .spawn(CelestialBody {
                name: "S".into(),
                body_type: BodyType::Planet,
                orbit_radius: 0.0,
                orbit_angle: 0.0,
                parent: None,
                environment: Environment::earth_like(),
            })
            .id();

        // Planet with parent = star — should have its angle increase.
        let planet = app
            .world_mut()
            .spawn(CelestialBody {
                name: "P".into(),
                body_type: BodyType::Planet,
                orbit_radius: 10.0,
                orbit_angle: 0.0,
                parent: Some(star),
                environment: Environment::earth_like(),
            })
            .id();

        let prev = app
            .world()
            .get::<CelestialBody>(planet)
            .unwrap()
            .orbit_angle;

        app.update();
        app.update();
        app.update();

        let after_planet = app
            .world()
            .get::<CelestialBody>(planet)
            .unwrap()
            .orbit_angle;
        let after_star = app.world().get::<CelestialBody>(star).unwrap().orbit_angle;

        assert!(
            after_planet > prev,
            "planet angle should monotonically increase, got {prev} -> {after_planet}"
        );
        assert_eq!(after_star, 0.0, "star should not rotate");
    }
}
