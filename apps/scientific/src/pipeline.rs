//! The process-wide pipeline counters the `/api/pipeline/status` route reports.
//!
//! The old service kept these outside its request-scoped pipeline service for a reason that still
//! holds: the route must report real work, and a per-request instance would always read zero.
//!
//! What moved here is the counter, not the producer. `trigger`, `webhook` and `batch` were dropped
//! with the graph slice (their only input was the graph adapter), so nothing in this build advances
//! these values — the route answers what the process has actually done, which at present is
//! nothing, rather than a fabricated activity stream.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug)]
pub struct PipelineState {
    running: AtomicBool,
    queue_depth: AtomicU64,
    processed_total: AtomicU64,
    failed_total: AtomicU64,
    last_run_at: Mutex<Option<String>>,
    started: Instant,
}

/// The route's answer, before it is given the contract's field names.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineSnapshot {
    pub running: bool,
    pub queue_depth: u64,
    pub processed_total: u64,
    pub failed_total: u64,
    pub last_run_at: Option<String>,
    pub throughput_per_minute: f64,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            queue_depth: AtomicU64::new(0),
            processed_total: AtomicU64::new(0),
            failed_total: AtomicU64::new(0),
            last_run_at: Mutex::new(None),
            started: Instant::now(),
        }
    }
}

impl PipelineState {
    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Relaxed);
    }

    pub fn set_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }

    pub fn record_processed(&self, count: u64) {
        self.processed_total.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_failed(&self, count: u64) {
        self.failed_total.fetch_add(count, Ordering::Relaxed);
    }

    pub fn set_last_run_at(&self, at: Option<String>) {
        if let Ok(mut slot) = self.last_run_at.lock() {
            *slot = at;
        }
    }

    /// The lifetime average, in entities per minute, over this process's uptime.
    ///
    /// A window would be a different number under a different name; the field says "per minute"
    /// and the honest denominator for a counter that never resets is how long it has been counting.
    pub fn snapshot(&self) -> PipelineSnapshot {
        let processed_total = self.processed_total.load(Ordering::Relaxed);
        let elapsed_minutes = self.started.elapsed().as_secs_f64() / 60.0;
        let throughput_per_minute = if elapsed_minutes > 0.0 {
            processed_total as f64 / elapsed_minutes
        } else {
            0.0
        };
        PipelineSnapshot {
            running: self.running.load(Ordering::Relaxed),
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            processed_total,
            failed_total: self.failed_total.load(Ordering::Relaxed),
            last_run_at: self.last_run_at.lock().ok().and_then(|slot| slot.clone()),
            throughput_per_minute,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_process_has_done_nothing() {
        let snapshot = PipelineState::default().snapshot();
        assert!(!snapshot.running);
        assert_eq!(snapshot.processed_total, 0);
        assert_eq!(snapshot.throughput_per_minute, 0.0);
        assert!(snapshot.last_run_at.is_none());
    }

    #[test]
    fn the_counters_are_the_ones_that_were_recorded() {
        let state = PipelineState::default();
        state.set_running(true);
        state.set_queue_depth(3);
        state.record_processed(10);
        state.record_failed(2);
        state.set_last_run_at(Some("2026-11-14T17:00:00Z".to_owned()));
        let snapshot = state.snapshot();
        assert!(snapshot.running);
        assert_eq!(snapshot.queue_depth, 3);
        assert_eq!(snapshot.processed_total, 10);
        assert_eq!(snapshot.failed_total, 2);
        assert_eq!(
            snapshot.last_run_at.as_deref(),
            Some("2026-11-14T17:00:00Z")
        );
        assert!(snapshot.throughput_per_minute >= 0.0);
    }
}
