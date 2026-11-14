//! `/api/stats` — meta-analysis, confidence intervals and trend detection.
//!
//! The request and response shapes are the captured ones, field for field, including the rounding
//! the old API applied. Validation mirrors the old Pydantic constraints and answers `422`; the
//! detail is a string here rather than FastAPI's list of field errors, which the tolerance list in
//! `contract/tolerances.toml` records.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::state::Shared;
use crate::stats::{self, IntervalMethod, Study, TimeSeriesPoint, TrendTest};

pub type Failure = (StatusCode, Json<Value>);

fn invalid(detail: impl Into<String>) -> Failure {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "detail": detail.into() })),
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct StudyResult {
    pub effect_size: f64,
    pub standard_error: f64,
    pub sample_size: i64,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct MetaAnalysisRequest {
    pub studies: Vec<StudyResult>,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default = "default_confidence")]
    pub confidence_level: f64,
}

fn default_method() -> String {
    "random".to_owned()
}

fn default_confidence() -> f64 {
    0.95
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct MetaAnalysisResponse {
    pub pooled_effect: f64,
    pub pooled_se: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub heterogeneity_q: f64,
    pub heterogeneity_i_squared: f64,
    pub method: String,
    pub study_weights: Vec<f64>,
    pub computation_time_ms: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ConfidenceIntervalRequest {
    pub values: Vec<f64>,
    #[serde(default = "default_confidence")]
    pub confidence_level: f64,
    #[serde(default = "default_interval_method")]
    pub method: String,
    #[serde(default = "default_bootstrap")]
    pub n_bootstrap: u32,
}

fn default_interval_method() -> String {
    "bootstrap".to_owned()
}

fn default_bootstrap() -> u32 {
    10_000
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ConfidenceIntervalResponse {
    pub mean: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub confidence_level: f64,
    pub method: String,
    pub computation_time_ms: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
pub struct SeriesPoint {
    pub timestamp: f64,
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct TrendRequest {
    pub data: Vec<SeriesPoint>,
    #[serde(default = "default_trend_test")]
    pub test: String,
}

fn default_trend_test() -> String {
    "mann_kendall".to_owned()
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct TrendResponse {
    pub trend: String,
    pub p_value: Option<f64>,
    pub statistic: Option<f64>,
    pub slope: Option<f64>,
    pub intercept: Option<f64>,
    pub computation_time_ms: f64,
}

fn check_confidence(level: f64) -> Result<(), Failure> {
    if level > 0.0 && level < 1.0 {
        Ok(())
    } else {
        Err(invalid(
            "confidence_level must be between 0 and 1 (exclusive)",
        ))
    }
}

/// `POST /api/stats/meta-analysis`
#[utoipa::path(
    post,
    path = "/api/stats/meta-analysis",
    operation_id = "statistics_meta_analysis",
    request_body = MetaAnalysisRequest,
    responses((status = 200, body = MetaAnalysisResponse), (status = 422, body = Value)),
    tag = "statistics"
)]
pub async fn meta_analysis(
    State(_state): State<Shared>,
    Json(body): Json<MetaAnalysisRequest>,
) -> Result<Json<MetaAnalysisResponse>, Failure> {
    let started = std::time::Instant::now();
    if body.studies.len() < 2 {
        return Err(invalid("studies must contain at least 2 entries"));
    }
    if body.method != "fixed" && body.method != "random" {
        return Err(invalid("method must be 'fixed' or 'random'"));
    }
    check_confidence(body.confidence_level)?;
    if let Some(bad) = body
        .studies
        .iter()
        .position(|s| s.standard_error <= 0.0 || s.sample_size <= 0)
    {
        return Err(invalid(format!(
            "studies[{bad}]: standard_error and sample_size must be positive"
        )));
    }

    let studies: Vec<Study> = body
        .studies
        .iter()
        .map(|s| Study {
            effect_size: s.effect_size,
            standard_error: s.standard_error,
            sample_size: s.sample_size,
        })
        .collect();
    let result = stats::meta_analysis(&studies, body.method == "random", body.confidence_level);
    Ok(Json(MetaAnalysisResponse {
        pooled_effect: result.pooled_effect,
        pooled_se: result.pooled_se,
        ci_lower: result.ci_lower,
        ci_upper: result.ci_upper,
        heterogeneity_q: result.heterogeneity_q,
        heterogeneity_i_squared: result.heterogeneity_i_squared,
        method: body.method,
        study_weights: result.study_weights,
        computation_time_ms: stats::round2(started.elapsed().as_secs_f64() * 1000.0),
    }))
}

/// `POST /api/stats/confidence-interval`
#[utoipa::path(
    post,
    path = "/api/stats/confidence-interval",
    operation_id = "statistics_confidence_interval",
    request_body = ConfidenceIntervalRequest,
    responses((status = 200, body = ConfidenceIntervalResponse), (status = 422, body = Value)),
    tag = "statistics"
)]
pub async fn confidence_interval(
    State(_state): State<Shared>,
    Json(body): Json<ConfidenceIntervalRequest>,
) -> Result<Json<ConfidenceIntervalResponse>, Failure> {
    let started = std::time::Instant::now();
    if body.values.len() < 2 || body.values.len() > 100_000 {
        return Err(invalid("values must contain between 2 and 100000 entries"));
    }
    check_confidence(body.confidence_level)?;
    if !(100..=100_000).contains(&body.n_bootstrap) {
        return Err(invalid("n_bootstrap must be between 100 and 100000"));
    }
    let method = match body.method.as_str() {
        "bootstrap" => IntervalMethod::Bootstrap,
        "normal" => IntervalMethod::Normal,
        _ => return Err(invalid("method must be 'bootstrap' or 'normal'")),
    };

    let result = stats::confidence_interval(
        &body.values,
        body.confidence_level,
        method,
        body.n_bootstrap,
    );
    Ok(Json(ConfidenceIntervalResponse {
        mean: result.mean,
        ci_lower: result.ci_lower,
        ci_upper: result.ci_upper,
        confidence_level: body.confidence_level,
        method: body.method,
        computation_time_ms: stats::round2(started.elapsed().as_secs_f64() * 1000.0),
    }))
}

/// `POST /api/stats/trend`
#[utoipa::path(
    post,
    path = "/api/stats/trend",
    operation_id = "statistics_trend",
    request_body = TrendRequest,
    responses((status = 200, body = TrendResponse), (status = 422, body = Value)),
    tag = "statistics"
)]
pub async fn trend(
    State(_state): State<Shared>,
    Json(body): Json<TrendRequest>,
) -> Result<Json<TrendResponse>, Failure> {
    let started = std::time::Instant::now();
    if body.data.len() < 3 || body.data.len() > 10_000 {
        return Err(invalid("data must contain between 3 and 10000 points"));
    }
    let test = match body.test.as_str() {
        "mann_kendall" => TrendTest::MannKendall,
        "linear_regression" => TrendTest::LinearRegression,
        _ => {
            return Err(invalid(
                "test must be 'mann_kendall' or 'linear_regression'",
            ));
        }
    };
    let points: Vec<TimeSeriesPoint> = body
        .data
        .iter()
        .map(|p| TimeSeriesPoint {
            timestamp: p.timestamp,
            value: p.value,
        })
        .collect();
    let result = stats::trend(&points, test);
    Ok(Json(TrendResponse {
        trend: result.trend.to_owned(),
        p_value: Some(result.p_value),
        statistic: Some(result.statistic),
        slope: result.slope,
        intercept: result.intercept,
        computation_time_ms: stats::round2(started.elapsed().as_secs_f64() * 1000.0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        Shared::new(crate::state::AppState::new(
            crate::config::Config::from_lookup(&[]).expect("defaults parse"),
        ))
    }

    #[tokio::test]
    async fn two_studies_pool_to_their_average() {
        let body = MetaAnalysisRequest {
            studies: vec![
                StudyResult {
                    effect_size: 0.5,
                    standard_error: 0.25,
                    sample_size: 100,
                    weight: None,
                    label: None,
                },
                StudyResult {
                    effect_size: 1.0,
                    standard_error: 0.25,
                    sample_size: 100,
                    weight: None,
                    label: None,
                },
            ],
            method: "fixed".to_owned(),
            confidence_level: 0.95,
        };
        let Json(response) = meta_analysis(State(shared()), Json(body))
            .await
            .expect("ok");
        assert!((response.pooled_effect - 0.75).abs() < 1e-9);
        assert_eq!(response.method, "fixed");
        assert_eq!(response.study_weights.len(), 2);
    }

    #[tokio::test]
    async fn one_study_is_refused_before_it_is_computed() {
        let body = MetaAnalysisRequest {
            studies: vec![StudyResult {
                effect_size: 0.5,
                standard_error: 0.25,
                sample_size: 10,
                weight: None,
                label: None,
            }],
            method: "fixed".to_owned(),
            confidence_level: 0.95,
        };
        let (status, Json(value)) = meta_analysis(State(shared()), Json(body))
            .await
            .expect_err("refused");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            value["detail"]
                .as_str()
                .expect("detail")
                .contains("at least 2")
        );
    }

    #[tokio::test]
    async fn a_monotone_series_is_increasing() {
        let body = TrendRequest {
            data: (0..10)
                .map(|i| SeriesPoint {
                    timestamp: i as f64,
                    value: i as f64,
                })
                .collect(),
            test: "mann_kendall".to_owned(),
        };
        let Json(response) = trend(State(shared()), Json(body)).await.expect("ok");
        assert_eq!(response.trend, "increasing");
    }
}
