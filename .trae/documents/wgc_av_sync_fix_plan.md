# WGC 窗口录制音画不同步问题修复计划

## 摘要 (Summary)
当前在使用 WGC (Windows Graphics Capture) 窗口录制时存在严重的音画不同步问题，主要表现为声音和画面存在固定的时间错位（提前或滞后），以及录制尾部可能出现的音频拉伸/压缩问题。本计划旨在从**视频时间戳标准化**、**音频精确裁剪(Trim)与延迟(Delay)计算**、**移除不当的音频重采样(Stretching)**三个维度彻底解决该问题。

## 当前状态分析 (Current State Analysis)
通过分析代码库，发现音画不同步的根源在于以下几个方面：
1. **视频首帧时间戳 (PTS) 未归零**：`wgc_capture.rs` 直接使用了系统底层的 QPC (QueryPerformanceCounter) 作为时间戳。虽然底层编码器可能做了部分处理，但这会导致 FFmpeg 合并时对视频起始时间线的认知产生偏差。
2. **音频早于视频时的错误截断**：在 `recorder_service.rs` 中，系统会通过计算 `seg.start_ms` 与 `calibrated_anchor_ms`（视频首帧时间戳）的差值来对齐音视频。但当音频比视频先开始时（即 `seg.start_ms < calibrated_anchor_ms`），代码仅使用了 `saturating_sub` 将延迟设为 0。这意味着**多出来的音频头部没有被裁掉**，直接导致音频从头到尾都比画面提前。
3. **错误的 FFmpeg 滤镜 (aresample=async=1)**：WGC 捕获具有典型的 VFR (变帧率) 特性（画面不动时不产生新帧）。在合并时使用了 `aresample=async=1:first_pts=0`，这会迫使 FFmpeg 尝试对齐音视频流的时长，导致原本正常的音频被错误地拉伸或压缩，引发灾难性的音画错位。

## 提议的修改 (Proposed Changes)

### 1. `src-tauri/src/features/recording/wgc_capture.rs`
**修改内容与原因**：
- 为 `WgcCaptureFlags` 增加 `first_frame_timestamp: Arc<AtomicI64>`。
- 在 `on_frame_arrived` 回调中，拦截所有画面的 `raw_timestamp`，记录首帧时间，并将后续所有帧的 `raw_timestamp` 减去首帧时间，使视频时间戳 (PTS) 严格从 `0` 开始。
- 移除原有的 `if frame_w != self.flags.width` 尺寸判断，统一走 `encoder.send_frame_buffer` 逻辑，确保我们能注入修改后的、归零的 `timestamp`。

### 2. `src-tauri/src/features/recording/state.rs`
**修改内容与原因**：
- 为 `AudioSegment` 结构体增加 `pub trim_start_ms: u64` 字段，默认值为 `0`。
- 用于记录当音频早于视频首帧开始时，需要从音频文件头部裁剪掉的时间。

### 3. `src-tauri/src/features/recording/recorder_service.rs`
**修改内容与原因**：
- **更新音频片段初始化**：在所有创建 `AudioSegment` 的地方，补充 `trim_start_ms: 0`。
- **修复首帧对齐算法**：在 `stop` 逻辑处理音视频对齐时，抛弃 `saturating_sub`。
  - 如果 `seg.start_ms < calibrated_anchor_ms`，则 `seg.trim_start_ms = calibrated_anchor_ms - seg.start_ms`，并且 `seg.start_ms = 0`（不需要额外延迟）。
  - 如果 `seg.start_ms >= calibrated_anchor_ms`，则 `seg.start_ms -= calibrated_anchor_ms`，并且 `seg.trim_start_ms = 0`。
- **FFmpeg 命令组装升级**：
  - 禁用 `trim_start_ms > 0` 时的快速路径 (`merge_audio_fast`)。
  - 在拼接输入流时，如果 `trim_start_ms > 0`，则在 `-i` 参数前插入 `-ss {}.{:03}` 参数（将毫秒转换为秒）。
- **移除危险的音频重采样**：在滤镜组装 `filter_parts` 时，完全移除 `aresample=async=1:first_pts=0`，直接保留 `adelay` 和 `amix`。让音视频保持其真实的长度和时间线，避免受 VFR 的影响而变形。

## 假设与决策 (Assumptions & Decisions)
- **性能开销假设**：在 `wgc_capture.rs` 中取消 `send_frame` 的直接传递，改为全量走内存复制与垂直翻转 (`send_frame_buffer`)，在现代 CPU 上处理 1080p@60fps 的内存拷贝开销微乎其微（毫秒级），相比于解决严重的音画不同步问题，这部分开销是完全可以接受的。
- **视频尾部不补帧**：不再主动为 WGC 视频生成末尾的补帧，而是通过移除 FFmpeg 的 `aresample` 滤镜，让播放器自然处理音频比视频长的情况（通常是保持最后一帧画面），这既符合 MP4 标准，又能避免额外的编码复杂性。

## 验证步骤 (Verification Steps)
1. 编译并启动应用程序。
2. 进行一段 WGC 窗口录制（录制时可以配合秒表/倒计时网页器，或者大声说话并配以明显的鼠标点击动作）。
3. 让画面静止 5-10 秒，然后结束录制。
4. 播放生成的 MP4 文件：
   - 检查鼠标点击声音是否与画面动作精确同步。
   - 检查最后静止的 5-10 秒内，是否声音正常播放而没有被异常加速或截断。