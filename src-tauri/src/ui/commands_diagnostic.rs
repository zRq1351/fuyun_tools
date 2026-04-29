use crate::core::app_state::AppState as SharedAppState;
use crate::core::perf_metrics::{get_perf_metrics_snapshot, reset_perf_metrics};
use crate::sync::Mutex;
use crate::ui::commands::{
    get_copy_paste_dedup_debug_state_value, now_unix_ms,
    COPY_PASTE_DEDUP_TOTAL_REQUESTS, COPY_PASTE_DEDUP_HIT_COUNT,
    COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT, COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT,
    COPY_PASTE_DEDUP_LOG_COUNT, COPY_PASTE_DEDUP_WINDOW_STATS,
};
use crate::ui::commands_backup::detect_video_hw_accel_encoder;
use crate::ui::commands_clipboard::lock_arc_mutex;
use crate::ui::commands_vc_runtime::check_vc_runtime_dependencies;
use crate::ui::commands_writeback::{last_writeback_result, perf_metric_group_label, perf_metric_group_rank, perf_metric_is_slow};
use crate::utils::image_clipboard::get_image_persist_queue_metrics_snapshot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotAvailability {
    pub status: String,
    pub phase: String,
    pub summary: String,
    pub details: Vec<String>,
    pub session_id: Option<u64>,
    pub recent_failure_kind: Option<String>,
    pub recent_failure_message: Option<String>,
    pub recent_failure_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticAction {
    pub key: String,
    pub label: String,
    pub action_type: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub key: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    pub details: Vec<String>,
    pub actions: Vec<DiagnosticAction>,
    pub last_checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticOverview {
    pub overall_status: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticActionResult {
    pub success: bool,
    pub action_key: String,
    pub message: String,
    pub needs_refresh: bool,
    pub should_restart: bool,
    pub navigate_to: Option<String>,
    pub external_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticActionRequest {
    pub action_key: String,
}

pub(crate) async fn build_diagnostic_items_inner(
    state: &Arc<Mutex<SharedAppState>>,
) -> Result<Vec<DiagnosticItem>, String> {
    let checked_at = now_unix_ms() as i64;
    let (
        settings,
        image_manager_arc,
        active_overlay_window,
        last_overlay_lifecycle,
        overlay_lifecycle_history,
    ) = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        (
            guard.settings.clone(),
            guard.image_clipboard_manager.clone(),
            guard.active_overlay_window.clone(),
            guard.last_overlay_lifecycle.clone(),
            guard.overlay_lifecycle_history.clone(),
        )
    };
    let storage_metrics = {
        let manager = lock_arc_mutex(&image_manager_arc);
        manager.get_storage_metrics()
    };
    let queue_metrics = get_image_persist_queue_metrics_snapshot();
    let mut perf_metrics = get_perf_metrics_snapshot();
    let dedup_state = get_copy_paste_dedup_debug_state_value();
    let vc_runtime = check_vc_runtime_dependencies().await?;
    let longshot = get_manual_longshot_availability().await?;

    let memory_ratio = if storage_metrics.memory_budget_bytes == 0 {
        0.0
    } else {
        storage_metrics.memory_bytes as f64 / storage_metrics.memory_budget_bytes as f64
    };
    let disk_ratio = if storage_metrics.disk_limit_bytes == 0 {
        0.0
    } else {
        storage_metrics.disk_bytes as f64 / storage_metrics.disk_limit_bytes as f64
    };
    let image_storage_status = if memory_ratio >= 1.0 || disk_ratio >= 1.0 {
        "error"
    } else if memory_ratio >= 0.8 || disk_ratio >= 0.8 {
        "warning"
    } else {
        "healthy"
    };

    let persist_status = if queue_metrics.timeout_drop_count > 0 || queue_metrics.full_count > 20 {
        "error"
    } else if queue_metrics.full_count > 0 {
        "warning"
    } else {
        "healthy"
    };

    let vc_ok = vc_runtime
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let vc_missing = vc_runtime
        .get("missing")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let dependency_status = if vc_ok { "healthy" } else { "error" };
    let dependency_summary = if vc_ok {
        "运行依赖检查正常".to_string()
    } else {
        format!("VC Runtime 缺失 {} 项依赖", vc_missing.len())
    };

    let recording_status = if settings.dev_force_ffmpeg_window_capture {
        "warning"
    } else {
        "healthy"
    };
    let recording_summary = if settings.dev_force_ffmpeg_window_capture {
        "当前录屏处于强制 FFmpeg 降级模式".to_string()
    } else {
        "当前录屏主链路未强制降级".to_string()
    };

    // 🔧 性能优化：检测视频硬件加速编码器
    let hw_encoder_info =
        if let Ok(ffmpeg_path) = crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path() {
            detect_video_hw_accel_encoder(&ffmpeg_path)
        } else {
            None
        };
    perf_metrics.sort_by(|a, b| {
        b.avg_duration_ms
            .partial_cmp(&a.avg_duration_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    let perf_error_items = perf_metrics
        .iter()
        .filter(|item| item.last_status == "error")
        .collect::<Vec<_>>();
    let perf_slow_items = perf_metrics
        .iter()
        .filter(|item| perf_metric_is_slow(item))
        .collect::<Vec<_>>();
    let perf_status = if perf_metrics.is_empty() {
        "unknown"
    } else if !perf_error_items.is_empty() {
        "warning"
    } else if !perf_slow_items.is_empty() {
        "warning"
    } else {
        "healthy"
    };
    let perf_summary = if let Some(item) = perf_metrics.first() {
        format!(
            "已采样 {} 条链路，慢项 {} 条，异常 {} 条，当前平均最慢项 {} {:.0} ms",
            perf_metrics.len(),
            perf_slow_items.len(),
            perf_error_items.len(),
            item.label,
            item.avg_duration_ms
        )
    } else {
        "尚无性能采样记录，触发 OCR、AI 或截图保存后会出现数据".to_string()
    };
    let mut perf_grouped: BTreeMap<String, Vec<&crate::core::perf_metrics::PerfMetricSnapshot>> =
        BTreeMap::new();
    for item in &perf_metrics {
        perf_grouped
            .entry(perf_metric_group_label(&item.key).to_string())
            .or_default()
            .push(item);
    }
    let mut perf_group_summaries = perf_grouped.into_iter().collect::<Vec<_>>();
    perf_group_summaries.sort_by(|(left, _), (right, _)| {
        perf_metric_group_rank(left)
            .cmp(&perf_metric_group_rank(right))
            .then_with(|| left.cmp(right))
    });
    let mut perf_details = Vec::new();
    if !perf_error_items.is_empty() {
        perf_details.extend(perf_error_items.iter().take(3).map(|item| {
            format!(
                "[最近异常] {}: last {} ms / error {}",
                item.label,
                item.last_duration_ms,
                item.last_error
                    .clone()
                    .unwrap_or_else(|| "未知错误".to_string())
            )
        }));
    } else {
        perf_details.push("最近异常: 无".to_string());
    }
    if !perf_slow_items.is_empty() {
        perf_details.extend(perf_slow_items.iter().take(4).map(|item| {
            format!(
                "[慢项] {}: avg {:.0} ms / max {} ms / samples {}",
                item.label, item.avg_duration_ms, item.max_duration_ms, item.sample_count
            )
        }));
    } else if !perf_metrics.is_empty() {
        perf_details.push("慢项提示: 当前没有超阈值链路".to_string());
    }
    perf_details.extend(perf_group_summaries.into_iter().map(|(group, items)| {
        let slow_count = items
            .iter()
            .filter(|item| perf_metric_is_slow(item))
            .count();
        let error_count = items
            .iter()
            .filter(|item| item.last_status == "error")
            .count();
        let slowest = items
            .iter()
            .max_by(|left, right| {
                left.avg_duration_ms
                    .partial_cmp(&right.avg_duration_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied();
        match slowest {
            Some(item) => format!(
                "[分组] {}: {} 条 / 慢项 {} / 异常 {} / 最慢 {} {:.0} ms",
                group,
                items.len(),
                slow_count,
                error_count,
                item.label,
                item.avg_duration_ms
            ),
            None => format!("[分组] {}: 0 条", group),
        }
    }));

    let window_hit_rate = dedup_state
        .get("metrics")
        .and_then(|value| value.get("windowHitRate"))
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let dedup_enabled = dedup_state
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let dedup_status = if !dedup_enabled {
        "warning"
    } else if window_hit_rate > 0.8 {
        "warning"
    } else {
        "healthy"
    };
    let dedup_summary = if !dedup_enabled {
        "回写去重已关闭".to_string()
    } else {
        format!("当前命中率 {:.1}%", window_hit_rate * 100.0)
    };
    let last_writeback = last_writeback_result();
    let writeback_status = match last_writeback.as_ref() {
        Some(item) if item.success => "healthy",
        Some(_) => "warning",
        None => "unknown",
    };
    let writeback_summary = match last_writeback.as_ref() {
        Some(item) if item.success => format!(
            "{} 最近一次回写成功{}",
            item.source,
            if item.target_window_title.is_empty() {
                String::new()
            } else {
                format!("，目标窗口 {}", item.target_window_title)
            }
        ),
        Some(item) => format!("{} 最近一次回写失败：{}", item.source, item.detail),
        None => "最近还没有回写执行记录".to_string(),
    };

    let longshot_status = match longshot.status.as_str() {
        "available" => "healthy",
        "busy" => "warning",
        "unavailable_missing_dependency" | "unavailable_runtime_error" => "error",
        _ => "unknown",
    };

    let mut longshot_details = longshot.details.clone();
    longshot_details.push(format!("当前阶段: {}", longshot.phase));
    if let Some(kind) = longshot.recent_failure_kind.as_ref() {
        longshot_details.push(format!("最近失败类型: {}", kind));
    }
    if let Some(message) = longshot.recent_failure_message.as_ref() {
        longshot_details.push(format!("最近失败原因: {}", message));
    }
    if let Some(at) = longshot.recent_failure_at {
        longshot_details.push(format!("最近失败时间: {}", at));
    }
    if longshot.status == "unavailable_missing_dependency" {
        longshot_details.push(
            "修复建议: 先确认 FFmpeg 可执行文件可用，再检查 longshot-opencv 构建能力".to_string(),
        );
    } else if longshot.status == "busy" {
        longshot_details.push("修复建议: 完成或取消当前长截图会话后再重试".to_string());
    } else if longshot.recent_failure_kind.as_deref() == Some("runtime_error") {
        longshot_details
            .push("修复建议: 重新开始一次长截图，若仍失败请打开诊断并检查最近失败原因".to_string());
    }

    let mut longshot_actions = vec![
        DiagnosticAction {
            key: "diagnostic.refresh".to_string(),
            label: "重新检查".to_string(),
            action_type: "refresh".to_string(),
            target: None,
        },
        DiagnosticAction {
            key: "longshot.open-settings".to_string(),
            label: "查看截图设置".to_string(),
            action_type: "open_settings".to_string(),
            target: Some("screenshot".to_string()),
        },
    ];
    if longshot.status == "unavailable_missing_dependency" {
        longshot_actions.push(DiagnosticAction {
            key: "longshot.show-help".to_string(),
            label: "查看修复说明".to_string(),
            action_type: "show_help".to_string(),
            target: None,
        });
        longshot_actions.push(DiagnosticAction {
            key: "longshot.download-ffmpeg".to_string(),
            label: "下载 FFmpeg".to_string(),
            action_type: "open_external".to_string(),
            target: None,
        });
        longshot_actions.push(DiagnosticAction {
            key: "longshot.show-build-help".to_string(),
            label: "查看构建要求".to_string(),
            action_type: "show_help".to_string(),
            target: None,
        });
    } else if longshot.recent_failure_kind.as_deref() == Some("runtime_error") {
        longshot_actions.push(DiagnosticAction {
            key: "longshot.show-runtime-help".to_string(),
            label: "查看失败说明".to_string(),
            action_type: "show_help".to_string(),
            target: None,
        });
    }

    Ok(vec![
        DiagnosticItem {
            key: "image-storage".to_string(),
            title: "图片存储占用".to_string(),
            status: image_storage_status.to_string(),
            summary: format!(
                "当前 {} 张图片，磁盘 {:.0}% / 内存 {:.0}%",
                storage_metrics.item_count,
                disk_ratio * 100.0,
                memory_ratio * 100.0
            ),
            details: vec![
                format!(
                    "磁盘占用 {} / {} 字节",
                    storage_metrics.disk_bytes, storage_metrics.disk_limit_bytes
                ),
                format!(
                    "内存占用 {} / {} 字节",
                    storage_metrics.memory_bytes, storage_metrics.memory_budget_bytes
                ),
                format!("置顶图片 {} 张", storage_metrics.pinned_count),
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "image-storage.open-settings".to_string(),
                    label: "打开设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("clipboard".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "image-persist-queue".to_string(),
            title: "图片持久化队列".to_string(),
            status: persist_status.to_string(),
            summary: format!(
                "队列容量 {}，满队 {} 次，超时丢弃 {} 次",
                queue_metrics.queue_size,
                queue_metrics.full_count,
                queue_metrics.timeout_drop_count
            ),
            details: vec![
                format!("发送超时 {} ms", queue_metrics.send_timeout_ms),
                format!("重试间隔 {} ms", queue_metrics.retry_interval_ms),
                format!("平均等待 {:.1} ms", queue_metrics.avg_wait_ms),
            ],
            actions: vec![DiagnosticAction {
                key: "diagnostic.refresh".to_string(),
                label: "刷新".to_string(),
                action_type: "refresh".to_string(),
                target: None,
            }],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "dependencies".to_string(),
            title: "依赖检查状态".to_string(),
            status: dependency_status.to_string(),
            summary: dependency_summary,
            details: vec![
                format!("VC Runtime: {}", if vc_ok { "已就绪" } else { "缺失" }),
                format!(
                    "FFmpeg: {}",
                    if crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path().is_ok() {
                        "已就绪"
                    } else {
                        "未检测到"
                    }
                ),
                if vc_missing.is_empty() {
                    "无缺失的 VC Runtime 组件".to_string()
                } else {
                    format!(
                        "缺失组件: {}",
                        vc_missing
                            .into_iter()
                            .filter_map(|value| value.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "重新检查".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "dependencies.download-vc-runtime".to_string(),
                    label: "下载依赖".to_string(),
                    action_type: "download_dependency".to_string(),
                    target: vc_runtime
                        .get("installUrl")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "recording-degrade".to_string(),
            title: "录屏降级状态".to_string(),
            status: recording_status.to_string(),
            summary: recording_summary,
            details: vec![
                format!(
                    "强制 FFmpeg 降级: {}",
                    if settings.dev_force_ffmpeg_window_capture {
                        "已开启"
                    } else {
                        "未开启"
                    }
                ),
                format!(
                    "录屏开关: {}",
                    if settings.recording_enabled {
                        "已启用"
                    } else {
                        "未启用"
                    }
                ),
                format!(
                    "视频硬件加速: {}",
                    match &hw_encoder_info {
                        Some(encoder) => format!("已检测到 {}", encoder),
                        None => "未检测到（使用软件编码）".to_string(),
                    }
                ),
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "recording-degrade.open-settings".to_string(),
                    label: "打开录屏设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("recording".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "performance-metrics".to_string(),
            title: "关键链路性能观测".to_string(),
            status: perf_status.to_string(),
            summary: perf_summary,
            details: if perf_metrics.is_empty() {
                vec![
                    "当前还没有运行时性能采样".to_string(),
                    "可先触发 OCR、AI 翻译/解释、截图保存等链路".to_string(),
                ]
            } else {
                perf_details
            },
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "perf-metrics.reset".to_string(),
                    label: "清零采样".to_string(),
                    action_type: "reset_metrics".to_string(),
                    target: None,
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "overlay-window".to_string(),
            title: "覆盖层窗口状态".to_string(),
            status: if active_overlay_window.is_some() {
                "warning"
            } else {
                "healthy"
            }
                .to_string(),
            summary: match active_overlay_window.as_deref() {
                Some(label) => format!("当前活动覆盖层窗口: {}", label),
                None => "当前没有活动覆盖层窗口".to_string(),
            },
            details: vec![
                "用于观察工具栏、剪贴板窗、结果窗、预览窗的生命周期一致性".to_string(),
                match active_overlay_window.as_deref() {
                    Some(label) => format!("活动窗口标签: {}", label),
                    None => "活动窗口标签: 无".to_string(),
                },
                match last_overlay_lifecycle.as_ref() {
                    Some(item) => format!(
                        "最近动作: {} -> {} (focused={}, at={})",
                        item.label, item.action, item.focused, item.occurred_at
                    ),
                    None => "最近动作: 无".to_string(),
                },
            ]
                .into_iter()
                .chain(overlay_lifecycle_history.iter().rev().take(5).map(|item| {
                    format!(
                        "历史: {} -> {} (focused={}, at={})",
                        item.label, item.action, item.focused, item.occurred_at
                    )
                }))
                .collect(),
            actions: vec![DiagnosticAction {
                key: "diagnostic.refresh".to_string(),
                label: "刷新".to_string(),
                action_type: "refresh".to_string(),
                target: None,
            }],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "copy-paste-dedup".to_string(),
            title: "回写去重状态".to_string(),
            status: dedup_status.to_string(),
            summary: dedup_summary,
            details: vec![
                format!(
                    "总请求数 {}",
                    dedup_state["metrics"]["totalRequests"]
                        .as_u64()
                        .unwrap_or(0)
                ),
                format!(
                    "命中总数 {}",
                    dedup_state["metrics"]["hitCount"].as_u64().unwrap_or(0)
                ),
                format!(
                    "时间窗口 {} ms",
                    dedup_state["windowMs"].as_u64().unwrap_or(0)
                ),
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "copy-paste-dedup.reset-metrics".to_string(),
                    label: "清零计数".to_string(),
                    action_type: "reset_metrics".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "copy-paste-dedup.open-settings".to_string(),
                    label: "调整设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("selection".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "writeback-flow".to_string(),
            title: "回写链路状态".to_string(),
            status: writeback_status.to_string(),
            summary: writeback_summary,
            details: match last_writeback.as_ref() {
                Some(item) => vec![
                    format!("来源 {}", item.source),
                    format!("阶段 {}", item.stage),
                    format!(
                        "目标窗口 {}",
                        if item.target_window_title.is_empty() {
                            "未知".to_string()
                        } else {
                            item.target_window_title.clone()
                        }
                    ),
                    format!("目标进程 PID {}", item.target_window_pid),
                ],
                None => vec![
                    "尚无最近回写结果".to_string(),
                    "可通过文字历史、图片历史或结果窗触发一次回写".to_string(),
                ],
            },
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "writeback-flow.open-settings".to_string(),
                    label: "查看划词设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("selection".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "longshot".to_string(),
            title: "长截图可用性状态".to_string(),
            status: longshot_status.to_string(),
            summary: longshot.summary,
            details: longshot_details,
            actions: longshot_actions,
            last_checked_at: checked_at,
        },
    ])
}

#[tauri::command]
pub async fn get_manual_longshot_availability() -> Result<ManualLongshotAvailability, String> {
    #[cfg(not(feature = "longshot-opencv"))]
    {
        return Ok(ManualLongshotAvailability {
            status: "unavailable_missing_dependency".to_string(),
            phase: "idle".to_string(),
            summary: "当前构建未启用长截图依赖".to_string(),
            details: vec![
                "需要启用 longshot-opencv feature".to_string(),
                "默认构建未携带 OpenCV 长截图能力".to_string(),
                "该问题属于构建能力缺失，无法通过当前运行时自动修复".to_string(),
            ],
            session_id: None,
            recent_failure_kind: None,
            recent_failure_message: None,
            recent_failure_at: None,
        });
    }

    #[cfg(feature = "longshot-opencv")]
    {
        let recent_failure =
            crate::features::screenshot::longshot::get_last_manual_longshot_failure();
        let session_id = crate::features::screenshot::longshot::active_manual_longshot_session_id();
        if let Some(session_id) = session_id {
            let status =
                crate::features::screenshot::longshot::get_manual_longshot_status(session_id)
                    .map_err(|e| format!("读取长截图状态失败: {}", e))?;
            return Ok(ManualLongshotAvailability {
                status: "busy".to_string(),
                phase: status.phase.clone(),
                summary: status.user_message,
                details: vec![
                    format!("当前阶段: {}", status.phase),
                    "请先完成或取消当前长截图会话".to_string(),
                ],
                session_id: Some(session_id),
                recent_failure_kind: recent_failure
                    .as_ref()
                    .map(|item| item.failure_kind.clone()),
                recent_failure_message: recent_failure.as_ref().map(|item| item.message.clone()),
                recent_failure_at: recent_failure.as_ref().map(|item| item.occurred_at),
            });
        }
        match crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path() {
            Ok(path) => Ok(ManualLongshotAvailability {
                status: "available".to_string(),
                phase: "idle".to_string(),
                summary: "长截图当前可用".to_string(),
                details: vec![
                    format!("已检测到 FFmpeg: {}", path.display()),
                    "当前构建已启用 longshot-opencv feature".to_string(),
                ],
                session_id: None,
                recent_failure_kind: recent_failure
                    .as_ref()
                    .map(|item| item.failure_kind.clone()),
                recent_failure_message: recent_failure.as_ref().map(|item| item.message.clone()),
                recent_failure_at: recent_failure.as_ref().map(|item| item.occurred_at),
            }),
            Err(err) => Ok(ManualLongshotAvailability {
                status: "unavailable_missing_dependency".to_string(),
                phase: "idle".to_string(),
                summary: "长截图依赖未就绪".to_string(),
                details: vec![
                    err,
                    "请先确保 FFmpeg 可执行文件可用，再重新检查".to_string(),
                    "若当前构建未携带 longshot-opencv feature，也需要切换到支持长截图的构建"
                        .to_string(),
                ],
                session_id: None,
                recent_failure_kind: recent_failure
                    .as_ref()
                    .map(|item| item.failure_kind.clone()),
                recent_failure_message: recent_failure.as_ref().map(|item| item.message.clone()),
                recent_failure_at: recent_failure.as_ref().map(|item| item.occurred_at),
            }),
        }
    }
}

#[tauri::command]
pub async fn get_diagnostic_items(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<DiagnosticItem>, String> {
    build_diagnostic_items_inner(state.inner()).await
}

#[tauri::command]
pub async fn get_diagnostic_overview(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<DiagnosticOverview, String> {
    let items = build_diagnostic_items_inner(state.inner()).await?;
    let error_count = items.iter().filter(|item| item.status == "error").count();
    let warning_count = items.iter().filter(|item| item.status == "warning").count();
    let overall_status = if error_count > 0 {
        "error"
    } else if warning_count > 0 {
        "warning"
    } else {
        "healthy"
    };
    Ok(DiagnosticOverview {
        overall_status: overall_status.to_string(),
        error_count,
        warning_count,
        checked_at: now_unix_ms() as i64,
    })
}

#[tauri::command]
pub async fn run_diagnostic_action(
    request: DiagnosticActionRequest,
) -> Result<DiagnosticActionResult, String> {
    match request.action_key.as_str() {
        "diagnostic.refresh" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "诊断状态已刷新".to_string(),
            needs_refresh: true,
            should_restart: false,
            navigate_to: None,
            external_url: None,
        }),
        "image-storage.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查剪贴板设置中的图片容量限制".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("clipboard".to_string()),
            external_url: None,
        }),
        "recording-degrade.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查录屏设置与依赖状态".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("recording".to_string()),
            external_url: None,
        }),
        "copy-paste-dedup.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查划词与回写设置".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("selection".to_string()),
            external_url: None,
        }),
        "longshot.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查截图设置与长截图能力".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("screenshot".to_string()),
            external_url: None,
        }),
        "longshot.show-help" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "长截图依赖修复建议：1) 先下载并配置 FFmpeg；2) 确认 ffmpeg 可在命令行直接执行；3) 若仍不可用，检查当前构建是否启用 longshot-opencv feature；4) 回到诊断页重新检查。".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("diagnostic".to_string()),
            external_url: None,
        }),
        "longshot.download-ffmpeg" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "已准备 FFmpeg 下载页".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: None,
            external_url: Some("https://ffmpeg.org/download.html".to_string()),
        }),
        "longshot.show-build-help" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "当前长截图除 FFmpeg 外，还要求构建启用 longshot-opencv feature。若诊断仍提示构建未启用，只能切换到支持长截图的构建产物。".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("diagnostic".to_string()),
            external_url: None,
        }),
        "longshot.show-runtime-help" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "长截图运行失败建议：重新开始一次长截图；若仍失败，优先检查滚动区域大小、依赖环境与最近失败原因。".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("diagnostic".to_string()),
            external_url: None,
        }),
        "dependencies.download-vc-runtime" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "已准备 VC Runtime 下载链接".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: None,
            external_url: Some("https://aka.ms/vs/17/release/vc_redist.x64.exe".to_string()),
        }),
        "copy-paste-dedup.reset-metrics" => {
            COPY_PASTE_DEDUP_TOTAL_REQUESTS.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_HIT_COUNT.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_LOG_COUNT.store(0, Ordering::Relaxed);
            if let Some(lock) = COPY_PASTE_DEDUP_WINDOW_STATS.get() {
                let mut stats = lock.lock().unwrap();
                stats.window_start_ms = now_unix_ms();
                stats.requests = 0;
                stats.hits = 0;
                stats.last_hit_at_ms = 0;
            }
            Ok(DiagnosticActionResult {
                success: true,
                action_key: request.action_key,
                message: "回写去重计数已清零".to_string(),
                needs_refresh: true,
                should_restart: false,
                navigate_to: None,
                external_url: None,
            })
        }
        "perf-metrics.reset" => {
            reset_perf_metrics();
            Ok(DiagnosticActionResult {
                success: true,
                action_key: request.action_key,
                message: "性能采样已清零".to_string(),
                needs_refresh: true,
                should_restart: false,
                navigate_to: None,
                external_url: None,
            })
        }
        _ => Err(format!("不支持的诊断动作: {}", request.action_key)),
    }
}


