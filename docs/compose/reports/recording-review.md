# Screen Recording Module Review & Optimization Report

**Task:** Task 7 - 屏幕录制模块审查与优化
**Scope:** [S4.4, S5.1, S5.4, S5.5]
**Date:** 2026-07-23

## 1. Architecture Overview

### Frontend (Vue 3)
```
src/pages/recording_toolbar/
├── App.vue                          # Single-file component (~1153 lines)
│   ├── Template: Recording toolbar capsule UI with settings panel
│   ├── Script: State management, IPC calls, event listeners
│   └── Style: Scoped CSS for capsule toolbar
└── main.js                          # Entry point
```

### Backend (Rust / Tauri)
```
src-tauri/src/features/recording/
├── mod.rs                    # Module declarations (10 lines)
├── recorder_service.rs       # Main orchestrator: start/stop/pause/resume (3481 lines)
├── state.rs                  # RecordingRuntime state machine (249 lines)
├── wgc_capture.rs            # Windows Graphics Capture window capture (454 lines)
├── native_wasapi.rs          # WASAPI audio capture (1496 lines)
├── ffmpeg_runner.rs          # FFmpeg path resolution (88 lines)
├── audio_device.rs           # Audio device enumeration (91 lines)
├── events.rs                 # Tauri event emission helpers (93 lines)
├── types.rs                  # Type definitions (92 lines)
├── error_codes.rs            # Error code constants (5 lines)
└── job_object.rs             # Windows Job Object for process cleanup (83 lines)
```

### Data Flow
```
User clicks "Start Recording"
  → App.vue: toggleRecordingState()
  → RecordingService.start() → Tauri invoke("start_recording")
  → recorder_service.rs: start_recording()
    ├── resolve_ffmpeg_path()
    ├── Build output paths (session_id, tmp_path, final_path)
    ├── Validate & lock runtime (RecordingPhase::Starting → Recording)
    ├── Start capture based on target_type:
    │   ├── "window" → wgc_capture.rs: start_window_capture_to_mp4() [WGC API]
    │   └── "screen"/"region" → spawn_ffmpeg_video_segment() [FFmpeg gdigrab]
    ├── Start audio capture if enabled:
    │   ├── System audio → native_wasapi.rs: start_system_loopback_aac_with_device()
    │   └── Microphone → native_wasapi.rs: start_microphone_wav_with_device()
    ├── Spawn stderr parser & stats loop threads
    └── Return RecordingSessionInfo

Recording stops:
  → recorder_service.rs: stop_recording()
    ├── Signal stop flags to all capture threads
    ├── Wait for video capture to finish (WGC thread / FFmpeg process)
    ├── Wait for audio threads to finish
    ├── Post-process: concat segments, trim initial frames, rename
    ├── Emit recording-finished event
    └── Spawn async audio merge task (if audio segments exist)

Audio merge (background):
  → recorder_service.rs: merge_system_audio_into_video()
    ├── Fast path: single segment → stream copy
    ├── Multi-segment: concat then stream copy
    └── Fallback: filter_complex with amix → re-encode
```

## 2. Performance Findings

### 2.1 Backend Issues

| # | Severity | File | Issue | Impact |
|---|----------|------|-------|--------|
| B1 | High | `recorder_service.rs:2918,2930` | **Potential deadlock**: `pause_recording` acquires `lock_arc_mutex(&runtime_arc)` twice in the same scope (line 2918 and 2930). The first MutexGuard is still alive when the second lock is attempted, causing deadlock on non-reentrant Mutex. | Deadlock on pause |
| B2 | Medium | `recorder_service.rs:2526` | `stop_recording` acquires runtime lock twice: once at line 2153 and again at line 2526 for elapsed_ms calculation. | Unnecessary lock contention |
| B3 | Medium | `recorder_service.rs:1654` | `spawn_stats_loop` calls `runtime.snapshot()` twice per iteration (line 1575 and line 1654). | Double computation per 500ms tick |
| B4 | Medium | `recorder_service.rs:1881-2108` | `start_recording` holds runtime lock for ~220 lines while spawning FFmpeg/WGC processes. | Blocks other threads during startup |
| B5 | Low | `native_wasapi.rs:1284` | System audio AAC encode hardcodes `-b:a 128k` ignoring user's configured `audio_bitrate_kbps`. | User audio bitrate setting ignored |

