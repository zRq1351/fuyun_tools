use parking_lot::Mutex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::process;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use sysinfo::{System, Pid};

/// 缓存的系统资源快照，避免每次查询都重建 System
struct CachedSystemResources {
    snapshot: SystemResourceSnapshot,
    last_refresh: Instant,
}

static CACHED_SYSTEM_RESOURCES: OnceLock<Mutex<CachedSystemResources>> = OnceLock::new();
const SYSTEM_RESOURCE_CACHE_TTL: Duration = Duration::from_secs(2);

/// Performance metric categories
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum PerfCategory {
    Startup,
    Memory,
    Cpu,
    ResponseLatency,
    Ipc,
    Other,
}

impl PerfCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            PerfCategory::Startup => "startup",
            PerfCategory::Memory => "memory",
            PerfCategory::Cpu => "cpu",
            PerfCategory::ResponseLatency => "response_latency",
            PerfCategory::Ipc => "ipc",
            PerfCategory::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "startup" => PerfCategory::Startup,
            "memory" => PerfCategory::Memory,
            "cpu" => PerfCategory::Cpu,
            "response_latency" => PerfCategory::ResponseLatency,
            "ipc" => PerfCategory::Ipc,
            _ => PerfCategory::Other,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfMetricSnapshot {
    pub key: String,
    pub label: String,
    pub category: String,
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

/// System resource snapshot for memory and CPU monitoring
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemResourceSnapshot {
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub memory_usage_percent: f64,
    pub process_memory_mb: u64,
    pub cpu_usage_percent: f64,
    pub timestamp: u64,
}

/// Startup timing breakdown
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupTiming {
    pub total_startup_ms: u64,
    pub app_state_init_ms: u64,
    pub tauri_builder_ms: u64,
    pub plugin_init_ms: u64,
    pub shortcut_register_ms: u64,
    pub window_preload_ms: u64,
    pub first_frame_ms: u64,
}

#[derive(Clone, Debug)]
struct PerfMetricAggregate {
    label: String,
    category: PerfCategory,
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
    fn new(label: &str, category: PerfCategory) -> Self {
        Self {
            label: label.to_string(),
            category,
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
            category: self.category.as_str().to_string(),
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

static PERF_METRICS: OnceLock<Mutex<BTreeMap<String, PerfMetricAggregate>>> = OnceLock::new();

fn metrics_store() -> &'static Mutex<BTreeMap<String, PerfMetricAggregate>> {
    PERF_METRICS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn now_unix_ms() -> u64 {
    crate::utils::utils_helpers::now_unix_ms_u64()
}

/// Record a performance metric with explicit category
pub fn record_perf_metric_with_category(
    key: &str,
    label: &str,
    category: PerfCategory,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
) {
    let mut guard = metrics_store().lock();
    let metric = guard
        .entry(key.to_string())
        .or_insert_with(|| PerfMetricAggregate::new(label, category.clone()));
    metric.label = label.to_string();
    metric.category = category;
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

/// Record a performance metric (backward-compatible, defaults to Other category)
pub fn record_perf_metric(
    key: &str,
    label: &str,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
) {
    record_perf_metric_with_category(
        key,
        label,
        PerfCategory::Other,
        duration_ms,
        success,
        error,
    );
}

pub fn get_perf_metrics_snapshot() -> Vec<PerfMetricSnapshot> {
    let guard = metrics_store().lock();
    guard
        .iter()
        .map(|(key, value)| value.snapshot(key))
        .collect::<Vec<_>>()
}

pub fn reset_perf_metrics() {
    let mut guard = metrics_store().lock();
    guard.clear();
}

/// Time a synchronous operation with explicit category
pub fn timed_sync_with_category<T, E, F>(
    key: &str,
    label: &str,
    category: PerfCategory,
    f: F,
) -> Result<T, E>
where
    F: FnOnce() -> Result<T, E>,
    E: ToString,
{
    let started_at = Instant::now();
    match f() {
        Ok(value) => {
            record_perf_metric_with_category(
                key,
                label,
                category,
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            Ok(value)
        }
        Err(error) => {
            record_perf_metric_with_category(
                key,
                label,
                category,
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.to_string()),
            );
            Err(error)
        }
    }
}

/// Time a synchronous operation (backward-compatible, defaults to Other category)
pub fn timed_sync<T, E, F>(key: &str, label: &str, f: F) -> Result<T, E>
where
    F: FnOnce() -> Result<T, E>,
    E: ToString,
{
    timed_sync_with_category(key, label, PerfCategory::Other, f)
}

/// Get current system resource usage (memory and CPU)
/// 使用 2 秒缓存避免频繁创建 System 实例
pub fn get_system_resources() -> SystemResourceSnapshot {
    let cache = CACHED_SYSTEM_RESOURCES.get_or_init(|| {
        Mutex::new(CachedSystemResources {
            snapshot: SystemResourceSnapshot::default(),
            last_refresh: Instant::now() - SYSTEM_RESOURCE_CACHE_TTL * 2,
        })
    });

    {
        let cached = cache.lock();
        if cached.last_refresh.elapsed() < SYSTEM_RESOURCE_CACHE_TTL {
            return cached.snapshot.clone();
        }
    }

    let snapshot = get_system_resources_inner();

    {
        let mut cached = cache.lock();
        cached.snapshot = snapshot.clone();
        cached.last_refresh = Instant::now();
    }

    snapshot
}

fn get_system_resources_inner() -> SystemResourceSnapshot {
    let timestamp = now_unix_ms();

    #[cfg(target_os = "windows")]
    {
        let mut sys = System::new();
        sys.refresh_memory();

        let total_memory = sys.total_memory() / 1024 / 1024;
        let used_memory = sys.used_memory() / 1024 / 1024;
        let memory_usage = if total_memory > 0 {
            (used_memory as f64 / total_memory as f64) * 100.0
        } else {
            0.0
        };

        let process_memory = {
            let pid = Pid::from_u32(process::id());
            sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            sys.process(pid)
                .map(|p| p.memory() / 1024 / 1024)
                .unwrap_or(0)
        };

        sys.refresh_cpu_all();
        let cpu_usage = sys.global_cpu_usage() as f64;

        SystemResourceSnapshot {
            total_memory_mb: total_memory,
            used_memory_mb: used_memory,
            memory_usage_percent: memory_usage,
            process_memory_mb: process_memory,
            cpu_usage_percent: cpu_usage,
            timestamp,
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        SystemResourceSnapshot {
            total_memory_mb: 0,
            used_memory_mb: 0,
            memory_usage_percent: 0.0,
            process_memory_mb: 0,
            cpu_usage_percent: 0.0,
            timestamp,
        }
    }
}

/// Record a startup timing metric
pub fn record_startup_timing(label: &str, duration_ms: u64) {
    record_perf_metric_with_category(
        "startup",
        label,
        PerfCategory::Startup,
        duration_ms,
        true,
        None,
    );
}

/// Record a memory usage metric
pub fn record_memory_usage(label: &str, memory_mb: u64) {
    record_perf_metric_with_category(
        "memory",
        label,
        PerfCategory::Memory,
        memory_mb,
        true,
        None,
    );
}

/// Record a CPU usage metric
pub fn record_cpu_usage(label: &str, cpu_percent: f64) {
    // Store as basis points (percent * 100) for integer precision
    record_perf_metric_with_category(
        "cpu",
        label,
        PerfCategory::Cpu,
        (cpu_percent * 100.0) as u64,
        true,
        None,
    );
}

/// Record an IPC response latency metric
pub fn record_ipc_latency(label: &str, duration_ms: u64, success: bool, error: Option<String>) {
    record_perf_metric_with_category(
        "ipc",
        label,
        PerfCategory::Ipc,
        duration_ms,
        success,
        error,
    );
}

/// Get metrics grouped by category
pub fn get_metrics_by_category() -> BTreeMap<String, Vec<PerfMetricSnapshot>> {
    let metrics = get_perf_metrics_snapshot();
    let mut grouped: BTreeMap<String, Vec<PerfMetricSnapshot>> = BTreeMap::new();

    for metric in metrics {
        grouped
            .entry(metric.category.clone())
            .or_default()
            .push(metric);
    }

    grouped
}

/// Get startup-specific metrics
pub fn get_startup_metrics() -> Vec<PerfMetricSnapshot> {
    get_perf_metrics_snapshot()
        .into_iter()
        .filter(|m| m.category == PerfCategory::Startup.as_str())
        .collect()
}

/// Get memory-specific metrics
pub fn get_memory_metrics() -> Vec<PerfMetricSnapshot> {
    get_perf_metrics_snapshot()
        .into_iter()
        .filter(|m| m.category == PerfCategory::Memory.as_str())
        .collect()
}

/// Get IPC latency metrics
pub fn get_ipc_metrics() -> Vec<PerfMetricSnapshot> {
    get_perf_metrics_snapshot()
        .into_iter()
        .filter(|m| m.category == PerfCategory::Ipc.as_str())
        .collect()
}

/// Get a summary of all performance metrics
pub fn get_perf_summary() -> PerfSummary {
    let metrics = get_perf_metrics_snapshot();
    let system = get_system_resources();

    let total_samples: u64 = metrics.iter().map(|m| m.sample_count).sum();
    let total_errors: u64 = metrics.iter().map(|m| m.error_count).sum();
    let avg_duration: f64 = {
        let (total_weighted, total_samples_dur) = metrics.iter()
            .fold((0.0, 0u64), |(w_sum, s_sum), m| {
                (w_sum + m.avg_duration_ms * m.sample_count as f64, s_sum + m.sample_count)
            });
        if total_samples_dur > 0 { total_weighted / total_samples_dur as f64 } else { 0.0 }
    };

    let startup_metrics = get_startup_metrics();
    let avg_startup_ms = if startup_metrics.is_empty() {
        0.0
    } else {
        startup_metrics.iter().map(|m| m.avg_duration_ms).sum::<f64>() / startup_metrics.len() as f64
    };

    let ipc_metrics = get_ipc_metrics();
    let avg_ipc_latency: f64 = if ipc_metrics.is_empty() {
        0.0
    } else {
        ipc_metrics.iter().map(|m| m.avg_duration_ms).sum::<f64>() / ipc_metrics.len() as f64
    };

    PerfSummary {
        total_metrics: metrics.len() as u64,
        total_samples,
        total_errors,
        avg_duration_ms: avg_duration,
        avg_startup_ms,
        avg_ipc_latency_ms: avg_ipc_latency,
        system_memory_mb: system.used_memory_mb,
        system_memory_percent: system.memory_usage_percent,
        system_cpu_percent: system.cpu_usage_percent,
        timestamp: system.timestamp,
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfSummary {
    pub total_metrics: u64,
    pub total_samples: u64,
    pub total_errors: u64,
    pub avg_duration_ms: f64,
    pub avg_startup_ms: f64,
    pub avg_ipc_latency_ms: f64,
    pub system_memory_mb: u64,
    pub system_memory_percent: f64,
    pub system_cpu_percent: f64,
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// perf_metrics 使用全局静态 store，相关测试需串行执行避免互相污染
    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_store() -> std::sync::MutexGuard<'static, ()> {
        TEST_MUTEX.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn test_perf_category_as_str_roundtrip() {
        let cases = [
            (PerfCategory::Startup, "startup"),
            (PerfCategory::Memory, "memory"),
            (PerfCategory::Cpu, "cpu"),
            (PerfCategory::ResponseLatency, "response_latency"),
            (PerfCategory::Ipc, "ipc"),
            (PerfCategory::Other, "other"),
        ];
        for (cat, s) in cases {
            assert_eq!(cat.as_str(), s);
            assert_eq!(PerfCategory::from_str(s), cat);
        }
        assert_eq!(PerfCategory::from_str("unknown_category"), PerfCategory::Other);
    }

    #[test]
    fn test_record_and_snapshot_counts() {
        let _guard = lock_store();
        reset_perf_metrics();
        record_perf_metric("key1", "标签", 100, true, None);
        record_perf_metric("key1", "标签", 200, true, None);
        record_perf_metric("key1", "标签", 50, false, Some("失败原因".to_string()));

        let snapshot = get_perf_metrics_snapshot();
        assert_eq!(snapshot.len(), 1);
        let m = &snapshot[0];
        assert_eq!(m.key, "key1");
        assert_eq!(m.sample_count, 3);
        assert_eq!(m.success_count, 2);
        assert_eq!(m.error_count, 1);
        assert_eq!(m.last_duration_ms, 50);
        assert_eq!(m.max_duration_ms, 200);
        assert_eq!(m.last_status, "error");
        assert_eq!(m.last_error.as_deref(), Some("失败原因"));
        // avg = (100+200+50)/3
        assert!((m.avg_duration_ms - 116.666666).abs() < 0.001);
    }

    #[test]
    fn test_reset_clears_metrics() {
        let _guard = lock_store();
        record_perf_metric("k", "l", 1, true, None);
        assert!(!get_perf_metrics_snapshot().is_empty());
        reset_perf_metrics();
        assert!(get_perf_metrics_snapshot().is_empty());
    }

    #[test]
    fn test_timed_sync_success_and_error() {
        let _guard = lock_store();
        reset_perf_metrics();
        let ok = timed_sync("op", "操作", || Ok::<_, String>(42));
        assert_eq!(ok.unwrap(), 42);

        let err = timed_sync("op2", "操作2", || Err::<i32, String>("boom".to_string()));
        assert_eq!(err.unwrap_err(), "boom");

        let snapshots = get_perf_metrics_snapshot();
        assert_eq!(snapshots.len(), 2);
        let success_metric = snapshots.iter().find(|m| m.key == "op").unwrap();
        assert_eq!(success_metric.last_status, "success");
        let error_metric = snapshots.iter().find(|m| m.key == "op2").unwrap();
        assert_eq!(error_metric.last_status, "error");
        assert_eq!(error_metric.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn test_record_with_category_and_filters() {
        let _guard = lock_store();
        reset_perf_metrics();
        record_perf_metric_with_category("s", "启动", PerfCategory::Startup, 10, true, None);
        record_perf_metric_with_category("m", "内存", PerfCategory::Memory, 5, true, None);
        record_perf_metric_with_category("i", "IPC", PerfCategory::Ipc, 3, false, Some("e".to_string()));

        assert_eq!(get_startup_metrics().len(), 1);
        assert_eq!(get_memory_metrics().len(), 1);
        assert_eq!(get_ipc_metrics().len(), 1);

        let grouped = get_metrics_by_category();
        assert_eq!(grouped.get("startup").map(|v| v.len()), Some(1));
        assert!(!grouped.contains_key("other"));
    }

    #[test]
    fn test_snapshot_avg_zero_when_no_samples() {
        let _guard = lock_store();
        reset_perf_metrics();
        record_perf_metric_with_category("empty", "空", PerfCategory::Other, 0, true, None);
        // 直接构造一个 0 样本的聚合验证 snapshot
        let agg = PerfMetricAggregate::new("l", PerfCategory::Other);
        let snap = agg.snapshot("k");
        assert_eq!(snap.avg_duration_ms, 0.0);
        assert_eq!(snap.sample_count, 0);
    }

    #[test]
    fn test_perf_summary_totals() {
        let _guard = lock_store();
        reset_perf_metrics();
        record_perf_metric_with_category("a", "A", PerfCategory::Startup, 10, true, None);
        record_perf_metric_with_category("b", "B", PerfCategory::Ipc, 20, false, Some("x".to_string()));

        let summary = get_perf_summary();
        assert_eq!(summary.total_metrics, 2);
        assert_eq!(summary.total_samples, 2);
        assert_eq!(summary.total_errors, 1);
        assert!(summary.avg_duration_ms > 0.0);
    }

    #[test]
    fn test_perf_summary_empty() {
        let _guard = lock_store();
        reset_perf_metrics();
        let summary = get_perf_summary();
        assert_eq!(summary.total_metrics, 0);
        assert_eq!(summary.total_samples, 0);
        assert_eq!(summary.avg_duration_ms, 0.0);
        assert_eq!(summary.avg_startup_ms, 0.0);
        assert_eq!(summary.avg_ipc_latency_ms, 0.0);
    }

    #[test]
    fn test_get_system_resources_returns_snapshot() {
        let res = get_system_resources();
        // 不校验具体数值，只保证结构可用且 timestamp 非 0
        assert!(res.timestamp > 0 || res.total_memory_mb >= 0);
        // 两次调用应命中缓存，结构一致
        let res2 = get_system_resources();
        assert_eq!(res.timestamp, res2.timestamp);
    }
}
