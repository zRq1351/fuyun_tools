use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AdaptivePollConfig {
    pub min_interval: Duration,
    pub warm_interval: Duration,
    pub idle_interval: Duration,
    pub max_interval: Duration,
    pub report_interval: Duration,
}

#[derive(Clone, Copy)]
enum PollMode {
    Hot,
    Warm,
    Idle,
}

impl PollMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::Warm => "warm",
            Self::Idle => "idle",
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PollMetricsReport {
    pub source: String,
    pub mode: String,
    pub interval_ms: u64,
    pub wakeups: u64,
    pub changes: u64,
    pub busy_skips: u64,
    pub wakeups_per_sec: f64,
    pub change_ratio: f64,
    pub timestamp_ms: u64,
}

pub struct AdaptivePoller {
    cfg: AdaptivePollConfig,
    mode: PollMode,
    current_interval: Duration,
    last_change_at: Instant,
    last_report_at: Instant,
    wakeups: u64,
    changes: u64,
    skipped_busy: u64,
    jitter_seed: u64,
}

impl AdaptivePoller {
    pub fn new(cfg: AdaptivePollConfig) -> Self {
        let now = Instant::now();
        Self {
            cfg,
            mode: PollMode::Warm,
            current_interval: cfg.warm_interval,
            last_change_at: now,
            last_report_at: now,
            wakeups: 0,
            changes: 0,
            skipped_busy: 0,
            jitter_seed: 0x9E3779B97F4A7C15,
        }
    }

    pub fn next_wait(&mut self) -> Duration {
        let base_ms = self.current_interval.as_millis() as i64;
        let now_nanos = Instant::now().elapsed().as_nanos() as u64;
        self.jitter_seed = self
            .jitter_seed
            .wrapping_mul(2862933555777941757)
            .wrapping_add(3037000493)
            ^ now_nanos;
        let pct = (self.jitter_seed % 21) as i64 - 10;
        let mut ms = base_ms + (base_ms * pct / 100);
        let min_ms = self.cfg.min_interval.as_millis() as i64;
        let max_ms = self.cfg.max_interval.as_millis() as i64;
        if ms < min_ms {
            ms = min_ms;
        }
        if ms > max_ms {
            ms = max_ms;
        }
        Duration::from_millis(ms as u64)
    }

    pub fn config(&self) -> AdaptivePollConfig {
        self.cfg
    }

    pub fn reconfigure(&mut self, cfg: AdaptivePollConfig) {
        if self.cfg == cfg {
            return;
        }
        self.cfg = cfg;
        if self.current_interval < self.cfg.min_interval {
            self.current_interval = self.cfg.min_interval;
        }
        if self.current_interval > self.cfg.max_interval {
            self.current_interval = self.cfg.max_interval;
        }
    }

    pub fn mark_change(&mut self) {
        self.wakeups = self.wakeups.saturating_add(1);
        self.changes = self.changes.saturating_add(1);
        self.mode = PollMode::Hot;
        self.current_interval = self.cfg.min_interval;
        self.last_change_at = Instant::now();
    }

    pub fn mark_busy_skip(&mut self) {
        self.wakeups = self.wakeups.saturating_add(1);
        self.skipped_busy = self.skipped_busy.saturating_add(1);
        self.mode = PollMode::Warm;
        self.current_interval = self.cfg.warm_interval;
    }

    pub fn mark_idle(&mut self) {
        self.wakeups = self.wakeups.saturating_add(1);
        let since_change = self.last_change_at.elapsed();
        if since_change <= Duration::from_secs(2) {
            self.mode = PollMode::Warm;
            self.current_interval = self.cfg.warm_interval;
            return;
        }
        if since_change <= Duration::from_secs(20) {
            self.mode = PollMode::Idle;
            let next_ms = (self.current_interval.as_millis() as u64).saturating_mul(2);
            let max_idle = self.cfg.idle_interval.as_millis() as u64;
            let capped = next_ms.min(max_idle).max(self.cfg.warm_interval.as_millis() as u64);
            self.current_interval = Duration::from_millis(capped);
            return;
        }
        self.mode = PollMode::Idle;
        let next_ms = (self.current_interval.as_millis() as u64).saturating_mul(2);
        let capped = next_ms.min(self.cfg.max_interval.as_millis() as u64);
        self.current_interval = Duration::from_millis(capped);
    }

    pub fn metrics_report_if_due(&mut self, name: &str) -> Option<PollMetricsReport> {
        if self.last_report_at.elapsed() < self.cfg.report_interval {
            return None;
        }
        let elapsed_secs = self.last_report_at.elapsed().as_secs_f64();
        let wakeups_per_sec = if elapsed_secs > 0.0 {
            self.wakeups as f64 / elapsed_secs
        } else {
            0.0
        };
        let change_ratio = if self.wakeups > 0 {
            self.changes as f64 / self.wakeups as f64
        } else {
            0.0
        };
        let report = PollMetricsReport {
            source: name.to_string(),
            mode: self.mode.as_str().to_string(),
            interval_ms: self.current_interval.as_millis() as u64,
            wakeups: self.wakeups,
            changes: self.changes,
            busy_skips: self.skipped_busy,
            wakeups_per_sec,
            change_ratio,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };
        self.last_report_at = Instant::now();
        self.wakeups = 0;
        self.changes = 0;
        self.skipped_busy = 0;
        Some(report)
    }

    pub fn metrics_line_if_due(&mut self, name: &str) -> Option<String> {
        self.metrics_report_if_due(name).map(|report| {
            format!(
            "自适应轮询[{}]: mode={}, interval={}ms, wakeups={}, changes={}, busy_skips={}, wakeups_per_sec={:.2}, change_ratio={:.3}",
            report.source,
            report.mode,
            report.interval_ms,
            report.wakeups,
            report.changes,
            report.busy_skips,
            report.wakeups_per_sec,
            report.change_ratio
        )
        })
    }
}