### 2.2 Frontend Issues

| # | Severity | File | Issue | Impact |
|---|----------|------|-------|--------|
| F1 | Medium | `App.vue:1100-1113` | Sequential `await` calls in `onMounted` for device list refreshes (4 sequential IPC round-trips). | Slower toolbar initialization |
| F2 | Low | `App.vue:1116-1121` | `capsuleSettingsVisible` watcher calls `refreshAllDropdownOptions()` on open, potentially redundant. | Minor redundant work |
| F3 | Low | `App.vue` | Single 1153-line SFC with no composables extraction. | Maintainability |

### 2.3 Memory / Resource Issues

| # | Severity | File | Issue | Impact |
|---|----------|------|-------|--------|
| R1 | Low | `recorder_service.rs:1409-1446` | `cleanup_stale_tmp_files` scans entire output directory on every `start_recording`. | Minor I/O overhead |
| R2 | Low | `native_wasapi.rs:55-56` | `AUDIO_RECENT_ACTIVITY` static HashMap persists for app lifetime, entries pruned at 5min but map itself never freed. | Negligible memory |

## 3. Optimizations Implemented

### 3.1 Fix Deadlock in `pause_recording`

**File:** `src-tauri/src/features/recording/recorder_service.rs`

**Problem:** Two `lock_arc_mutex(&runtime_arc)` calls in the same scope (lines 2918 and 2930) create a deadlock risk.

**Fix:** Merge into a single lock scope:
```rust
let elapsed_ms = {
    let mut runtime = lock_arc_mutex(&runtime_arc);
    runtime.wgc_stop_flag = None;
    runtime.wgc_pause_flag = None;
    runtime.system_audio_wav_path = None;
    runtime.system_audio_stream_start_ms = None;
    runtime.mic_audio_wav_path = None;
    runtime.mic_audio_stream_start_ms = None;
    runtime.phase = RecordingPhase::Paused;
    runtime.paused_at_instant = Some(std::time::Instant::now());
    runtime.snapshot().elapsed_ms
};
```

### 3.2 Consolidate Lock in `stop_recording`

**File:** `src-tauri/src/features/recording/recorder_service.rs`

**Problem:** Second lock acquisition at line 2526 for elapsed_ms calculation is unnecessary since `elapsed_ms` can be captured in the first lock scope.

**Fix:** Capture `duration_ms` inside the first lock scope and use it directly.

### 3.3 Fix Double Snapshot in `spawn_stats_loop`

**File:** `src-tauri/src/features/recording/recorder_service.rs`

**Problem:** `runtime.snapshot()` called twice per iteration (line 1575 and line 1654).

**Fix:** Use the first snapshot for elapsed_ms and pass it through.

### 3.4 Pass Configured Audio Bitrate to System Audio AAC

**File:** `src-tauri/src/features/recording/native_wasapi.rs`

**Problem:** `start_system_loopback_aac_with_device` hardcodes `-b:a 128k` instead of using the user's configured bitrate.

**Fix:** Add `audio_bitrate_kbps` parameter and use it in FFmpeg args.

### 3.5 Parallelize Frontend Initialization

**File:** `src/pages/recording_toolbar/App.vue`

**Problem:** Four sequential `await` calls in `onMounted` for device list refreshes.

**Fix:** Use `Promise.allSettled` to run device refreshes in parallel.

## 4. Findings Not Addressed (Out of Scope)

- **A1**: `App.vue` monolith (1153 lines) — refactoring into composables is a larger structural change
- **A2**: `native_wasapi.rs` sample format dispatch duplication — requires macro extraction, high risk
- **R1**: `cleanup_stale_tmp_files` directory scan — acceptable for recording start frequency

## 5. Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| Deadlock fix in pause_recording | Low | Single lock scope is strictly safer than dual scope |
| Lock consolidation in stop_recording | Low | Same data, fewer acquisitions |
| Double snapshot fix | Low | Pure refactor, same semantics |
| Audio bitrate pass-through | Low | Falls back to 128k if invalid |
| Frontend parallel init | Low | Promise.allSettled handles individual failures |
