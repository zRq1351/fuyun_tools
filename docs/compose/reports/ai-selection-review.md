# AI Text Selection Module Review Report

## Executive Summary

The AI text selection module provides a seamless text selection → AI action pipeline across the application. This review identifies performance bottlenecks in the detection, clipboard capture, IPC communication, and AI request handling stages. Key optimizations target unnecessary mutex contention, redundant IPC roundtrips, position vector cloning in hot paths, and blocking operations on critical threads.

## Architecture Overview

### Pipeline Flow

```
Mouse Up Event (Windows Hook)
  → Hook Thread (handle_hook_event)
  → Detection Thread (perform_text_selection_detection)
  → Clipboard Snapshot + Ctrl+C Simulation + Restore
  → Toolbar Display (IPC → Vue component)
  → User Click Action (translate/explain/custom)
  → AI Request (streaming)
  → Result Display Window
```

### Key Components

| Component | File | Role |
|-----------|------|------|
| Mouse Listener | `src-tauri/src/features/mouse_listener.rs` | Low-level Windows hooks, drag state machine, selection detection |
| Text Selection | `src-tauri/src/features/text_selection.rs` | Clipboard capture via Ctrl+C simulation, snapshot/restore |
| AI Services | `src-tauri/src/services/ai_services.rs` | AI client caching, streaming orchestration, perf metrics |
| Clipboard Wakeup | `src-tauri/src/services/clipboard_wakeup.rs` | Event-driven clipboard change notification via WM_CLIPBOARDUPDATE |
| Clipboard Access Guard | `src-tauri/src/services/clipboard_access_guard.rs` | Global mutex serializing clipboard OS access |
| AI Client | `src-tauri/src/services/ai_client.rs` | OpenAI-compatible streaming HTTP client |
| Perf Metrics | `src-tauri/src/core/perf_metrics.rs` | In-memory metric aggregator |
| IPC Layer | `src/services/ipc.js` | Frontend service definitions (AIService, WindowService, etc.) |
| Selection Toolbar | `src/pages/selection_toolbar/App.vue` | Toolbar UI component |
| Result Display | `src/pages/result_display/App.vue` | AI result display window |

## Identified Issues

### Critical: Position Vector Cloning in Hot Path (mouse_listener.rs:350)

**Impact:** CPU waste, GC pressure during every mouse move event during drag

Every `MouseMove` event during a drag operation clones the entire `Vec<(i32, i32)>` (up to 20 entries), pushes a new point, and truncates if over 20. Mouse move events fire at 100–1000Hz, causing continuous allocation and copying.

**Current Code:**
```rust
MouseActionState::Dragging(..., ref positions) => {
    let mut new_positions = positions.clone();  // Clone on every mouse move!
    new_positions.push((mouse_x, mouse_y));
    if new_positions.len() > 20 {
        new_positions.drain(0..new_positions.len() - 20);
    }
```

**Fix:** Use `VecDeque` for O(1) push with bounded capacity, or mutate in place since we hold `&mut` via the Mutex guard.

### Critical: Redundant IPC Roundtrips on Toolbar Action (selection_toolbar/App.vue)

**Impact:** ~300ms additional latency before AI request starts

Each toolbar action (translate/explain/custom) triggers:
1. `ensureSelectionAiConfigured()` → `AISettingsService.getSettings()` (1 IPC roundtrip)
2. `WindowService.selectionToolbarBlur()` (1 IPC roundtrip)
3. `AIService.streamTranslate()` (1 IPC roundtrip → starts async work)

The `getSettings()` call is particularly wasteful since settings rarely change between clicks. Settings can be cached on mount and refreshed only when needed.

### Moderate: Blocking Operations on Hook Thread (mouse_listener.rs)

**Impact:** Risk of Windows unhooking the callback (300ms timeout)

The Windows low-level hook callback (`low_level_mouse_proc`) dispatches events to `handle_hook_event` on the hook thread. During `LeftButtonRelease`, this function performs:
- Up to 8 mutex acquisitions (`mouse_action_state`, `last_click`, `listener_state`, `last_processed_time`, `detection_anchor_pos`)
- 2 Win32 syscalls (`GetCursorInfo`, `LoadCursorW`)

