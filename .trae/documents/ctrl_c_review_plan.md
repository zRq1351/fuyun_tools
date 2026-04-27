# 用户 Ctrl+C 修复方案 Review 及优化计划

## 1. 目标与成功标准
- **目标**：Review 您对“Ctrl+C 复制丢失”问题的修复代码，排查潜在的边缘问题，并制定相应的优化修复计划。
- **成功标准**：
  1. 确认现有的 `missed_event` 和 `LLKHF_INJECTED` 逻辑的健壮性。
  2. 修复 `is_manual_copy` 判断可能引发的严重“误判（False Positive）”导致剪贴板被划词污染的问题。

## 2. 当前状态与潜在问题分析
经过仔细的代码走查，您添加的底层钩子注入判定 (`is_injected`) 和事件漏捕获补偿 (`missed_event`) 逻辑是非常出色的，基本解决了核心痛点。

但是，在 `src-tauri/src/features/text_selection.rs` 中存在一个**高风险的隐患**：
```rust
let manual_c_time = MANUAL_CTRL_C_TIME.load(Ordering::SeqCst);
let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
let is_manual_copy = now.saturating_sub(manual_c_time) < 1000;
```
**隐患场景（False Positive）**：
1. 用户手动选中并按 `Ctrl+C` 复制了 "文本A"（此时 `manual_c_time` 被更新）。
2. 在接下来的 1 秒内，用户使用鼠标划选了 "文本B"。鼠标松开触发了底层的划词检测（`get_selected_text_windows`）。
3. 划词检测结束时，发现当前时间距离上次 `manual_c_time` 不到 1000 毫秒，判定 `is_manual_copy` 为 `true`。
4. 系统跳过了剪贴板快照恢复，并将划选的 "文本B" 强行留在了剪贴板并写入历史记录。
**结果**：用户的剪贴板被意外的鼠标划词污染了（"文本A" 变成了 "文本B"），违背了“划词检测不污染剪贴板”的初衷。

## 3. 提议的变更
为了彻底解决固定 1000ms 时间窗带来的误判，我们需要将时间判定改为“基于当前划词任务的生命周期”。

### 3.1. 优化手动复制的判断逻辑
- **文件**: `src-tauri/src/features/text_selection.rs`
- **修改**:
  1. 在 `get_selected_text_windows` 函数入口处（即捕获旧剪贴板快照之前），记录当前时间戳 `start_time`。
  2. 在函数末尾判断时，将条件从 `< 1000` 改为 `manual_c_time >= start_time`。
  3. **原理**：只有当用户在“当前这划词检测流程执行期间”按下了物理的 `Ctrl+C`，才算作冲突并保留新内容。如果是在划词检测之前按下的，其内容已经被保存在了 `original_snapshot` 中，最后会被正确恢复。

## 4. 假设与决策
- **决策**：保留现有的 `missed_event` 轮询补偿（50ms）和 `LLKHF_INJECTED` 钩子过滤机制，这部分逻辑严密且性能开销极低。仅对时间戳判定进行精确收紧。

## 5. 验证步骤
1. **测试常规防污染**：复制一段文本，然后在 1 秒内用鼠标划选另一段文本。确认划选结束后，剪贴板依然是之前复制的内容，且历史记录中没有新增划选的文本。
2. **测试极端冲突**：用鼠标划选文本，在鼠标松开的瞬间立刻手动按下 `Ctrl+C`，确认该文本被成功记录，并未丢失。