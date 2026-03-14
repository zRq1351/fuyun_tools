use crate::services::adaptive_poll::PollMetricsReport;
use crate::utils::utils_helpers::get_poll_metrics_file_path;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

const MAX_METRICS_POINTS: usize = 720;

lazy_static! {
    static ref METRICS_STORE: Mutex<Vec<PollMetricsReport>> = Mutex::new(load_metrics());
}

fn load_metrics() -> Vec<PollMetricsReport> {
    let path = get_poll_metrics_file_path();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<Vec<PollMetricsReport>>(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_metrics(metrics: &[PollMetricsReport]) {
    let path = get_poll_metrics_file_path();
    if let Ok(text) = serde_json::to_string(metrics) {
        let _ = fs::write(path, text);
    }
}

pub fn record(report: PollMetricsReport) {
    if let Ok(mut guard) = METRICS_STORE.lock() {
        guard.push(report);
        if guard.len() > MAX_METRICS_POINTS {
            let remove_count = guard.len().saturating_sub(MAX_METRICS_POINTS);
            guard.drain(0..remove_count);
        }
        save_metrics(&guard);
    }
}

pub fn list(limit: usize) -> Vec<PollMetricsReport> {
    if let Ok(guard) = METRICS_STORE.lock() {
        let size = guard.len();
        let take = limit.min(size);
        return guard[size - take..size].to_vec();
    }
    Vec::new()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PollMetricsMinuteAggregate {
    pub minute_epoch_ms: u64,
    pub source: String,
    pub samples: u64,
    pub wakeups_total: u64,
    pub changes_total: u64,
    pub busy_skips_total: u64,
    pub wakeups_per_sec_avg: f64,
    pub change_ratio_avg: f64,
    pub hit_bucket_low: u64,
    pub hit_bucket_mid: u64,
    pub hit_bucket_high: u64,
}

pub fn aggregate_by_minute(limit_minutes: usize) -> Vec<PollMetricsMinuteAggregate> {
    let points = if let Ok(guard) = METRICS_STORE.lock() {
        guard.clone()
    } else {
        Vec::new()
    };
    let mut grouped: BTreeMap<(u64, String), PollMetricsMinuteAggregate> = BTreeMap::new();
    for p in points {
        let minute = (p.timestamp_ms / 60_000) * 60_000;
        let key = (minute, p.source.clone());
        let entry = grouped
            .entry(key)
            .or_insert_with(|| PollMetricsMinuteAggregate {
                minute_epoch_ms: minute,
                source: p.source.clone(),
                samples: 0,
                wakeups_total: 0,
                changes_total: 0,
                busy_skips_total: 0,
                wakeups_per_sec_avg: 0.0,
                change_ratio_avg: 0.0,
                hit_bucket_low: 0,
                hit_bucket_mid: 0,
                hit_bucket_high: 0,
            });
        entry.samples += 1;
        entry.wakeups_total = entry.wakeups_total.saturating_add(p.wakeups);
        entry.changes_total = entry.changes_total.saturating_add(p.changes);
        entry.busy_skips_total = entry.busy_skips_total.saturating_add(p.busy_skips);
        entry.wakeups_per_sec_avg += p.wakeups_per_sec;
        entry.change_ratio_avg += p.change_ratio;
        if p.change_ratio < 0.1 {
            entry.hit_bucket_low += 1;
        } else if p.change_ratio < 0.4 {
            entry.hit_bucket_mid += 1;
        } else {
            entry.hit_bucket_high += 1;
        }
    }
    let mut values: Vec<PollMetricsMinuteAggregate> = grouped
        .into_values()
        .map(|mut v| {
            if v.samples > 0 {
                let s = v.samples as f64;
                v.wakeups_per_sec_avg /= s;
                v.change_ratio_avg /= s;
            }
            v
        })
        .collect();
    values.sort_by_key(|v| v.minute_epoch_ms);
    if values.is_empty() {
        return values;
    }
    let keep = limit_minutes.max(1).saturating_mul(2);
    if values.len() > keep {
        values.split_off(values.len() - keep)
    } else {
        values
    }
}

pub fn export_json(limit: usize) -> Result<String, String> {
    let rows = list(limit);
    #[derive(Serialize)]
    struct ExportRow {
        timestamp_ms: String,
        source: String,
        mode: String,
        interval_ms: u64,
        wakeups: u64,
        changes: u64,
        busy_skips: u64,
        wakeups_per_sec: f64,
        change_ratio: f64,
    }
    let mapped: Vec<ExportRow> = rows
        .into_iter()
        .map(|r| ExportRow {
            timestamp_ms: format_timestamp_ms(r.timestamp_ms),
            source: r.source,
            mode: r.mode,
            interval_ms: r.interval_ms,
            wakeups: r.wakeups,
            changes: r.changes,
            busy_skips: r.busy_skips,
            wakeups_per_sec: r.wakeups_per_sec,
            change_ratio: r.change_ratio,
        })
        .collect();
    serde_json::to_string_pretty(&mapped).map_err(|e| e.to_string())
}

pub fn export_csv(limit: usize) -> String {
    let rows = list(limit);
    let mut out = String::new();
    out.push_str("字段说明,含义\n");
    out.push_str("timestamp_ms,采样时间（格式：YYYY-MM-DD HH:MM:SS.mmm）\n");
    out.push_str("source,监听来源（text=文本剪贴板；image=图片剪贴板）\n");
    out.push_str("mode,轮询状态（hot/warm/idle）\n");
    out.push_str("interval_ms,当前轮询间隔（毫秒）\n");
    out.push_str("wakeups,该采样周期内监听唤醒次数\n");
    out.push_str("changes,该采样周期内检测到变化的次数\n");
    out.push_str("busy_skips,该采样周期内因忙状态跳过处理的次数\n");
    out.push_str("wakeups_per_sec,平均每秒唤醒次数\n");
    out.push_str("change_ratio,变化命中率（changes / wakeups）\n");
    out.push('\n');
    out.push_str(
        "timestamp_ms,source,mode,interval_ms,wakeups,changes,busy_skips,wakeups_per_sec,change_ratio\n",
    );
    for r in rows {
        let line = format!(
            "{},{},{},{},{},{},{},{:.4},{:.4}\n",
            format_timestamp_ms(r.timestamp_ms),
            r.source,
            r.mode,
            r.interval_ms,
            r.wakeups,
            r.changes,
            r.busy_skips,
            r.wakeups_per_sec,
            r.change_ratio
        );
        out.push_str(&line);
    }
    out
}

fn format_timestamp_ms(timestamp_ms: u64) -> String {
    let total_secs = (timestamp_ms / 1000) as i64;
    let millis = (timestamp_ms % 1000) as u32;
    let days = total_secs.div_euclid(86_400);
    let secs_of_day = total_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let second = (secs_of_day % 60) as u32;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        year, month, day, hour, minute, second, millis
    )
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}
