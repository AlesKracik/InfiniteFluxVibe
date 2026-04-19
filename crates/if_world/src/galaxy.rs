// galaxy.rs: Multi-system galaxy model.
//
// Phase 4 introduces interplanetary (actually inter-*system*) logistics. The
// game world now consists of a cluster of star systems connected by a sparse
// graph of warp lanes. A `Galaxy` resource describes the high-level map; each
// individual system is still represented at runtime by `StarSystem` + a set of
// `CelestialBody` entities (see `bodies.rs`).
//
// Systems are generated lazily — only the `active_system` is fully spawned as
// entities. Other systems live as `SystemDescriptor` metadata (name, position,
// seed) until visited. This keeps the ECS lightweight even in a many-system
// galaxy, and matches how the per-planet surface generator already works
// (surfaces are stored on each planet entity, not recomputed from seed).
//
// Galactic positions are in abstract "light-year" units — purely for UI map
// rendering and warp-distance calculations; there is no real physics here.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Metadata about one star system in the galaxy.
///
/// Kept deliberately small so the `Galaxy` resource is cheap to serialize even
/// with dozens of systems. `generated == true` means the system's bodies have
/// already been spawned as entities and do not need regenerating on visit;
/// `false` means the client should call `generate_star_system(seed)` when the
/// player first warps there.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SystemDescriptor {
    pub name: String,
    /// Galactic coordinates (2D light-year map).
    pub position: Vec2,
    /// Cached generation seed. Deterministic: visiting twice produces the same
    /// bodies both times.
    pub seed: u32,
    /// Has the system been visited/generated yet?
    pub generated: bool,
}

impl SystemDescriptor {
    pub fn new(name: impl Into<String>, position: Vec2, seed: u32) -> Self {
        Self {
            name: name.into(),
            position,
            seed,
            generated: false,
        }
    }
}

/// The galaxy: a flat list of systems plus a symmetric graph of warp lanes.
///
/// `warp_lanes` stores undirected edges as `(a, b)` with `a < b`. Helpers on
/// `Galaxy` treat the graph as symmetric regardless of lookup order so callers
/// never have to think about orientation.
///
/// `active_system` is an index into `systems` — the system currently rendered
/// on the orbital view. Warping updates this field; the client watches for the
/// change and (if needed) re-spawns the target system's entities.
#[derive(Resource, Clone, Debug, Serialize, Deserialize, Default)]
pub struct Galaxy {
    pub systems: Vec<SystemDescriptor>,
    /// Graph of warp lanes connecting systems. Stored symmetrically as a pair
    /// of indices `(a, b)` with `a < b`. Use `has_warp_lane` for lookups.
    pub warp_lanes: Vec<(usize, usize)>,
    /// Index of currently-active system in `systems`.
    pub active_system: usize,
}

