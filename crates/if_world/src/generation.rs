// generation.rs: Deterministic procedural generation of star systems.
//
// We intentionally avoid an external `rand` dependency: the workspace is
// meant to stay lean. Instead we use a tiny seedable PRNG (a 64-bit linear
// congruential generator, "SplitMix64"-style mixing) so generation is:
//   - deterministic (same seed ⇒ same system every time), and
//   - cheap to compile (no extra crate in the graph).
//
// The distributions encoded here are chosen so that a small statistical
// test on generated grids can detect bugs (e.g., weights flipped between
// ores). See tests at the bottom of this file and in `bodies.rs`.

use if_common::{DEFAULT_GRID_HEIGHT, DEFAULT_GRID_WIDTH};

use crate::bodies::{BodyType, CelestialBody, Environment, PlanetSurface, ResourceDistribution};
use crate::grid::Grid;

/// A minimal deterministic PRNG.
///
/// Internal state is a single `u64`. Each call to `next_u64` mixes the state
/// using the SplitMix64 avalanche function. This is not cryptographically
/// secure — it just needs to be uniform enough for terrain and repeatable
/// between runs. 64-bit splitmix is well-studied and has excellent
/// distribution for this use.
#[derive(Clone, Debug)]
struct SeededRng {
    state: u64,
}

