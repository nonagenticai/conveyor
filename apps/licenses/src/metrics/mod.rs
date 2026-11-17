//! Prometheus exposition for the absorbed licences service.
//!
//! The Python reference (`services/expiry_metrics.py` and `services/tenant_subject_invariant.py`
//! in the archived repository) registered two custom collectors on one private registry behind
//! `GET /metrics`; this module is that endpoint's Rust home. `expiry` owns the licence/token
//! expiry collector and `tenant_subject` owns the tenant-id-is-a-principal collector. They fail
//! independently: a database fault in one leaves the other's families on the wire, exactly as the
//! two Python collectors did.
//!
//! Every family is a GAUGE and is hand-rendered. `metrics-exporter-prometheus` is used only for
//! the process-wide recorder handle; the families below are fresh snapshots of small tables, not
//! values pushed through the macros, so hand-rendering keeps the label sets and the zero-seeded
//! samples under direct control.
//!
//! ⚠️ Deliberate deviation from the reference: the Python collectors cached each snapshot for
//! 60 seconds because their engine used NullPool (a fresh database connection per scrape). This
//! service uses a pooled connection and these tables are tiny, so a snapshot is taken per scrape:
//! strictly fresher than the reference, at ~14 indexed queries. If scrape cost ever becomes a
//! measured problem, add the TTL here rather than at a call site.
//!
//! `/metrics` is outside the captured OpenAPI contract on purpose: it is in the auth middleware's
//! public set (`auth::PUBLIC_ENDPOINTS`) and is never added to `router::ROUTES` or the `OpenApi`
//! document, so `--check-lock` does not see it.

pub mod expiry;
pub mod tenant_subject;

use std::sync::OnceLock;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sqlx::PgPool;

/// The process-wide recorder. `install_recorder` may run once per process; tests that build
/// several routers reuse the same registry.
pub fn handle() -> &'static PrometheusHandle {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("installing the Prometheus recorder once per process")
    })
}

/// The complete `/metrics` body: the recorder's own series, then both collectors' families.
///
/// Always answers, whatever the database does: each collector renders its own
/// `..._collector_up` gauge and nothing else when its queries fail.
pub async fn render(pool: &Option<PgPool>) -> String {
    let mut body = handle().render();
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(&expiry::render(pool).await);
    body.push_str(&tenant_subject::render(pool).await);
    body
}

/// One gauge family: a HELP/TYPE header and its samples.
///
/// `labels` must be in alphabetical order and every sample's values are positional against it —
/// the reference's exposition sorts label names alphabetically, and a stable order keeps the two
/// implementations diffable.
pub(crate) struct Family {
    name: &'static str,
    help: &'static str,
    labels: &'static [&'static str],
    samples: Vec<(Vec<String>, f64)>,
}

impl Family {
    pub(crate) fn new(
        name: &'static str,
        help: &'static str,
        labels: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            help,
            labels,
            samples: Vec::new(),
        }
    }

    /// Add a sample. `values` must be positional against `labels`.
    pub(crate) fn sample(&mut self, values: Vec<String>, value: f64) {
        self.samples.push((values, value));
    }

    /// The family's exposition text. The HELP/TYPE header is emitted even with no samples: the
    /// reference's `GaugeMetricFamily` does the same, and an empty family (no active licence has
    /// an expiry today) must be distinguishable from a missing one.
    pub(crate) fn render(&self) -> String {
        let mut out = format!(
            "# HELP {} {}\n# TYPE {} gauge\n",
            self.name, self.help, self.name
        );
        for (values, value) in &self.samples {
            if self.labels.is_empty() {
                out.push_str(&format!("{} {}\n", self.name, value));
                continue;
            }
            out.push_str(self.name);
            out.push('{');
            for (index, (label, label_value)) in self.labels.iter().zip(values).enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(label);
                out.push_str("=\"");
                out.push_str(&escape(label_value));
                out.push('"');
            }
            out.push_str(&format!("}} {}\n", value));
        }
        out
    }
}

/// Escape a label value: backslash, double quote and newline, per the text exposition format.
pub(crate) fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_escaped_and_rendered_in_declared_order() {
        let mut family = Family::new(
            "nonagentic_license_example",
            "an example",
            &["app", "tenant"],
        );
        family.sample(vec!["a\"b".to_owned(), "c\\d".to_owned()], 1.0);
        let text = family.render();
        assert!(text.contains("# HELP nonagentic_license_example an example"));
        assert!(text.contains("# TYPE nonagentic_license_example gauge"));
        assert!(text.contains("nonagentic_license_example{app=\"a\\\"b\",tenant=\"c\\\\d\"} 1"));
    }

    #[test]
    fn an_empty_family_still_carries_its_header() {
        let family = Family::new("nonagentic_license_empty", "nothing due", &["kind"]);
        let text = family.render();
        assert_eq!(
            text,
            "# HELP nonagentic_license_empty nothing due\n\
             # TYPE nonagentic_license_empty gauge\n"
        );
    }

    #[test]
    fn a_newline_in_a_label_does_not_break_the_exposition() {
        let mut family = Family::new("nonagentic_license_example", "an example", &["tenant"]);
        family.sample(vec!["a\nb".to_owned()], 1.0);
        let text = family.render();
        assert!(text.contains("tenant=\"a\\nb\""));
        assert_eq!(text.lines().count(), 3);
    }
}