impl Galaxy {
    /// Is there a direct warp lane between systems `a` and `b`?
    ///
    /// Symmetric: order of arguments does not matter.
    pub fn has_warp_lane(&self, a: usize, b: usize) -> bool {
        if a == b {
            return false;
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        self.warp_lanes.iter().any(|&(x, y)| x == lo && y == hi)
    }

    /// Galactic-space distance (light-years) between two systems, or `None` if
    /// either index is out of range.
    pub fn distance(&self, a: usize, b: usize) -> Option<f32> {
        let pa = self.systems.get(a)?.position;
        let pb = self.systems.get(b)?.position;
        Some(pa.distance(pb))
    }

    /// Add a warp lane between `a` and `b`, deduplicating and storing
    /// canonically (smaller index first). A no-op if `a == b` or the lane
    /// already exists.
    pub fn add_warp_lane(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        if !self.warp_lanes.iter().any(|&(x, y)| x == lo && y == hi) {
            self.warp_lanes.push((lo, hi));
        }
    }

    /// Currently-active system descriptor, if any.
    pub fn active(&self) -> Option<&SystemDescriptor> {
        self.systems.get(self.active_system)
    }
}

// -----------------------------------------------------------------------------
// Generation
// -----------------------------------------------------------------------------

/// Minimal deterministic RNG shared with `generation.rs`. We duplicate the
/// small splitmix implementation here so the `galaxy` module stays self-
/// contained (keeps unit tests independent and avoids cross-module visibility
/// churn). The constants are identical; same seeds produce the same outputs.
#[derive(Clone, Debug)]
struct GalRng {
    state: u64,
}

impl GalRng {
    fn new(seed: u64) -> Self {
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

    fn next_f32(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32; // 24 bits
        (bits as f32) / ((1u32 << 24) as f32)
    }

    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        debug_assert!(lo <= hi);
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as u32
    }
}

/// A pool of evocative system names. Enough entries to cover any generated
/// count with room to spare; we pick without replacement.
const SYSTEM_NAMES: &[&str] = &[
    "Sol",
    "Alpha Centauri",
    "Barnard",
    "Wolf",
    "Lalande",
    "Sirius",
    "Procyon",
    "Tau Ceti",
    "Epsilon Eridani",
    "Ross",
    "Gliese",
    "Kapteyn",
    "Vega",
    "Altair",
    "Kepler",
];

/// Warp-lane connection threshold in light-year units. Systems closer than this
/// are automatically joined by a warp lane.
pub const WARP_LANE_MAX_DISTANCE: f32 = 250.0;

/// Soft bound on the galactic cluster (systems are placed roughly within this
/// absolute coordinate on each axis).
pub const GALAXY_CLUSTER_RADIUS: f32 = 500.0;

/// Procedurally generate a galaxy of 8-12 star systems.
///
/// * Positions are uniformly scattered in a square of side `2 *
///   GALAXY_CLUSTER_RADIUS` centered on the origin. A later pass could spread
///   them with Lloyd relaxation; for phase 4 the UI does not require it.
/// * Warp lanes connect any pair closer than `WARP_LANE_MAX_DISTANCE`. After
///   the threshold pass we run a single connectivity sweep: every system gets
///   a warp lane to its nearest neighbor, guaranteeing the graph is connected
///   even if the threshold leaves some systems isolated.
/// * The first system is always flagged `generated = true` and named "Sol" so
///   it matches the default spawn done by `spawn_star_system`.
pub fn generate_galaxy(seed: u32) -> Galaxy {
    let mut rng = GalRng::new(seed as u64);

    let system_count = rng.range(8, 12) as usize;

    let mut systems: Vec<SystemDescriptor> = Vec::with_capacity(system_count);

    // Build a shuffled name pool (Fisher-Yates) so names are unique. "Sol"
    // is reserved for the home system (index 0), so exclude it here to
    // guarantee no duplicates even when the shuffle would otherwise place it
    // in one of the subsequent slots.
    let mut name_pool: Vec<String> = SYSTEM_NAMES
        .iter()
        .filter(|&&s| s != "Sol")
        .map(|s| s.to_string())
        .collect();
    for i in (1..name_pool.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        name_pool.swap(i, j);
    }

    // The home system is always index 0 with a stable name. We position it
    // near the origin so galactic map rendering is centered.
    let home_position = Vec2::new((rng.next_f32() - 0.5) * 40.0, (rng.next_f32() - 0.5) * 40.0);
    systems.push(SystemDescriptor {
        name: "Sol".to_string(),
        position: home_position,
        // Keep the home seed identical to `spawn_star_system`'s hard-coded
        // constant so Sol's planets match whether they are spawned via the
        // galaxy flow or the legacy startup path.
        seed: 42,
        generated: true,
    });

    for i in 1..system_count {
        let name = name_pool
            .get(i % name_pool.len())
            .cloned()
            .unwrap_or_else(|| format!("System-{i}"));
        let position = Vec2::new(
            (rng.next_f32() - 0.5) * 2.0 * GALAXY_CLUSTER_RADIUS,
            (rng.next_f32() - 0.5) * 2.0 * GALAXY_CLUSTER_RADIUS,
        );
        // Per-system seed is derived deterministically from the galaxy seed
        // so the same galaxy seed always produces identical planetary layouts.
        let sys_seed = seed
            .wrapping_add((i as u32).wrapping_mul(0x9E3779B1))
            .wrapping_add(0xA3F1);
        systems.push(SystemDescriptor::new(name, position, sys_seed));
    }

    let mut galaxy = Galaxy {
        systems,
        warp_lanes: Vec::new(),
        active_system: 0,
    };

    // Threshold pass: link every pair within range.
    for i in 0..galaxy.systems.len() {
        for j in (i + 1)..galaxy.systems.len() {
            let d = galaxy.systems[i]
                .position
                .distance(galaxy.systems[j].position);
            if d < WARP_LANE_MAX_DISTANCE {
                galaxy.add_warp_lane(i, j);
            }
        }
    }

    // Connectivity pass: ensure every system has at least one lane by joining
    // it to its nearest neighbor. Prevents stranded systems when the cluster
    // generates an outlier beyond `WARP_LANE_MAX_DISTANCE` from everything.
    for i in 0..galaxy.systems.len() {
        let has_any = galaxy.warp_lanes.iter().any(|&(a, b)| a == i || b == i);
        if has_any {
            continue;
        }
        // Find nearest j != i.
        let mut best: Option<(usize, f32)> = None;
        for j in 0..galaxy.systems.len() {
            if j == i {
                continue;
            }
            let d = galaxy.systems[i]
                .position
                .distance(galaxy.systems[j].position);
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((j, d));
            }
        }
        if let Some((j, _)) = best {
            galaxy.add_warp_lane(i, j);
        }
    }

