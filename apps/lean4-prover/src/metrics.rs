//! Prometheus exposition.
//!
//! Deliberately hand-rolled rather than pulling a client library: the surface
//! is five counters and two gauges, and the metric NAMES are the contract —
//! `gpu_admission_*` is what the fleet's admission dashboards and alerts
//! already scrape from every other service, and the Prometheus job config
//! keeps `job_name: lean4-prover`. Renaming them here would silently drop the
//! series the panels read.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Metrics {
    pub admission_active: AtomicU64,
    pub admission_waiting: AtomicU64,
    pub admission_admitted_total: AtomicU64,
    pub admission_rejected_total: AtomicU64,
    pub requests_total: AtomicU64,
    pub lean_timeouts_total: AtomicU64,
    pub auth_rejected_total: AtomicU64,
    pub auth_unavailable_total: AtomicU64,
}

impl Metrics {
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(1024);

        let gauges = [
            (
                "gpu_admission_active",
                "Lean runs currently holding the prove lane",
                self.admission_active.load(Ordering::Relaxed),
            ),
            (
                "gpu_admission_waiting",
                "Requests waiting for the prove lane",
                self.admission_waiting.load(Ordering::Relaxed),
            ),
        ];
        for (name, help, value) in gauges {
            let _ = writeln!(out, "# HELP {name} {help}");
            let _ = writeln!(out, "# TYPE {name} gauge");
            let _ = writeln!(out, "{name}{{lane=\"prove\"}} {value}");
        }

        let counters = [
            (
                "gpu_admission_admitted_total",
                "Requests admitted to the prove lane",
                self.admission_admitted_total.load(Ordering::Relaxed),
            ),
            (
                "gpu_admission_rejected_total",
                "Requests refused by the prove lane (503)",
                self.admission_rejected_total.load(Ordering::Relaxed),
            ),
            (
                "lean4_prover_requests_total",
                "HTTP requests served",
                self.requests_total.load(Ordering::Relaxed),
            ),
            (
                "lean4_prover_lean_timeouts_total",
                "Lean runs killed after exceeding their timeout",
                self.lean_timeouts_total.load(Ordering::Relaxed),
            ),
            (
                "lean4_prover_auth_rejected_total",
                "Requests refused by the licence gate (401)",
                self.auth_rejected_total.load(Ordering::Relaxed),
            ),
            (
                "lean4_prover_auth_unavailable_total",
                "Requests refused because the licences service could not be reached (503)",
                self.auth_unavailable_total.load(Ordering::Relaxed),
            ),
        ];
        for (name, help, value) in counters {
            let _ = writeln!(out, "# HELP {name} {help}");
            let _ = writeln!(out, "# TYPE {name} counter");
            let _ = writeln!(out, "{name} {value}");
        }

        out
    }
}
