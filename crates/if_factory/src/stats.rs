// stats.rs: Throughput tracking for buildings.
//
// Records how many items pass through a building's inventory over a
// rolling time window, allowing items/minute calculations.

use bevy::prelude::*;

use crate::inventory::Inventory;

/// How many ticks to track. At 64 ticks/sec, 640 ticks = 10 seconds.
/// We extrapolate to items/minute from this window.
const TRACKING_WINDOW: usize = 640;

/// Component that tracks item throughput for a building.
///
/// It works by snapshotting the inventory's total count each tick and
/// comparing against the count from TRACKING_WINDOW ticks ago. The
/// difference tells us how many items were added in that window.
///
/// We use a circular buffer (ring buffer) to avoid shifting a Vec
/// every tick. `head` points to where the next snapshot goes.
#[derive(Component, Debug)]
pub struct ThroughputTracker {
    /// Circular buffer of inventory total counts, one per tick.
    snapshots: Vec<u32>,
    /// Index where the next snapshot will be written.
    head: usize,
    /// Items per minute (calculated from the rolling window).
    pub items_per_minute: f32,
}

impl ThroughputTracker {
    pub fn new() -> Self {
        Self {
            snapshots: vec![0; TRACKING_WINDOW],
            head: 0,
            items_per_minute: 0.0,
        }
    }
}

impl Default for ThroughputTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// System: update throughput trackers each tick.
///
/// For each building with both an Inventory and a ThroughputTracker,
/// snapshot the current total and compute items/minute.
pub fn throughput_tracking_system(mut tracker_q: Query<(&Inventory, &mut ThroughputTracker)>) {
    for (inventory, mut tracker) in &mut tracker_q {
        let current_total = inventory.total_count();

        // Copy head index to avoid simultaneous borrow of tracker.snapshots
        // and tracker.head (same borrow checker pattern as in transport.rs).
        let head = tracker.head;

        // The oldest snapshot in the ring buffer is at the current head
        // (it's about to be overwritten).
        let oldest_total = tracker.snapshots[head];

        // Write current snapshot
        tracker.snapshots[head] = current_total;
        tracker.head = (head + 1) % TRACKING_WINDOW;

        // Calculate throughput: items gained over the window period.
        // The window is TRACKING_WINDOW ticks. At 64 ticks/sec, that's 10 seconds.
        // Multiply by 6 to extrapolate to items/minute.
        let items_in_window = current_total.saturating_sub(oldest_total);
        tracker.items_per_minute = items_in_window as f32 * (60.0 * 64.0 / TRACKING_WINDOW as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Inventory;
    use bevy::ecs::world::World;
    use if_common::item::ItemType;

    #[test]
    fn tracker_starts_at_zero() {
        let tracker = ThroughputTracker::new();
        assert_eq!(tracker.items_per_minute, 0.0);
    }

    #[test]
    fn stable_inventory_has_zero_throughput() {
        let mut world = World::new();

        // Start empty — no items added during the window
        world.spawn((Inventory::new(1000), ThroughputTracker::new()));

        let mut schedule = Schedule::default();
        schedule.add_systems(throughput_tracking_system);

        // Run one full window of ticks
        for _ in 0..TRACKING_WINDOW {
            schedule.run(&mut world);
        }

        // No items were added, so throughput should be 0
        let mut query = world.query::<&ThroughputTracker>();
        let tracker = query.single(&world).unwrap();
        assert_eq!(tracker.items_per_minute, 0.0);
    }

    #[test]
    fn tracker_reflects_additions() {
        let mut world = World::new();

        let entity = world
            .spawn((Inventory::new(1000), ThroughputTracker::new()))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(throughput_tracking_system);

        // Run a few ticks with no items
        for _ in 0..10 {
            schedule.run(&mut world);
        }

        // Add some items
        world
            .get_mut::<Inventory>(entity)
            .unwrap()
            .try_add(ItemType::CopperOre, 100);

        // Run the rest of the window
        for _ in 0..(TRACKING_WINDOW - 10) {
            schedule.run(&mut world);
        }

        let mut query = world.query::<&ThroughputTracker>();
        let tracker = query.single(&world).unwrap();
        assert!(tracker.items_per_minute > 0.0);
    }
}
