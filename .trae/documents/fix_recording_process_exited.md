# 修复录屏连锁异常及无权限错误 (Error 5) 计划

## 1. 当前问题分析
用户在进行录屏操作时，遇到了由于目标窗口（或系统桌面）权限不足/被系统保护导致的录制失败。具体表现为 FFmpeg 的 `gdigrab` 插件抛出了 `Failed to capture image (error 5)`（Windows 下的 Access Denied 错误）。
目前代码的错误处理机制存在以下不足：
1. **错误提示不友好**：系统未能识别 `error 5`，将其作为普通错误抛出原始的英文 stderr 内容。
2. **重复触发事件（连锁异常）**：FFmpeg 退出时会连续在 stderr 输出多行错误日志（例如 `error 5`、`I/O error`），并最终退出进程。后端代码在 `spawn_stderr_parser` 中逐行解析日志，每匹配到一行错误就会向前端发送一次 `recording-error` 事件；随后 `spawn_stats_loop` 监听到进程退出时又会发送一次错误事件。前端在短时间内收到多个错误事件，就会把它们拼接为带有 `| 连锁异常:` 字样的长报错信息，严重影响用户体验。

## 2. 改进方案 (Proposed Changes)
需要修改 `src-tauri/src/features/recording/recorder_service.rs`，具体步骤如下：

### 2.1 增加特定错误的映射支持
修改 `map_ffmpeg_error` 函数：
- 增加对 `error 5` 及 `access is denied` 的解析判断。
- 映射为 `RECORDING_PROCESS_EXITED`，并提供对用户友好的中文提示：“当前画面录制权限被拒绝（目标窗口可能受系统保护、被最小化或权限不足）。请尝试以管理员身份运行本软件，或选择其他区域/窗口。”。

### 2.2 修复 `spawn_stderr_parser` 多次触发错误事件的问题
修改 `spawn_stderr_parser` 中的日志遍历逻辑：
- 在调用 `map_ffmpeg_error` 映射到错误后，检查 `runtime.last_error` 是否已经有值。
- 只有在 `last_error.is_none()` 时才设置错误状态、发出 `emit_error_payload`，并结束当前录制进程，防止解析到后续错误行时重复触发事件。

### 2.3 修复 `spawn_stats_loop` 在进程退出时重复抛出错误的问题
修改 `spawn_stats_loop` 中的进程退出检查逻辑：
- 在进程 `try_wait()` 成功后，构建退出的错误信息 `err_msg`。
- 检查 `runtime.last_error` 是否已经被前面的日志解析器设置：
  - 如果未设置（`is_none`），说明是静默崩溃或其他原因退出，此时将 `err_msg` 存入 `last_error`，并向前端 emit 错误事件。
  - 如果已设置（`is_some`），说明前面已经向前端抛出过确切的致命错误，此时仅切换 `runtime.phase` 到 `ErrorPhase`，不再向前端发送多余的错误事件。

## 3. 假设与决策 (Assumptions & Decisions)
- **假设**：`error 5` 是权限类问题的明确标识，给出引导用户开启管理员权限或换区录制的建议是合理的。
- **决策**：不再改变前端对“连锁异常”的拼接逻辑，而是从源头（Rust 后端）控制每个录制会话（Session）只抛出一次最准确的致命错误，这不仅解决了当前问题，还能防止未来出现其他的连锁异常提示。

## 4. 验证步骤 (Verification steps)
1. 编译并运行 Tauri 应用。
2. 尝试录制一个带有 UAC 权限限制的窗口或受保护的界面（如浏览器 DRM 视频），触发 `gdigrab` 的 `error 5` 错误。
3. 观察前端错误通知是否仅显示一条友好的中文提示，不再出现多条叠加的 `连锁异常`。