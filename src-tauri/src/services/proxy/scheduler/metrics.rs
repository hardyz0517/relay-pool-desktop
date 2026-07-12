use std::collections::HashMap;
use std::sync::Mutex;

const EWMA_ALPHA: f64 = 0.2;
const EWMA_RETAIN: f64 = 1.0 - EWMA_ALPHA;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeMetricsSnapshot {
    pub error_rate_ewma: f64,
    pub ttft_ewma_ms: Option<f64>,
    pub has_ttft: bool,
}

impl Default for RuntimeMetricsSnapshot {
    fn default() -> Self {
        Self {
            error_rate_ewma: 0.0,
            ttft_ewma_ms: None,
            has_ttft: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct RuntimeMetricsRegistry {
    metrics: Mutex<HashMap<String, RuntimeMetricState>>,
}

impl RuntimeMetricsRegistry {
    pub fn report_result(
        &self,
        key: impl Into<String>,
        success: bool,
        first_token_ms: Option<i64>,
    ) {
        let mut metrics = self
            .metrics
            .lock()
            .expect("runtime metrics registry poisoned");
        let state = metrics.entry(key.into()).or_default();
        let error_sample = if success { 0.0 } else { 1.0 };
        state.error_rate_ewma = Some(match state.error_rate_ewma {
            Some(current) => current * EWMA_RETAIN + error_sample * EWMA_ALPHA,
            None => error_sample,
        });

        if let Some(first_token_ms) = first_token_ms {
            if first_token_ms > 0 {
                let sample = first_token_ms as f64;
                state.ttft_ewma_ms = Some(match state.ttft_ewma_ms {
                    Some(current) => current * EWMA_RETAIN + sample * EWMA_ALPHA,
                    None => sample,
                });
            }
        }
    }

    pub fn snapshot(&self, key: &str) -> RuntimeMetricsSnapshot {
        let metrics = self
            .metrics
            .lock()
            .expect("runtime metrics registry poisoned");
        metrics
            .get(key)
            .map(RuntimeMetricState::snapshot)
            .unwrap_or_default()
    }
}

#[derive(Debug, Default)]
struct RuntimeMetricState {
    error_rate_ewma: Option<f64>,
    ttft_ewma_ms: Option<f64>,
}

impl RuntimeMetricState {
    fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            error_rate_ewma: self.error_rate_ewma.unwrap_or(0.0),
            ttft_ewma_ms: self.ttft_ewma_ms,
            has_ttft: self.ttft_ewma_ms.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_and_ttft_ewma_use_alpha_point_two() {
        let registry = RuntimeMetricsRegistry::default();

        registry.report_result("key-a", false, Some(1000));
        registry.report_result("key-a", true, Some(2000));

        let snapshot = registry.snapshot("key-a");
        assert_approx_eq(snapshot.error_rate_ewma, 0.8);
        assert_eq!(snapshot.ttft_ewma_ms, Some(1200.0));
        assert!(snapshot.has_ttft);
    }

    #[test]
    fn success_only_error_rate_is_zero() {
        let registry = RuntimeMetricsRegistry::default();

        registry.report_result("key-a", true, None);

        let snapshot = registry.snapshot("key-a");
        assert_eq!(snapshot.error_rate_ewma, 0.0);
    }

    #[test]
    fn non_positive_ttft_does_not_set_ttft() {
        let registry = RuntimeMetricsRegistry::default();

        registry.report_result("key-a", true, Some(0));
        registry.report_result("key-a", true, Some(-10));

        let snapshot = registry.snapshot("key-a");
        assert_eq!(snapshot.ttft_ewma_ms, None);
        assert!(!snapshot.has_ttft);
    }

    #[test]
    fn missing_key_snapshot_is_neutral() {
        let registry = RuntimeMetricsRegistry::default();

        let snapshot = registry.snapshot("missing-key");

        assert_eq!(snapshot.error_rate_ewma, 0.0);
        assert_eq!(snapshot.ttft_ewma_ms, None);
        assert!(!snapshot.has_ttft);
    }

    fn assert_approx_eq(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "expected {actual} to equal {expected}"
        );
    }
}
