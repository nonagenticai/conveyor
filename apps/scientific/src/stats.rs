//! The statistical core: meta-analysis, confidence intervals and trend detection.
//!
//! These are ports of the old service's `services/stats_service.py`, and the numbers must match
//! it to the rounding the old API applied (6 decimals for point estimates, 4 for heterogeneity,
//! 2 for elapsed milliseconds). The one deliberately non-identical part is the bootstrap
//! confidence interval: the old service drew from numpy's `default_rng(42)` and this draws from a
//! local SplitMix64, so two honest implementations of the same estimator differ in the third
//! decimal. `contract/tolerances.toml` declares that, and `tests/parity.rs` holds it to it.

use statrs::distribution::{ContinuousCDF, Normal, StudentsT};

/// Round to `places` decimals, half away from zero — Python's `round` on a float, which is what
/// the old API emitted.
fn round_to(value: f64, places: u32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let factor = 10f64.powi(places as i32);
    let scaled = value * factor;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5).floor()
    } else {
        (scaled - 0.5).ceil()
    };
    rounded / factor
}

pub fn round2(value: f64) -> f64 {
    round_to(value, 2)
}

pub fn round4(value: f64) -> f64 {
    round_to(value, 4)
}

pub fn round6(value: f64) -> f64 {
    round_to(value, 6)
}

fn normal() -> Normal {
    Normal::new(0.0, 1.0).expect("the standard normal is a valid distribution")
}

/// The two-sided critical value for a confidence level: `Φ⁻¹((1 + level) / 2)`.
pub fn z_critical(confidence_level: f64) -> f64 {
    normal().inverse_cdf((1.0 + confidence_level) / 2.0)
}

fn t_critical(confidence_level: f64, df: f64) -> f64 {
    StudentsT::new(0.0, 1.0, df)
        .expect("df > 0")
        .inverse_cdf((1.0 + confidence_level) / 2.0)
}

fn normal_cdf(z: f64) -> f64 {
    normal().cdf(z)
}

fn t_cdf(t: f64, df: f64) -> f64 {
    StudentsT::new(0.0, 1.0, df).expect("df > 0").cdf(t)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Sample standard deviation (`ddof = 1`).
fn sample_sd(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let m = mean(values);
    let sum_sq: f64 = values.iter().map(|v| (v - m) * (v - m)).sum();
    (sum_sq / (n - 1.0)).sqrt()
}

/// A linear-interpolated percentile, matching numpy's default (`np.percentile`).
fn percentile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite inputs"));
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (sorted.len() as f64 - 1.0) * (q / 100.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        return sorted[lower];
    }
    let frac = rank - lower as f64;
    sorted[lower] * (1.0 - frac) + sorted[upper] * frac
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).expect("finite inputs"));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// One study in a meta-analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Study {
    pub effect_size: f64,
    pub standard_error: f64,
    pub sample_size: i64,
}

/// The pooled result, matching the old `MetaAnalysisResponse`.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaAnalysis {
    pub pooled_effect: f64,
    pub pooled_se: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub heterogeneity_q: f64,
    pub heterogeneity_i_squared: f64,
    pub study_weights: Vec<f64>,
}

