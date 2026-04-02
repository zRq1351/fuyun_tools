# 录屏端到端回归（脚本化）

## 1. 目标

- 提供可重复执行的录屏自测链路：启动录制 -> 暂停 -> 恢复 -> 停止 -> 产物校验。
- 快速验证 P2/P3 关键能力：系统状态机、pause/resume、文件落盘、基本统计事件。

## 2. 已落地能力

- 后端新增命令：`run_recording_regression`。
- 回归流程固定执行：
    - `start_recording`（display，默认关闭音频输入以提高环境兼容性）
    - 等待约 1.2s
    - `pause_recording`
    - 等待约 0.7s
    - `resume_recording`
    - 等待约 1.2s
    - `stop_recording`
    - 校验输出文件存在且大小 > 0
- 返回结构：`success/sessionId/outputPath/durationMs/fileSizeBytes/steps/message`。

## 3. 使用方式

- 方式 A（推荐）：打开录屏页面，点击“运行回归自测”按钮。
- 方式 B：通过前端 IPC 服务调用 `RecordingService.runRegression()`。

## 4. 通过标准

- 返回 `success = true`。
- `outputPath` 非空，文件存在且 `fileSizeBytes > 0`。
- `steps` 至少包含：
    - `start_recording:ok`
    - `pause_recording:ok`
    - `resume_recording:ok`
    - `stop_recording:ok`
    - `verify_output_file:ok`

## 5. 失败排查

- 若返回 ffmpeg 启动失败：
    - 检查 sidecar 是否存在；
    - 检查发布流程 FFmpeg 下载与校验步骤是否通过。
- 若输出文件为空：
    - 检查桌面采集权限；
    - 检查 ffmpeg stderr 中是否存在进程异常退出信息。
- 若 pause/resume 失败：
    - 检查录制状态是否被并发切换；
    - 检查录制会话是否已被自动停止。
