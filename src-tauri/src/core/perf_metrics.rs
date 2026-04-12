use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfMetricSnapshot {
    pub key: String,
    pub label: String,
    pub sample_count: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub last_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u64,
    pub last_status: String,
    pub last_error: Option<String>,
    pub last_recorded_at: u64,
}

#[derive(Clone, Debug)]
struct PerfMetricAggregate {
    label: String,
    sample_count: u64,
    success_count: u64,
    error_count: u64,
    total_duration_ms: u64,
    last_duration_ms: u64,
    max_duration_ms: u64,
    last_status: String,
    last_error: Option<String>,
    last_recorded_at: u64,
}

impl PerfMetricAggregate {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            sample_count: 0,
            success_count: 0,
            error_count: 0,
            total_duration_ms: 0,
            last_duration_ms: 0,
            max_duration_ms: 0,
            last_status: "unknown".to_string(),
            last_error: None,
            last_recorded_at: 0,
        }
    }

    fn snapshot(&self, key: &str) -> PerfMetricSnapshot {
        PerfMetricSnapshot {
            key: key.to_string(),
            label: self.label.clone(),
            sample_count: self.sample_count,
            success_count: self.success_count,
            error_count: self.error_count,
            last_duration_ms: self.last_duration_ms,
            avg_duration_ms: if self.sample_count == 0 {
                0.0
            } else {
                self.total_duration_ms as f64 / self.sample_count as f64
            },
            max_duration_ms: self.max_duration_ms,
            last_status: self.last_status.clone(),
            last_error: self.last_error.clone(),
            last_recorded_at: self.last_recorded_at,
        }
    }
}

static PERF_METRICS: OnceLock<StdMutex<BTreeMap<String, PerfMetricAggregate>>> = OnceLock::new();

fn metrics_store() -> &'static StdMutex<BTreeMap<String, PerfMetricAggregate>> {
    PERF_METRICS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn record_perf_metric(
    key: &str,
    label: &str,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
) {
    let mut guard = metrics_store().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let metric = guard
        .entry(key.to_string())
        .or_insert_with(|| PerfMetricAggregate::new(label));
    metric.label = label.to_string();
    metric.sample_count = metric.sample_count.saturating_add(1);
    metric.total_duration_ms = metric.total_duration_ms.saturating_add(duration_ms);
    metric.last_duration_ms = duration_ms;
    metric.max_duration_ms = metric.max_duration_ms.max(duration_ms);
    metric.last_recorded_at = now_unix_ms();
    if success {
        metric.success_count = metric.success_count.saturating_add(1);
        metric.last_status = "success".to_string();
        metric.last_error = None;
    } else {
        metric.error_count = metric.error_count.saturating_add(1);
        metric.last_status = "error".to_string();
        metric.last_error = error;
    }
}

pub fn get_perf_metrics_snapshot() -> Vec<PerfMetricSnapshot> {
    let guard = metrics_store().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .iter()
        .map(|(key, value)| value.snapshot(key))
        .collect::<Vec<_>>()
}

pub fn reset_perf_metrics() {
    let mut guard = metrics_store().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clear();
}

pub fn timed_sync<T, E, F>(key: &str, label: &str, f: F) -> Result<T, E>
where
    F: FnOnce() -> Result<T, E>,
    E: ToString,
{
    let started_at = Instant::now();
    match f() {
        Ok(value) => {
            record_perf_metric(key, label, started_at.elapsed().as_millis() as u64, true, None);
            Ok(value)
        }
        Err(error) => {
            record_perf_metric(
                key,
                label,
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.to_string()),
            );
            Err(error)
        }
    }
}
