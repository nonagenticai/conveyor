//! The billing-service client: exactly one read, `GET /api/usage/breakdown`.
//!
//! The request/response mapping is stated in the crate docs (`lib.rs`). This module owns the
//! transport, the envelope unwrap and the decimal-string -> JSON-number conversion; it does
//! no arithmetic and never substitutes a value the upstream did not send.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Number;

use crate::error::ShimError;

/// The upstream route this shim reads. Its handler is `usage_cross_breakdown` in
/// `billing-service/src/routers/usage_breakdown.py` (router prefix `/api/usage`).
pub const BREAKDOWN_PATH: &str = "/api/usage/breakdown";

/// A slow billing read fails the caller's request instead of holding the connection open.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// A read of one tenant's cost rollup.
#[derive(Debug, Clone)]
pub struct RollupQuery {
    /// Inclusive UTC day (`YYYY-MM-DD`), sent as `from`.
    pub from: String,
    /// Inclusive UTC day (`YYYY-MM-DD`), sent as `to`.
    pub to: String,
    /// The requested dimensions, in caller order; sent as the upstream `group_by` CSV.
    pub group_by: Vec<String>,
    /// Forwarded as `X-Tenant-Id`; the upstream refuses a read without one.
    pub tenant_id: String,
}

/// The upstream breakdown, in the shim's own terms.
#[derive(Debug)]
pub struct Breakdown {
    pub rows: Vec<BreakdownRow>,
    /// The upstream `totals.cost_usd` — it covers groups beyond the upstream row cap.
    pub total_cost_usd: Number,
}

#[derive(Debug)]
pub struct BreakdownRow {
    /// Dimension name -> value; `None` is the upstream's null (unattributed spend).
    pub dims: BTreeMap<String, Option<String>>,
    pub cost_usd: Number,
}

/// The billing-service read client.
#[derive(Debug, Clone)]
pub struct BillingService {
    http: reqwest::Client,
    /// `None` is the explicitly-unconfigured state, answered as `503`.
    base_url: Option<reqwest::Url>,
    token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("BILLING_SERVICE_URL is not a URL: {0}")]
    BaseUrl(String),
    #[error("cannot build the HTTP client: {0}")]
    HttpClient(String),
}

impl BillingService {
    /// Build the client. The base URL is optional by design: absent is a serving state that
    /// answers `503 billing_unconfigured`, not a startup failure.
    pub fn new(base_url: Option<&str>, token: Option<String>) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .timeout(UPSTREAM_TIMEOUT)
            .build()
            .map_err(|error| ClientError::HttpClient(error.to_string()))?;
        let base_url = match base_url {
            Some(raw) => Some(
                reqwest::Url::parse(raw)
                    .map_err(|error| ClientError::BaseUrl(error.to_string()))?,
            ),
            None => None,
        };
        let token = token
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty());
        Ok(Self {
            http,
            base_url,
            token,
        })
    }

    /// Read the breakdown. Every failure path is a [`ShimError`]: unconfigured -> 503,
    /// everything else -> 502.
    pub async fn rollup(&self, query: &RollupQuery) -> Result<Breakdown, ShimError> {
        let base = self.base_url.as_ref().ok_or(ShimError::Unconfigured)?;
        // Concatenate rather than `Url::join`, so a base URL carrying a path prefix
        // (`https://gateway/billing`) keeps it.
        let url = format!("{}{}", base.as_str().trim_end_matches('/'), BREAKDOWN_PATH);
        let group_by = query.group_by.join(",");
        let mut request = self
            .http
            .get(url)
            .query(&[
                ("from", query.from.as_str()),
                ("to", query.to.as_str()),
                ("group_by", group_by.as_str()),
            ])
            .header("x-tenant-id", query.tenant_id.as_str())
            .header("accept", "application/json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(|error| {
            ShimError::upstream(format!("billing-service request failed: {error}"))
        })?;
        let status = response.status();
        let body = response.text().await.map_err(|error| {
            ShimError::upstream(format!("cannot read billing-service's answer: {error}"))
        })?;
        if !status.is_success() {
            return Err(ShimError::upstream(format!(
                "billing-service answered {status}: {}",
                snippet(&body)
            )));
        }

        let envelope: Envelope = serde_json::from_str(&body).map_err(|error| {
            ShimError::upstream(format!(
                "billing-service answered a body outside the breakdown contract: {error}"
            ))
        })?;
        let data = envelope.data;
        let mut rows = Vec::with_capacity(data.rows.len());
        for row in data.rows {
            let cost_usd = row
                .cost_usd
                .into_number()
                .map_err(|error| ShimError::upstream(format!("row cost_usd: {error}")))?;
            rows.push(BreakdownRow {
                dims: row.dims,
                cost_usd,
            });
        }
        let total_cost_usd = data
            .totals
            .cost_usd
            .into_number()
            .map_err(|error| ShimError::upstream(format!("totals.cost_usd: {error}")))?;
        Ok(Breakdown {
            rows,
            total_cost_usd,
        })
    }
}

/// The upstream's uniform `{data, meta}` wrapper; only `data` is read.
#[derive(Debug, Deserialize)]
struct Envelope {
    data: BreakdownBody,
}

#[derive(Debug, Deserialize)]
struct BreakdownBody {
    totals: Totals,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Totals {
    cost_usd: Money,
}

#[derive(Debug, Deserialize)]
struct Row {
    dims: BTreeMap<String, Option<String>>,
    cost_usd: Money,
}

/// The upstream serialises `Decimal` as a JSON **string**; a number is accepted too, so a
/// producer that switches to numeric decimals does not break the shim. Neither is rewritten
/// beyond parsing.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Money {
    Text(String),
    Number(Number),
}

impl Money {
    fn into_number(self) -> Result<Number, String> {
        match self {
            Self::Number(number) => Ok(number),
            Self::Text(text) => Number::from_str(text.trim())
                .map_err(|error| format!("{text:?} is not a JSON number: {error}")),
        }
    }
}

/// A bounded excerpt for the 502 detail; an upstream error page can be arbitrarily large.
fn snippet(body: &str) -> String {
    const LIMIT: usize = 300;
    let body = body.trim();
    if body.chars().count() <= LIMIT {
        return body.to_owned();
    }
    let mut cut: String = body.chars().take(LIMIT).collect();
    cut.push_str("...");
    cut
}
