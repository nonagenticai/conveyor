//! The `prove` admission lane.
//!
//! Every compute endpoint (`/api/check`, `/api/elaborate`, `/api/prove`) spawns
//! `lake env lean`, and each of those can load Mathlib — so they all share ONE
//! lane. The lane admits `max_active` runs at once, queues `max_waiting` more
//! first-come-first-served, and answers everything else with 503 +
//! `Retry-After`. It is never 429: 429 means "you are too fast", 503 means
//! "the machine is busy", and callers already retry on the latter.
//!
//! The permit frees when the request unwinds, including on cancellation —
//! [`crate::runner`] kills the whole Lean process group in that case, so a
//! dropped request can never leave a Lean process running with no slot held.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::AppError;
use crate::metrics::Metrics;

#[derive(Debug)]
pub struct Gate {
    semaphore: Arc<Semaphore>,
    max_waiting: usize,
    max_wait: Duration,
    waiting: AtomicU64,
    metrics: Arc<Metrics>,
}

impl Gate {
    pub fn new(
        max_active: usize,
        max_waiting: usize,
        max_wait_s: f64,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_active.max(1))),
            max_waiting,
            max_wait: Duration::from_secs_f64(max_wait_s),
            waiting: AtomicU64::new(0),
            metrics,
        }
    }

    /// Wait for a slot, or fail with [`AppError::Busy`] (503) when the lane is
    /// saturated for longer than the configured wait.
    pub async fn acquire(&self) -> Result<Permit, AppError> {
        if let Ok(permit) = self.semaphore.clone().try_acquire_owned() {
            return Ok(self.admit(permit));
        }

        // Registration happens before the await so a request that is about to
        // queue is counted as a waiter; it is decremented on EVERY exit path,
        // otherwise a burst of timed-out waiters permanently inflates the count
        // and the lane starts refusing work it could serve.
        let queued = self.waiting.fetch_add(1, Ordering::SeqCst) + 1;
        if queued > self.max_waiting as u64 {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
            self.metrics
                .admission_rejected_total
                .fetch_add(1, Ordering::Relaxed);
            return Err(AppError::Busy(
                "lean lane is saturated: too many waiting requests".to_string(),
            ));
        }
        self.metrics
            .admission_waiting
            .store(queued, Ordering::Relaxed);

        let result =
            tokio::time::timeout(self.max_wait, self.semaphore.clone().acquire_owned()).await;

        self.waiting.fetch_sub(1, Ordering::SeqCst);
        self.metrics
            .admission_waiting
            .store(self.waiting.load(Ordering::Relaxed), Ordering::Relaxed);

        match result {
            Ok(Ok(permit)) => Ok(self.admit(permit)),
            Ok(Err(_)) => {
                // The semaphore was closed (shutdown). Same answer as busy.
                self.metrics
                    .admission_rejected_total
                    .fetch_add(1, Ordering::Relaxed);
                Err(AppError::Busy("lean lane is shutting down".to_string()))
            }
            Err(_) => {
                self.metrics
                    .admission_rejected_total
                    .fetch_add(1, Ordering::Relaxed);
                Err(AppError::Busy(format!(
                    "lean lane is saturated: no slot within {:.0}s",
                    self.max_wait.as_secs_f64()
                )))
            }
        }
    }

    fn admit(&self, permit: OwnedSemaphorePermit) -> Permit {
        self.metrics
            .admission_admitted_total
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .admission_active
            .fetch_add(1, Ordering::Relaxed);
        Permit {
            _permit: permit,
            metrics: Arc::clone(&self.metrics),
        }
    }
}

/// Holds one `prove` slot for as long as it is alive.
#[derive(Debug)]
pub struct Permit {
    _permit: OwnedSemaphorePermit,
    metrics: Arc<Metrics>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.metrics
            .admission_active
            .fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn admits_up_to_capacity_then_refuses() {
        let metrics = Arc::new(Metrics::default());
        let gate = Arc::new(Gate::new(1, 0, 0.0, Arc::clone(&metrics)));

        let held = gate.acquire().await.expect("first is admitted");
        let refused = gate.acquire().await;
        assert!(matches!(refused, Err(AppError::Busy(_))));
        assert_eq!(metrics.admission_rejected_total.load(Ordering::Relaxed), 1);

        drop(held);
        assert_eq!(metrics.admission_active.load(Ordering::Relaxed), 0);
        gate.acquire()
            .await
            .expect("slot is reusable after release");
    }

    #[tokio::test]
    async fn a_waiter_gives_up_after_max_wait() {
        let metrics = Arc::new(Metrics::default());
        let gate = Arc::new(Gate::new(1, 1, 0.05, Arc::clone(&metrics)));
        let _held = gate.acquire().await.expect("first is admitted");

        let start = std::time::Instant::now();
        let refused = gate.acquire().await;
        assert!(matches!(refused, Err(AppError::Busy(_))));
        assert!(start.elapsed() >= Duration::from_millis(40));
        // The waiter count must be back to zero, not leaked.
        assert_eq!(metrics.admission_waiting.load(Ordering::Relaxed), 0);
    }
}
