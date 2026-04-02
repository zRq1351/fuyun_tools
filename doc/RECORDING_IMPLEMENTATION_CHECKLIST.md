# 录屏录音（可生产发布）开工接口清单

## 1. 目标与范围

- 目标：在现有 Tauri/Vue 架构下落地「录屏 + 录音频（麦克风/系统音频）」能力。
- 首发平台：Windows。
- 发布要求：可进入现有自动更新与发布流程，具备错误可观测、长录制稳定性与资源控制。

## 2. 与现有工程风格对齐

- Tauri 命令名：`snake_case`。
- 事件名：`kebab-case`。
- 前端页面不得直接调用 `invoke`，统一经 `src/services` 服务层。
- 设置项命名：前端 `camelCase`，后端/持久化 `snake_case`，由设置页做字段映射。

## 3. 文件级实施清单（后端 Rust）

### 3.1 新增文件

- `src-tauri/src/features/recording/mod.rs`
- `src-tauri/src/features/recording/types.rs`
- `src-tauri/src/features/recording/state.rs`
- `src-tauri/src/features/recording/recorder_service.rs`
- `src-tauri/src/features/recording/ffmpeg_runner.rs`
- `src-tauri/src/features/recording/audio_device.rs`
- `src-tauri/src/features/recording/events.rs`
- `src-tauri/src/features/recording/error_codes.rs`
- `src-tauri/src/ui/commands_recording.rs`

### 3.2 修改文件

- `src-tauri/src/features/mod.rs`
    - 导出 `recording` 模块。
- `src-tauri/src/ui/mod.rs`
    - 挂载 `commands_recording`。
- `src-tauri/src/lib.rs`
    - 注册录屏命令到 `invoke_handler![]`。
    - 启动时初始化录屏运行时状态。
- `src-tauri/src/core/app_state.rs`
    - 新增 `recording_runtime`（`Arc<Mutex<...>>`）状态。
- `src-tauri/src/utils/settings_model.rs`
    - 扩展录屏设置字段及默认值。
- `src-tauri/src/core/config.rs`
    - 新增默认快捷键常量 `DEFAULT_RECORDING_SHORTCUT`。
- `src-tauri/capabilities/default.json`
    - 确保录屏页面窗口标签与必要权限可访问。
- `src-tauri/tauri.conf.json`
    - 新增 `recording` 页面窗口定义。
    - 新增打包 sidecar（ffmpeg 可执行文件）配置。
- `src-tauri/Cargo.toml`
    - 仅补充必要依赖（序列化、状态管理等），避免引入重型重复依赖。

## 4. 命令接口（后端）

### 4.1 命令清单

- `start_recording`
- `pause_recording`
- `resume_recording`
- `stop_recording`
- `cancel_recording`
- `get_recording_state`
- `list_recording_audio_devices`
- `open_recording_folder`

### 4.2 数据结构（建议放 `types.rs`）

- `StartRecordingRequest`（`#[serde(rename_all = "camelCase")]`）
    - `target_type: String`（`display | window`）
    - `target_id: Option<String>`
    - `capture_cursor: bool`
    - `capture_system_audio: bool`
    - `capture_microphone: bool`
    - `microphone_device_id: Option<String>`
    - `fps: u32`
    - `video_bitrate_kbps: u32`
    - `audio_bitrate_kbps: u32`
    - `output_dir: Option<String>`
    - `container: String`（首版固定 `mp4`）
    - `op_id: Option<u64>`
- `RecordingSessionInfo`
    - `session_id: String`
    - `started_at_ms: i64`
    - `output_path_tmp: String`
- `RecordingRuntimeState`
    - `state: String`（`idle|starting|recording|paused|stopping|error`）
    - `session_id: Option<String>`
    - `elapsed_ms: u64`
    - `dropped_video_frames: u64`
    - `audio_buffer_level_ms: u32`
    - `last_error: Option<String>`
- `RecordingStopResult`
    - `session_id: String`
    - `output_path: String`
    - `duration_ms: u64`
    - `file_size_bytes: u64`
- `AudioInputDevice`
    - `id: String`
    - `name: String`
    - `is_default: bool`

## 5. 状态机（后端）

- 状态集合：`IDLE -> STARTING -> RECORDING <-> PAUSED -> STOPPING -> FINALIZING -> COMPLETED`
- 异常分支：任一状态可进入 `ERROR`；用户取消进入 `CANCELED`。
- 并发规则：
    - 全局单活会话（同一时刻只允许一个 `session_id`）。
    - `pause/resume/stop/cancel` 需要幂等。
    - `stop` 允许在 `paused` 状态直接执行。
- 生命周期规则：
    - 录制文件先写 `*.tmp.mp4`，结束后原子重命名为最终文件。
    - 异常退出后自动扫描并清理陈旧临时文件。

## 6. 事件协议（后端 -> 前端）

### 6.1 事件名

- `recording-state-changed`
- `recording-stats-updated`
- `recording-error`
- `recording-finished`
- `recording-device-list-updated`

### 6.2 事件负载

- `recording-state-changed`
    - `{ sessionId, state, elapsedMs }`