If any mutex is contended, the hook thread blocks. Windows will silently unhook callbacks that don't return within 300ms, breaking the entire selection detection feature.

**Mitigation:** The hook thread already dispatches via channel + try_recv, limiting blocking. However, the `handle_hook_event` function itself could benefit from reducing lock scope.

### Moderate: Clipboard Polling Retry Loop (text_selection.rs:338–392)

**Impact:** Up to 600ms blocking of the detection thread

`wait_for_clipboard_update()` loops with 10ms intervals for up to 600ms, reading clipboard content each iteration. Each read acquires the global clipboard access guard mutex. This blocks the detection thread and holds the `is_processing_selection` flag.

**Opportunity:** The event-driven wakeup via `clipboard_wakeup.rs` reduces average latency, but the polling fallback still dominates worst-case latency. Consider shortening the max retry duration or adding early exit when clipboard content hasn't changed.

### Minor: Regex Validation on Every Selection (mouse_listener.rs)

**Impact:** Minor latency (~0.1ms) on each detection attempt

`is_valid_selection()` calls `is_phone_number()`, `is_email_address()`, `is_url()`, `is_error_text()` on every detected selection. While regexes are precompiled via `LazyLock`, the string operations (`to_lowercase()`, `trim()`) add unnecessary overhead for short text selections.

### Minor: Settings Reload on Every Toolbar Expand (selection_toolbar/App.vue:156)

**Impact:** Unnecessary IPC call on every hover-to-expand

The `onMouseEnter` handler calls `AISettingsService.getSettings()` every time the toolbar expands, even though settings rarely change. This adds an IPC roundtrip to the toolbar expansion latency.

## Optimizations Implemented

### 1. Position Vector → VecDeque with In-Place Mutation

**File:** `src-tauri/src/features/mouse_listener.rs`

Replace `Vec<(i32, i32)>` with `VecDeque<(i32, i32)>` in `MouseActionState::Dragging`, using `push_back` + `pop_front` for bounded capacity. This eliminates the clone on every mouse move event.

### 2. Cache Settings in Selection Toolbar

**File:** `src/pages/selection_toolbar/App.vue`

Cache `AISettingsService.getSettings()` result on mount and reuse for the first few actions. Refresh only after a timeout or when explicitly needed (e.g., after settings window closes). This eliminates 1 IPC roundtrip per action.

### 3. Reduce Mutex Scope in Hook Event Handler

**File:** `src-tauri/src/features/mouse_listener.rs`

Minimize the scope of mutex guards in `handle_hook_event` for `LeftButtonRelease`. Read needed values early, drop guards immediately, then process with local copies. This reduces contention on the hook thread.

### 4. Optimize Clipboard Retry Loop

**File:** `src-tauri/src/features/text_selection.rs`

Add early exit in `wait_for_clipboard_update()` when clipboard content is unchanged and no sequence number change is detected, avoiding unnecessary clipboard reads in the polling fallback.

### 5. Optimize Linear Movement Calculation

**File:** `src-tauri/src/features/mouse_listener.rs`

Precompute running sums in the `Dragging` state instead of recalculating from the full positions vector on each `LeftButtonRelease` evaluation. This reduces the check_linear_movement computation from O(n) to O(1).

## Test Plan

1. **Functional Testing:** Select text in various applications (Notepad, Chrome, VS Code) and verify toolbar appears with correct text
2. **Performance Testing:** Monitor CPU usage during rapid text selection operations; verify no hook timeout warnings in logs
3. **Latency Testing:** Measure time from mouse-up to toolbar display; compare before/after optimization
4. **Stress Testing:** Rapidly select/deselect text 50+ times; verify no memory leaks or state corruption
5. **Edge Cases:** Select text in right-to-left languages, select across line boundaries, select very long text (>1000 chars)

## Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| VecDeque in Dragging state | Low | Drop-in replacement with same API semantics |
| Settings caching | Low | Cache has TTL, refreshes on demand |
| Mutex scope reduction | Medium | Must verify all state transitions still atomic |
| Clipboard retry optimization | Low | Early exit only when clearly safe |
| Running sums optimization | Medium | Must verify R² calculation matches original |