/// Fixed or random effects, by inverse-variance weighting (DerSimonian-Laird for the random arm).
pub fn meta_analysis(
    studies: &[Study],
    random_effects: bool,
    confidence_level: f64,
) -> MetaAnalysis {
    let yi: Vec<f64> = studies.iter().map(|s| s.effect_size).collect();
    let wi: Vec<f64> = studies
        .iter()
        .map(|s| 1.0 / (s.standard_error * s.standard_error))
        .collect();
    let k = yi.len() as f64;

    let sum_w: f64 = wi.iter().sum();
    let sum_wy: f64 = wi.iter().zip(&yi).map(|(w, y)| w * y).sum();
    let pooled_fixed = sum_wy / sum_w;

    let q: f64 = wi
        .iter()
        .zip(&yi)
        .map(|(w, y)| w * (y - pooled_fixed) * (y - pooled_fixed))
        .sum();
    let i_squared = if q > 0.0 {
        ((q - (k - 1.0)) / q).max(0.0)
    } else {
        0.0
    };

    let (pooled, pooled_se, weights_out) = if random_effects {
        let sum_w_sq: f64 = wi.iter().map(|w| w * w).sum();
        let c = sum_w - sum_w_sq / sum_w;
        let tau2 = if c > 0.0 {
            ((q - (k - 1.0)) / c).max(0.0)
        } else {
            0.0
        };
        let wi_star: Vec<f64> = studies
            .iter()
            .map(|s| 1.0 / (s.standard_error * s.standard_error + tau2))
            .collect();
        let sum_star: f64 = wi_star.iter().sum();
        let pooled = wi_star.iter().zip(&yi).map(|(w, y)| w * y).sum::<f64>() / sum_star;
        (pooled, 1.0 / sum_star.sqrt(), wi_star)
    } else {
        (pooled_fixed, 1.0 / sum_w.sqrt(), wi.clone())
    };

    let z = z_critical(confidence_level);
    MetaAnalysis {
        pooled_effect: round6(pooled),
        pooled_se: round6(pooled_se),
        ci_lower: round6(pooled - z * pooled_se),
        ci_upper: round6(pooled + z * pooled_se),
        heterogeneity_q: round4(q),
        heterogeneity_i_squared: round4(i_squared),
        study_weights: weights_out.into_iter().map(round6).collect(),
    }
}

/// Which interval estimator a confidence-interval request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalMethod {
    Bootstrap,
    Normal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceInterval {
    pub mean: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub method: IntervalMethod,
}

/// A SplitMix64 stream. Seeded and local so a bootstrap is reproducible across runs; deliberately
/// not numpy's PCG64, and the tolerance list says so.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

/// The contract's upper bound on `n_bootstrap` (`contract/openapi.lock.json`: 100…100 000).
///
/// The router validates the request against it, but this function is public and a direct caller is
/// not the router: without the clamp, `u32::MAX` would both size an allocation from an untrusted
/// number (a multi-gigabyte `Vec`, CodeQL alert on the PR) and spin 4.3 billion resamples. A bound
/// the contract already declares belongs in the code that does the work, not only in the layer that
/// parses the request.
pub const MAX_BOOTSTRAP: u32 = 100_000;

pub fn confidence_interval(
    values: &[f64],
    confidence_level: f64,
    method: IntervalMethod,
    n_bootstrap: u32,
) -> ConfidenceInterval {
    let m = mean(values);
    let (lower, upper) = match method {
        IntervalMethod::Bootstrap => {
            let n_bootstrap = n_bootstrap.min(MAX_BOOTSTRAP);
            let mut rng = SplitMix64::new(42);
            let mut boot = Vec::with_capacity(n_bootstrap as usize);
            for _ in 0..n_bootstrap {
                let mut total = 0.0;
                for _ in 0..values.len() {
                    total += values[rng.below(values.len())];
                }
                boot.push(total / values.len() as f64);
            }
            let alpha = 1.0 - confidence_level;
            (
                percentile(&boot, 100.0 * alpha / 2.0),
                percentile(&boot, 100.0 * (1.0 - alpha / 2.0)),
            )
        }
        IntervalMethod::Normal => {
            let n = values.len() as f64;
            let se = sample_sd(values) / n.sqrt();
            let t = t_critical(confidence_level, n - 1.0);
            (m - t * se, m + t * se)
        }
    };
    ConfidenceInterval {
        mean: round6(m),
        ci_lower: round6(lower),
        ci_upper: round6(upper),
        method,
    }
}