- `recording-stats-updated`
    - `{ sessionId, fps, videoBitrateKbps, audioBitrateKbps, droppedVideoFrames, audioBufferLevelMs }`
- `recording-error`
    - `{ sessionId, code, message, recoverable }`
- `recording-finished`
    - `{ sessionId, outputPath, durationMs, fileSizeBytes }`
- `recording-device-list-updated`
    - `{ microphones: [{ id, name, isDefault }] }`

### 6.3 推送节流

- `recording-stats-updated` 固定 500ms 推送一次。
- 仅在状态变化时发送 `recording-state-changed`。

## 7. 错误码规范（建议）

- `RECORDING_ALREADY_RUNNING`
- `RECORDING_NOT_RUNNING`
- `RECORDING_START_FAILED`
- `RECORDING_STOP_FAILED`
- `RECORDING_PERMISSION_DENIED`
- `FFMPEG_NOT_FOUND`
- `FFMPEG_EXEC_ERROR`
- `AUDIO_DEVICE_NOT_FOUND`
- `AUDIO_DEVICE_LOST`
- `DISK_SPACE_INSUFFICIENT`
- `OUTPUT_PATH_INVALID`
- `UNSUPPORTED_CAPTURE_TARGET`

## 8. 文件级实施清单（前端 Vue）

### 8.1 新增文件

- `src/pages/recording/App.vue`
- `src/pages/recording/main.js`
- `src/services/recording-ipc.js`（或扩展 `src/services/ipc.js`）
- `src/composables/useRecordingState.js`
- `src/pages/settings/components/RecordingSettings.vue`

### 8.2 修改文件

- `src/vite.config.js`
    - 新增 `recording.html` 多入口配置。
- `src/recording.html`
    - 新增录屏页面 HTML 入口。
- `src/services/ipc.js`
    - 增加 `IPC_COMMANDS` 与 `RecordingService`（若不拆分新文件）。
- `src/pages/settings/App.vue`
    - 新增 `recording` 分栏。
    - 扩展 `form` 字段、变更检测与保存映射。

## 9. 设置项字段清单

### 9.1 后端（`snake_case`）

- `recording_enabled: bool`
- `recording_hot_key: String`
- `recording_default_fps: u32`
- `recording_default_video_bitrate_kbps: u32`
- `recording_default_audio_bitrate_kbps: u32`
- `recording_capture_cursor: bool`
- `recording_capture_system_audio: bool`
- `recording_capture_microphone: bool`
- `recording_microphone_device_id: String`
- `recording_output_dir: String`
- `recording_auto_open_folder: bool`
- `recording_max_duration_minutes: u32`
- `recording_file_name_template: String`

### 9.2 前端（`camelCase`）

- `recordingEnabled`
- `recordingToggleShortcut`
- `recordingDefaultFps`
- `recordingDefaultVideoBitrateKbps`
- `recordingDefaultAudioBitrateKbps`
- `recordingCaptureCursor`
- `recordingCaptureSystemAudio`
- `recordingCaptureMicrophone`
- `recordingMicrophoneDeviceId`
- `recordingOutputDir`
- `recordingAutoOpenFolder`
- `recordingMaxDurationMinutes`
- `recordingFileNameTemplate`

## 10. FFmpeg 集成规范（生产发布）

- 采用 sidecar 模式随应用发布。
- 启动录制前执行 ffmpeg 自检（存在性、可执行、版本）。
- 命令行参数统一由 `ffmpeg_runner.rs` 生成，禁止散落拼接。
- 对所有外部参数做白名单校验（路径、容器、码率范围）。
- 录制时输出 stderr 到结构化日志，失败时回传错误码与摘要信息。

## 11. 分期落地（可并行分工）

- P1（MVP）：全屏录制 + 麦克风录音 + 停止导出 + 设置页基础配置 + 快捷键。
- P2：系统音频（WASAPI loopback）+ 暂停/恢复 + 录制统计。
- P3：窗口录制 + 设备热插拔恢复 + 崩溃恢复与临时文件治理。

## 12. 开发验收清单（Definition of Done）

- 功能：
    - 可启动、暂停、恢复、停止、取消录制。
    - 输出文件可正常播放，音画同步无明显漂移。
- 稳定性：
    - 连续录制 60 分钟无崩溃，无内存持续异常增长。
    - 异常断开设备后可感知并给出明确提示。
- 可观测：
    - 关键状态和错误码可在日志追踪。
    - 前端可展示基础录制状态与错误原因。
- 发布：
    - CI 产物包含 sidecar，安装后无需手工补依赖即可录制。
    - 自动更新后录制功能可用，旧配置可平滑迁移。

## 13. 直接开工顺序（建议）

- 第 1 天：后端状态机与命令骨架、前端服务层接口、设置字段定义。
- 第 2 天：ffmpeg runner + start/stop 主链路（先麦克风）。
- 第 3 天：事件推送 + 页面状态联动 + 错误码打通。
- 第 4 天：系统音频、暂停/恢复、打包 sidecar。
- 第 5 天：回归验证、长录制压测、发布链路验收。