    galaxy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galaxy_has_8_to_12_systems() {
        for seed in 0..32u32 {
            let g = generate_galaxy(seed);
            assert!(
                (8..=12).contains(&g.systems.len()),
                "seed {seed}: expected 8-12 systems, got {}",
                g.systems.len()
            );
        }
    }

    #[test]
    fn galaxy_names_are_unique() {
        let g = generate_galaxy(7);
        let mut names: Vec<&str> = g.systems.iter().map(|s| s.name.as_str()).collect();
        names.sort();
        let n_before = names.len();
        names.dedup();
        assert_eq!(n_before, names.len(), "duplicate system names");
    }

    #[test]
    fn galaxy_home_system_is_sol_at_index_zero() {
        let g = generate_galaxy(123);
        assert_eq!(g.systems[0].name, "Sol");
        assert!(g.systems[0].generated);
        assert_eq!(g.active_system, 0);
    }

    #[test]
    fn galaxy_positions_inside_cluster() {
        let g = generate_galaxy(5);
        for s in &g.systems {
            // Home may be near origin; all others should land inside cluster
            // bounds. Small numerical slack for float noise.
            assert!(s.position.x.abs() <= GALAXY_CLUSTER_RADIUS + 1.0);
            assert!(s.position.y.abs() <= GALAXY_CLUSTER_RADIUS + 1.0);
        }
    }

    #[test]
    fn galaxy_warp_lanes_are_symmetric() {
        let g = generate_galaxy(11);
        // Every stored pair should have lo < hi (canonical form).
        for &(a, b) in &g.warp_lanes {
            assert!(a < b, "lane ({a}, {b}) not canonical");
            // has_warp_lane works in either direction.
            assert!(g.has_warp_lane(a, b));
            assert!(g.has_warp_lane(b, a));
        }
    }

    #[test]
    fn galaxy_graph_is_connected() {
        // Every system must have at least one warp lane after connectivity
        // pass. (Not a strong connectivity guarantee, but enough for phase 4.)
        let g = generate_galaxy(99);
        for i in 0..g.systems.len() {
            let has_any = g.warp_lanes.iter().any(|&(a, b)| a == i || b == i);
            assert!(has_any, "system {i} has no warp lane");
        }
    }

    #[test]
    fn galaxy_is_deterministic() {
        let a = generate_galaxy(2026);
        let b = generate_galaxy(2026);
        assert_eq!(a.systems.len(), b.systems.len());
        for (sa, sb) in a.systems.iter().zip(b.systems.iter()) {
            assert_eq!(sa.name, sb.name);
            assert!((sa.position - sb.position).length() < 1e-5);
            assert_eq!(sa.seed, sb.seed);
        }
        assert_eq!(a.warp_lanes, b.warp_lanes);
    }

    #[test]
    fn galaxy_distance_works() {
        let mut g = Galaxy::default();
        g.systems.push(SystemDescriptor::new("A", Vec2::ZERO, 1));
        g.systems
            .push(SystemDescriptor::new("B", Vec2::new(3.0, 4.0), 2));
        assert!((g.distance(0, 1).unwrap() - 5.0).abs() < 1e-5);
        assert_eq!(g.distance(0, 99), None);
    }

    #[test]
    fn galaxy_add_warp_lane_deduplicates() {
        let mut g = Galaxy::default();
        g.systems.push(SystemDescriptor::new("A", Vec2::ZERO, 1));
        g.systems
            .push(SystemDescriptor::new("B", Vec2::new(10.0, 0.0), 2));
        g.add_warp_lane(0, 1);
        g.add_warp_lane(1, 0); // reversed, should dedupe
        g.add_warp_lane(0, 0); // self-loop, should be ignored
        assert_eq!(g.warp_lanes.len(), 1);
        assert!(g.has_warp_lane(0, 1));
    }

    #[test]
    fn galaxy_serde_round_trip() {
        let g = generate_galaxy(55);
        let bytes = bincode::serialize(&g).unwrap();
        let restored: Galaxy = bincode::deserialize(&bytes).unwrap();
        assert_eq!(restored.systems.len(), g.systems.len());
        assert_eq!(restored.warp_lanes, g.warp_lanes);
        assert_eq!(restored.active_system, g.active_system);
    }
}