impl SeededRng {
    fn new(seed: u64) -> Self {
        // Nonzero initial state; splitmix handles a zero seed fine, but we
        // still fold in a constant to decorrelate adjacent seeds.
        Self {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform f32 in `[0.0, 1.0)`.
    fn next_f32(&mut self) -> f32 {
        // Use the high 24 bits for maximum float precision.
        let bits = (self.next_u64() >> 40) as u32; // 24 bits
        (bits as f32) / ((1u32 << 24) as f32)
    }

    /// Integer in `[lo, hi]` inclusive.
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        debug_assert!(lo <= hi);
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as u32
    }
}

/// Generate a 32x32 surface grid for a planet with the given distribution.
///
/// Each tile is sampled independently from the distribution weights. This
/// gives a scattered, un-clumped layout which is fine for the Phase 3
/// foundation; later phases can add noise/clustering on top.
pub fn generate_planet_surface(distribution: ResourceDistribution, seed: u32) -> Grid {
    let mut rng = SeededRng::new(seed as u64);
    let weights = distribution.weights();

    let mut grid = Grid::new(DEFAULT_GRID_WIDTH, DEFAULT_GRID_HEIGHT);
    for y in 0..grid.height {
        for x in 0..grid.width {
            let tile = weights.sample(rng.next_f32());
            grid.set(x, y, tile);
        }
    }
    grid
}

/// One generated celestial body + (optionally) its surface.
///
/// We return this as a tuple so the caller can decide how to materialize
/// entities — in the client startup we spawn them directly; tests can
/// inspect the list without touching Bevy at all.
pub type GeneratedBody = (CelestialBody, Option<PlanetSurface>);

/// Procedurally generate a star system.
///
/// Produces a star (no surface) followed by 3–5 planets (each with a surface).
/// Each planet gets one small moon ~40% of the time (no surface for moons yet —
/// they exist as orbiting entities for the system-map view). Asteroids are
/// reserved for a later phase.
pub fn generate_star_system(seed: u32) -> Vec<GeneratedBody> {
    let mut rng = SeededRng::new(seed as u64);
    let mut out: Vec<GeneratedBody> = Vec::new();

    // --- The star ---
    out.push((CelestialBody::star("Sol"), None));

    // --- Planets ---
    let planet_count = rng.range(3, 5);
    let distributions = [
        ResourceDistribution::CopperRich,
        ResourceDistribution::IronRich,
        ResourceDistribution::Balanced,
        ResourceDistribution::Lush,
        ResourceDistribution::Barren,
    ];

    for i in 0..planet_count {
        // Spread planets across increasing orbital radii so the system map
        // has a clear ordering. Small random jitter avoids a perfectly even
        // spacing.
        let base_radius = 80.0 + (i as f32) * 60.0;
        let jitter = (rng.next_f32() - 0.5) * 20.0;
        let orbit_radius = base_radius + jitter;

        let orbit_angle = rng.next_f32() * std::f32::consts::TAU;

        // Pick a distribution based on the index, so test output is stable
        // but still varied.
        let distribution = distributions[(i as usize) % distributions.len()];

        let environment = planet_environment(distribution, &mut rng);

        let name = format!("Planet-{}", i + 1);

        let body = CelestialBody {
            name: name.clone(),
            body_type: BodyType::Planet,
            orbit_radius,
            orbit_angle,
            // Parent is resolved at entity-spawn time (see spawn_star_system
            // in grid.rs). Generation works with names; entities are wired up
            // by whoever materializes the system.
            parent: None,
            environment,
        };

        // Derive the surface seed from the system seed + index so each planet
        // has a stable surface even when the planet order doesn't change.
        let surface_seed = seed.wrapping_add(i * 1_000_003).wrapping_add(17);
        let surface = PlanetSurface {
            grid: generate_planet_surface(distribution, surface_seed),
            resource_distribution: distribution,
        };

        out.push((body, Some(surface)));

        // ~40% chance of a moon for this planet.
        if rng.next_f32() < 0.4 {
            let moon = CelestialBody {
                name: format!("{name}-Moon"),
                body_type: BodyType::Moon,
                orbit_radius: 15.0 + rng.next_f32() * 10.0,
                orbit_angle: rng.next_f32() * std::f32::consts::TAU,
                parent: None,
                environment: Environment::vacuum(),
            };
            out.push((moon, None));
        }
    }

    out
}

/// Pick an environment compatible with the distribution so the two feel
/// consistent (Barren ⇒ thin atmosphere, Lush ⇒ Earth-like, etc.).
fn planet_environment(distribution: ResourceDistribution, rng: &mut SeededRng) -> Environment {
    let mut jitter = || (rng.next_f32() - 0.5) * 0.2;
    match distribution {
        ResourceDistribution::Lush => Environment {
            gravity: 1.0 + jitter(),
            atmosphere: 1.0,
            temperature: 0.0 + jitter(),
        },
        ResourceDistribution::Balanced => Environment {
            gravity: 0.9 + jitter(),
            atmosphere: 0.7,
            temperature: 0.1 + jitter(),
        },
        ResourceDistribution::CopperRich => Environment {
            gravity: 1.1 + jitter(),
            atmosphere: 0.5,
            temperature: 0.4 + jitter(),
        },
        ResourceDistribution::IronRich => Environment {
            gravity: 1.2 + jitter(),
            atmosphere: 0.3,
            temperature: -0.2 + jitter(),
        },
        ResourceDistribution::Barren => Environment {
            gravity: 0.6 + jitter(),
            atmosphere: 0.05,
            temperature: -0.7 + jitter(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use if_common::TileType;

    fn count_tiles(grid: &Grid) -> (u32, u32, u32, u32) {
        let (mut copper, mut iron, mut rock, mut empty) = (0u32, 0u32, 0u32, 0u32);
        for y in 0..grid.height {
            for x in 0..grid.width {
                match grid.get(x, y).unwrap() {
                    TileType::CopperDeposit => copper += 1,
                    TileType::IronDeposit => iron += 1,
                    TileType::Rock => rock += 1,
                    TileType::Empty => empty += 1,
                }
            }
        }
        (copper, iron, rock, empty)
    }

    #[test]
    fn generate_star_system_creates_3_to_5_planets() {
        for seed in 0..32u32 {
            let bodies = generate_star_system(seed);
            let planet_count = bodies
                .iter()
                .filter(|(b, _)| b.body_type == BodyType::Planet && b.parent.is_none())
                // first body is the star (also parent == None) — skip by surface presence
                // (the star has no surface, planets do)
                .filter(|(_, s)| s.is_some())
                .count();
            assert!(
                (3..=5).contains(&planet_count),
                "seed {seed}: expected 3–5 planets, got {planet_count}"
            );
        }
    }

    #[test]
    fn generate_star_system_is_deterministic() {
        let a = generate_star_system(42);
        let b = generate_star_system(42);
        assert_eq!(a.len(), b.len());
        for (i, ((ba, sa), (bb, sb))) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(ba.name, bb.name, "body {i} name differs");
            assert_eq!(ba.body_type, bb.body_type);
            assert!((ba.orbit_radius - bb.orbit_radius).abs() < 1e-5);
            assert!((ba.orbit_angle - bb.orbit_angle).abs() < 1e-5);
            assert_eq!(sa.is_some(), sb.is_some());
            if let (Some(sa), Some(sb)) = (sa, sb) {
                assert_eq!(sa.resource_distribution, sb.resource_distribution);
                assert_eq!(sa.grid.tiles(), sb.grid.tiles());
            }
        }
    }

    #[test]
    fn copper_rich_produces_more_copper_than_iron() {
        let grid = generate_planet_surface(ResourceDistribution::CopperRich, 1);
        let (copper, iron, _rock, _empty) = count_tiles(&grid);
        assert!(
            copper > iron,
            "CopperRich should have more copper ({copper}) than iron ({iron})"
        );
        // At 32*32=1024 tiles with copper weight 0.35 we expect ~358 copper
        // and ~102 iron. Allow generous slack for the small sample.
        assert!(
            copper >= 2 * iron,
            "CopperRich copper={copper} iron={iron} — ratio too low"
        );
    }

    #[test]
    fn iron_rich_produces_more_iron_than_copper() {
        let grid = generate_planet_surface(ResourceDistribution::IronRich, 2);
        let (copper, iron, _rock, _empty) = count_tiles(&grid);
        assert!(
            iron > copper,
            "IronRich should have more iron ({iron}) than copper ({copper})"
        );
        assert!(
            iron >= 2 * copper,
            "IronRich iron={iron} copper={copper} — ratio too low"
        );
    }

    #[test]
    fn barren_has_very_few_deposits() {
        let grid = generate_planet_surface(ResourceDistribution::Barren, 3);
        let (copper, iron, rock, empty) = count_tiles(&grid);
        let deposits = copper + iron;
        let non_deposits = rock + empty;
        assert!(
            non_deposits > deposits * 5,
            "Barren should be mostly rock/empty: deposits={deposits} non_deposits={non_deposits}"
        );
    }

    #[test]
    fn lush_has_many_deposits() {
        let grid = generate_planet_surface(ResourceDistribution::Lush, 4);
        let (copper, iron, _rock, _empty) = count_tiles(&grid);
        let deposits = copper + iron;
        // Lush has 0.25 + 0.25 = 0.5 weight on deposits. At 1024 tiles ~512.
        assert!(
            deposits > 300,
            "Lush should have many deposits, got {deposits}"
        );
    }

    #[test]
    fn balanced_has_roughly_equal_copper_and_iron() {
        let grid = generate_planet_surface(ResourceDistribution::Balanced, 5);
        let (copper, iron, _rock, _empty) = count_tiles(&grid);
        let diff = copper.abs_diff(iron) as f32;
        let total = (copper + iron) as f32;
        // Expect symmetric weights; allow 60% slack for small sample noise.
        assert!(
            total > 0.0 && diff / total < 0.6,
            "Balanced copper={copper} iron={iron} too skewed"
        );
    }

    #[test]
    fn generate_planet_surface_is_deterministic() {
        let a = generate_planet_surface(ResourceDistribution::CopperRich, 123);
        let b = generate_planet_surface(ResourceDistribution::CopperRich, 123);
        assert_eq!(a.tiles(), b.tiles());
    }

    #[test]
    fn generate_planet_surface_differs_across_seeds() {
        let a = generate_planet_surface(ResourceDistribution::Balanced, 1);
        let b = generate_planet_surface(ResourceDistribution::Balanced, 2);
        assert_ne!(a.tiles(), b.tiles());
    }
}
