// power.rs: Power generation and consumption.
//
// Buildings consume power, generators produce it. The PowerGrid resource
// tracks the ratio of generation to consumption. When demand exceeds supply,
// machines slow down proportionally.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Component for buildings that consume power (drills, machines).
/// `demand` is the power units required per tick at full speed.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct PowerConsumer {
    pub demand: f32,
}

/// Component for buildings that generate power.
/// `output` is the power units produced per tick.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct PowerGenerator {
    pub output: f32,
}

/// Global resource tracking the power balance across all buildings.
///
/// `power_ratio` is clamped to 0.0–1.0:
///   1.0 = enough power (or surplus) — everything runs at full speed
///   0.5 = half the needed power — machines run at half speed
///   0.0 = no generators — everything stops
///
/// The ratio is recalculated every tick by the power_system.
#[derive(Resource)]
pub struct PowerGrid {
    pub total_generation: f32,
    pub total_consumption: f32,
    pub power_ratio: f32,
}

impl Default for PowerGrid {
    fn default() -> Self {
        Self {
            total_generation: 0.0,
            total_consumption: 0.0,
            // Start at 1.0 so buildings work before any generators exist.
            // This makes early gameplay simpler — power is a concern you
            // add later, not a blocker from the start.
            power_ratio: 1.0,
        }
    }
}

/// System: recalculate the power grid balance each tick.
///
/// Sums all generators and all consumers, then computes the ratio.
/// Other systems (mining, production) read `Res<PowerGrid>` to scale
/// their speed.
pub fn power_system(
    mut grid: ResMut<PowerGrid>,
    generators_q: Query<&PowerGenerator>,
    consumers_q: Query<&PowerConsumer>,
) {
    let total_gen: f32 = generators_q.iter().map(|g| g.output).sum();
    let total_con: f32 = consumers_q.iter().map(|c| c.demand).sum();

    grid.total_generation = total_gen;
    grid.total_consumption = total_con;

    // If no consumers or enough power, ratio is 1.0.
    // Otherwise, it's the fraction of demand that is met.
    grid.power_ratio = if total_con <= 0.0 || total_gen >= total_con {
        1.0
    } else {
        total_gen / total_con
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;

    #[test]
    fn no_buildings_means_full_power() {
        let mut world = World::new();
        world.insert_resource(PowerGrid::default());

        let mut schedule = Schedule::default();
        schedule.add_systems(power_system);
        schedule.run(&mut world);

        let grid = world.resource::<PowerGrid>();
        assert_eq!(grid.power_ratio, 1.0);
    }

    #[test]
    fn surplus_power_is_clamped_to_one() {
        let mut world = World::new();
        world.insert_resource(PowerGrid::default());
        world.spawn(PowerGenerator { output: 100.0 });
        world.spawn(PowerConsumer { demand: 30.0 });

        let mut schedule = Schedule::default();
        schedule.add_systems(power_system);
        schedule.run(&mut world);

        let grid = world.resource::<PowerGrid>();
        assert_eq!(grid.power_ratio, 1.0);
    }

    #[test]
    fn partial_power_gives_fractional_ratio() {
        let mut world = World::new();
        world.insert_resource(PowerGrid::default());
        world.spawn(PowerGenerator { output: 50.0 });
        world.spawn(PowerConsumer { demand: 100.0 });

        let mut schedule = Schedule::default();
        schedule.add_systems(power_system);
        schedule.run(&mut world);

        let grid = world.resource::<PowerGrid>();
        assert!((grid.power_ratio - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn no_generators_means_zero_power() {
        let mut world = World::new();
        world.insert_resource(PowerGrid::default());
        world.spawn(PowerConsumer { demand: 50.0 });

        let mut schedule = Schedule::default();
        schedule.add_systems(power_system);
        schedule.run(&mut world);

        let grid = world.resource::<PowerGrid>();
        assert_eq!(grid.power_ratio, 0.0);
    }
}
