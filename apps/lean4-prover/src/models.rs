//! Wire types. Field names and nullability are the caller contract and are
//! carried over from the FastAPI service unchanged — `scientific-processor`
//! reads these keys.

use serde::{Deserialize, Serialize};

use crate::config::MAX_TIMEOUT_S;

pub const MAX_SOURCE_LEN: usize = 100_000;
pub const MAX_TERM_LEN: usize = 50_000;
pub const MAX_THEOREM_LEN: usize = 50_000;
pub const MAX_CONTEXT_LEN: usize = 100_000;
pub const MAX_TACTICS: usize = 50;
pub const MAX_IMPORTS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeanDiagnostic {
    /// `error`, `warning` or `information`.
    pub severity: String,
    /// 1-indexed line number, when the diagnostic names one.
    pub line: Option<u32>,
    /// 0-indexed column number, when the diagnostic names one.
    pub column: Option<u32>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    pub source: String,
    pub timeout: Option<u64>,
}

impl CheckRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_len("source", &self.source, 1, MAX_SOURCE_LEN)?;
        validate_timeout(self.timeout)
    }
}

#[derive(Debug, Deserialize)]
pub struct ElaborateRequest {
    pub term: String,
    pub context: Option<String>,
    pub timeout: Option<u64>,
}

impl ElaborateRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_len("term", &self.term, 1, MAX_TERM_LEN)?;
        if let Some(ctx) = &self.context {
            validate_len("context", ctx, 0, MAX_CONTEXT_LEN)?;
        }
        validate_timeout(self.timeout)
    }
}

#[derive(Debug, Deserialize)]
pub struct ProveRequest {
    pub theorem: String,
    pub tactics: Option<Vec<String>>,
    pub imports: Option<Vec<String>>,
    pub timeout: Option<u64>,
}

impl ProveRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_len("theorem", &self.theorem, 1, MAX_THEOREM_LEN)?;
        if let Some(tactics) = &self.tactics
            && tactics.len() > MAX_TACTICS
        {
            return Err(format!("tactics: at most {MAX_TACTICS} entries"));
        }
        if let Some(imports) = &self.imports
            && imports.len() > MAX_IMPORTS
        {
            return Err(format!("imports: at most {MAX_IMPORTS} entries"));
        }
        validate_timeout(self.timeout)
    }
}

fn validate_len(field: &str, value: &str, min: usize, max: usize) -> Result<(), String> {
    let len = value.chars().count();
    if len < min {
        return Err(format!("{field}: must be at least {min} character(s)"));
    }
    if len > max {
        return Err(format!("{field}: must be at most {max} characters"));
    }
    Ok(())
}

fn validate_timeout(timeout: Option<u64>) -> Result<(), String> {
    match timeout {
        None => Ok(()),
        Some(0) => Err("timeout: must be at least 1".to_string()),
        Some(t) if t > MAX_TIMEOUT_S => Err(format!(
            "timeout: must be at most {MAX_TIMEOUT_S} seconds, got {t}"
        )),
        Some(_) => Ok(()),
    }
}

#[derive(Debug, Serialize)]
pub struct CheckResponse {
    pub valid: bool,
    pub diagnostics: Vec<LeanDiagnostic>,
    pub stdout: String,
    pub stderr: String,
    pub computation_time_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct ElaborateResponse {
    pub elaborated_type: Option<String>,
    pub valid: bool,
    pub diagnostics: Vec<LeanDiagnostic>,
    pub stdout: String,
    pub stderr: String,
    pub computation_time_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct ProveResponse {
    pub proven: bool,
    pub successful_tactic: Option<String>,
    pub diagnostics: Vec<LeanDiagnostic>,
    pub stdout: String,
    pub stderr: String,
    pub computation_time_ms: f64,
}

/// Round to two decimals, matching the Python service's `round(x, 2)` so
/// dashboards comparing the two do not see spurious drift.
pub fn round_ms(ms: f64) -> f64 {
    (ms * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prove_rejects_more_than_fifty_tactics() {
        let req = ProveRequest {
            theorem: "example : True".into(),
            tactics: Some(vec!["trivial".into(); 51]),
            imports: None,
            timeout: None,
        };
        assert!(req.validate().unwrap_err().contains("tactics"));
    }

    #[test]
    fn timeout_above_ceiling_is_rejected() {
        let req = CheckRequest {
            source: "def x := 1".into(),
            timeout: Some(MAX_TIMEOUT_S + 1),
        };
        assert!(req.validate().unwrap_err().contains("timeout"));
    }

    #[test]
    fn empty_source_is_rejected() {
        let req = CheckRequest {
            source: String::new(),
            timeout: None,
        };
        assert!(req.validate().is_err());
    }
}
