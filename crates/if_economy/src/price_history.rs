// price_history.rs: Ring-buffer of recent trade prices.
//
// Used to drive UI charts and indicators ("latest price", "avg",
// "session high / low"). We use a fixed-size VecDeque so memory is
// bounded: once `max_samples` is hit, the oldest sample is evicted.
//
// All math is integer — avg() uses `sum_of_cents / n` with truncation
// toward zero. This means the UI average might be one cent below the
// "true" mean; that's intentional and acceptable.

use crate::credits::Credits;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Ring-buffer of (tick, price) samples.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceHistory {
    /// Samples in insertion order, oldest at front.
    pub samples: VecDeque<(u64, Credits)>,
    /// Cap on number of samples retained.
    pub max_samples: usize,
}

impl PriceHistory {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples: max_samples.max(1),
        }
    }

    /// Append a sample, evicting the oldest if at capacity.
    pub fn record(&mut self, tick: u64, price: Credits) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back((tick, price));
    }

    /// Most recent price, if any.
    pub fn latest(&self) -> Option<Credits> {
        self.samples.back().map(|(_, p)| *p)
    }

    /// Arithmetic mean (truncated to cent). None if empty.
    pub fn avg(&self) -> Option<Credits> {
        if self.samples.is_empty() {
            return None;
        }
        // Sum in i128 to avoid i64 overflow across many samples.
        let sum: i128 = self.samples.iter().map(|(_, p)| p.cents() as i128).sum();
        let n = self.samples.len() as i128;
        // Truncating division — fine for a UI indicator.
        let avg_cents = (sum / n) as i64;
        Some(Credits::from_cents(avg_cents))
    }

    /// (min, max) across the buffer. None if empty.
    pub fn min_max(&self) -> Option<(Credits, Credits)> {
        let mut iter = self.samples.iter().map(|(_, p)| *p);
        let first = iter.next()?;
        let mut lo = first;
        let mut hi = first;
        for p in iter {
            if p < lo {
                lo = p;
            }
            if p > hi {
                hi = p;
            }
        }
        Some((lo, hi))
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

impl Default for PriceHistory {
    /// Default window: 256 samples — plenty for a few minutes of trades.
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_empty_history() {
        let h = PriceHistory::new(10);
        assert!(h.is_empty());
        assert!(h.latest().is_none());
        assert!(h.avg().is_none());
        assert!(h.min_max().is_none());
    }

    #[test]
    fn record_adds_samples() {
        let mut h = PriceHistory::new(10);
        h.record(1, Credits::from_whole(10));
        h.record(2, Credits::from_whole(20));
        assert_eq!(h.len(), 2);
        assert_eq!(h.latest(), Some(Credits::from_whole(20)));
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let mut h = PriceHistory::new(3);
        h.record(1, Credits::from_whole(10));
        h.record(2, Credits::from_whole(20));
        h.record(3, Credits::from_whole(30));
        h.record(4, Credits::from_whole(40));
        assert_eq!(h.len(), 3);
        // First sample (tick=1) should be evicted
        let ticks: Vec<u64> = h.samples.iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![2, 3, 4]);
    }

    #[test]
    fn avg_of_samples() {
        let mut h = PriceHistory::new(10);
        h.record(1, Credits::from_whole(10));
        h.record(2, Credits::from_whole(20));
        h.record(3, Credits::from_whole(30));
        assert_eq!(h.avg(), Some(Credits::from_whole(20)));
    }

    #[test]
    fn avg_truncates_toward_zero() {
        let mut h = PriceHistory::new(10);
        h.record(1, Credits::from_cents(100));
        h.record(2, Credits::from_cents(101));
        // Mean = 100.5 cents -> truncates to 100 cents
        assert_eq!(h.avg(), Some(Credits::from_cents(100)));
    }

    #[test]
    fn min_max() {
        let mut h = PriceHistory::new(10);
        h.record(1, Credits::from_whole(10));
        h.record(2, Credits::from_whole(5));
        h.record(3, Credits::from_whole(20));
        h.record(4, Credits::from_whole(8));
        let (lo, hi) = h.min_max().unwrap();
        assert_eq!(lo, Credits::from_whole(5));
        assert_eq!(hi, Credits::from_whole(20));
    }

    #[test]
    fn min_max_single_sample() {
        let mut h = PriceHistory::new(10);
        h.record(1, Credits::from_whole(7));
        let (lo, hi) = h.min_max().unwrap();
        assert_eq!(lo, Credits::from_whole(7));
        assert_eq!(hi, Credits::from_whole(7));
    }

    #[test]
    fn zero_max_samples_clamped_to_one() {
        let mut h = PriceHistory::new(0);
        h.record(1, Credits::from_whole(5));
        h.record(2, Credits::from_whole(10));
        // After clamping, at most 1 sample
        assert_eq!(h.len(), 1);
        assert_eq!(h.latest(), Some(Credits::from_whole(10)));
    }

    #[test]
    fn default_has_reasonable_size() {
        let h = PriceHistory::default();
        assert!(h.max_samples >= 64);
    }
}