/// A point in a trend series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeSeriesPoint {
    pub timestamp: f64,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendTest {
    MannKendall,
    LinearRegression,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trend {
    pub trend: &'static str,
    pub p_value: f64,
    pub statistic: f64,
    pub slope: Option<f64>,
    pub intercept: Option<f64>,
}

/// Theil-Sen slope and Mann-Kendall S, exactly as the old `_mann_kendall` computes them.
fn mann_kendall(values: &[f64]) -> (f64, f64, f64, Option<f64>) {
    let n = values.len();
    let mut s: i64 = 0;
    let mut slopes = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n.saturating_sub(1) {
        for j in (i + 1)..n {
            let diff = values[j] - values[i];
            if diff > 0.0 {
                s += 1;
            } else if diff < 0.0 {
                s -= 1;
            }
            slopes.push(diff / (j - i) as f64);
        }
    }

    let n_f = n as f64;
    let var_s = n_f * (n_f - 1.0) * (2.0 * n_f + 5.0) / 18.0;
    let z = if s > 0 {
        (s as f64 - 1.0) / var_s.sqrt()
    } else if s < 0 {
        (s as f64 + 1.0) / var_s.sqrt()
    } else {
        0.0
    };
    let p_value = 2.0 * (1.0 - normal_cdf(z.abs()));
    let tau = if n > 1 {
        2.0 * s as f64 / (n_f * (n_f - 1.0))
    } else {
        0.0
    };
    let slope = if slopes.is_empty() {
        None
    } else {
        Some(median(slopes))
    };
    (p_value, tau, z, slope)
}

/// One of the two tests over the same series.
pub fn trend(points: &[TimeSeriesPoint], test: TrendTest) -> Trend {
    let values: Vec<f64> = points.iter().map(|p| p.value).collect();
    match test {
        TrendTest::MannKendall => {
            let (p_value, tau, _z, slope) = mann_kendall(&values);
            let direction = if p_value < 0.05 {
                if tau > 0.0 {
                    "increasing"
                } else {
                    "decreasing"
                }
            } else {
                "no_trend"
            };
            Trend {
                trend: direction,
                p_value: round6(p_value),
                statistic: round6(tau),
                slope: slope.map(round6),
                intercept: None,
            }
        }
        TrendTest::LinearRegression => {
            let xs: Vec<f64> = points.iter().map(|p| p.timestamp).collect();
            let n = xs.len() as f64;
            let x_bar = mean(&xs);
            let y_bar = mean(&values);
            let sxx: f64 = xs.iter().map(|x| (x - x_bar) * (x - x_bar)).sum();
            let sxy: f64 = xs
                .iter()
                .zip(&values)
                .map(|(x, y)| (x - x_bar) * (y - y_bar))
                .sum();
            let slope = sxy / sxx;
            let intercept = y_bar - slope * x_bar;
            let syy: f64 = values.iter().map(|y| (y - y_bar) * (y - y_bar)).sum();
            let r = sxy / (sxx * syy).sqrt();
            // Two-sided p for the slope: t with n - 2 degrees of freedom.
            let t = r * ((n - 2.0) / (1.0 - r * r)).sqrt();
            let p_value = 2.0 * (1.0 - t_cdf(t.abs(), n - 2.0));
            let direction = if p_value < 0.05 {
                if slope > 0.0 {
                    "increasing"
                } else {
                    "decreasing"
                }
            } else {
                "no_trend"
            };
            Trend {
                trend: direction,
                p_value: round6(p_value),
                statistic: round6(slope),
                slope: Some(round6(slope)),
                intercept: Some(round6(intercept)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clamp is not cosmetic: an unbounded `n_bootstrap` would size an allocation from a
    /// caller-supplied number. The seeded PRNG makes "clamped to the contract maximum" observable
    /// without a timing assertion.
    #[test]
    fn an_unbounded_bootstrap_count_is_clamped_to_the_contract_maximum() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        let at_max = confidence_interval(&values, 0.95, IntervalMethod::Bootstrap, MAX_BOOTSTRAP);
        let past_max = confidence_interval(&values, 0.95, IntervalMethod::Bootstrap, u32::MAX);
        assert_eq!(past_max.ci_lower, at_max.ci_lower);
        assert_eq!(past_max.ci_upper, at_max.ci_upper);
    }

    #[test]
    fn the_standard_normal_quantile_is_where_it_should_be() {
        assert!((z_critical(0.95) - 1.959_963_984_5).abs() < 1e-9);
        assert!((z_critical(0.99) - 2.575_829_303_5).abs() < 1e-9);
    }

    #[test]
    fn a_reference_meta_analysis_matches_a_hand_computation() {
        // Two studies, equal standard errors: the pooled estimate is the plain average and the
        // fixed-effects standard error is se / sqrt(2).
        let studies = vec![
            Study {
                effect_size: 0.5,
                standard_error: 0.25,
                sample_size: 100,
            },
            Study {
                effect_size: 1.0,
                standard_error: 0.25,
                sample_size: 100,
            },
        ];
        let fixed = meta_analysis(&studies, false, 0.95);
        assert!((fixed.pooled_effect - 0.75).abs() < 1e-9);
        assert!((fixed.pooled_se - 0.25 / 2f64.sqrt()).abs() < 1e-6);
        // Q = Σ wi (yi − pooled)² = 2 * 16 * 0.0625 = 2.0
        assert!((fixed.heterogeneity_q - 2.0).abs() < 1e-6);
        // I² = (Q − (k−1)) / Q = (2 − 1) / 2
        assert!((fixed.heterogeneity_i_squared - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_homogeneous_pair_has_zero_heterogeneity() {
        let studies = vec![
            Study {
                effect_size: 0.4,
                standard_error: 0.2,
                sample_size: 50,
            },
            Study {
                effect_size: 0.4,
                standard_error: 0.2,
                sample_size: 50,
            },
        ];
        let fixed = meta_analysis(&studies, false, 0.95);
        assert_eq!(fixed.heterogeneity_q, 0.0);
        assert_eq!(fixed.heterogeneity_i_squared, 0.0);
        assert!((fixed.pooled_effect - 0.4).abs() < 1e-9);
    }

    #[test]
    fn the_normal_interval_contains_the_mean() {
        let ci = confidence_interval(
            &[1.0, 2.0, 3.0, 4.0, 5.0],
            0.95,
            IntervalMethod::Normal,
            1000,
        );
        assert!((ci.mean - 3.0).abs() < 1e-9);
        assert!(ci.ci_lower < 3.0 && ci.ci_upper > 3.0);
    }

    #[test]
    fn the_bootstrap_is_reproducible() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let a = confidence_interval(&values, 0.95, IntervalMethod::Bootstrap, 2000);
        let b = confidence_interval(&values, 0.95, IntervalMethod::Bootstrap, 2000);
        assert_eq!(a, b);
        assert!(a.ci_lower < a.mean && a.mean < a.ci_upper);
    }

    #[test]
    fn a_monotone_series_has_an_increasing_trend() {
        let points: Vec<TimeSeriesPoint> = (0..20)
            .map(|i| TimeSeriesPoint {
                timestamp: i as f64,
                value: i as f64,
            })
            .collect();
        let mk = trend(&points, TrendTest::MannKendall);
        assert_eq!(mk.trend, "increasing");
        assert!(mk.p_value < 0.05);
        assert_eq!(mk.slope, Some(1.0));

        let lr = trend(&points, TrendTest::LinearRegression);
        assert_eq!(lr.trend, "increasing");
        assert_eq!(lr.slope, Some(1.0));
        assert_eq!(lr.intercept, Some(0.0));
    }

    #[test]
    fn a_flat_series_has_no_trend() {
        let points: Vec<TimeSeriesPoint> = (0..20)
            .map(|i| TimeSeriesPoint {
                timestamp: i as f64,
                value: 5.0,
            })
            .collect();
        assert_eq!(trend(&points, TrendTest::MannKendall).trend, "no_trend");
    }
}
