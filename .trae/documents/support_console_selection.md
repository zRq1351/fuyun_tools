# 支持在命令行/终端中划词的修改计划

## 摘要 (Summary)
当前项目在划词后通过模拟 `Ctrl+C` 来复制选中的文本。由于在 Windows 命令行/终端（如 cmd, PowerShell, Windows Terminal）中，`Ctrl+C` 通常会触发 `SIGINT`（中断当前正在执行的命令），因此当前代码通过 `is_foreground_window_console()` 粗暴地排除了在终端环境下的划词检测。
本计划旨在移除这一排除限制，并在检测到目标窗口为命令行/终端时，将模拟按键从 `Ctrl+C` 智能切换为更安全的 `Ctrl+Insert`，从而在不影响命令行正常运行的情况下完美支持终端划词。

## 当前状态分析 (Current State Analysis)
1. **`src-tauri/src/features/mouse_listener.rs`**:
   - `is_foreground_window_console()` 用于检测前台是否为终端窗口（支持识别 cmd, powershell, Windows Terminal, mintty 等）。
   - 在鼠标松开事件（`HookEvent::LeftButtonRelease`）和 `perform_text_selection_detection` 函数中，只要检测到是终端窗口，就会直接跳过划词流程，导致在终端下无法划词。
2. **`src-tauri/src/features/text_selection.rs`**:
   - `execute_ctrl_c_with_safety` 内部固定模拟发送 `Ctrl + C`（使用 `enigo` 的 `Key::Unicode('c')`）。

## 建议更改 (Proposed Changes)

### 1. 修改 `src-tauri/src/features/mouse_listener.rs`
- **操作**: 
  - 将 `fn is_foreground_window_console() -> bool` 的可见性修改为 `pub fn is_foreground_window_console() -> bool`，以便其他模块调用。
  - 在 `handle_hook_event` 中的 `HookEvent::LeftButtonRelease` 处理逻辑里，**移除** `if !is_foreground_window_console()` 的条件判断及对应的 `else` 分支，允许终端窗口触发划词检测。
  - 在 `perform_text_selection_detection` 函数中，**移除**以下跳过逻辑：
    ```rust
    if is_foreground_window_console() {
        log::info!("在命令行/终端环境中，跳过划词检测");
        return None;
    }
    ```

### 2. 修改 `src-tauri/src/features/text_selection.rs`
- **操作**:
  - 在 `execute_ctrl_c_with_safety` 函数中调用 `crate::features::mouse_listener::is_foreground_window_console()` 获取当前是否为终端窗口。
  - 修改 `enigo.key(Key::Unicode('c'), enigo::Direction::Click)` 的调用：如果是终端窗口，且在 Windows 系统下，则使用 `Key::Raw(0x2D)`（对应 `VK_INSERT`，即 `Insert` 键的虚拟键码）代替 `Key::Unicode('c')`。
  - 修改后的按键逻辑示例：
    ```rust
    let copy_key = if is_console {
        #[cfg(target_os = "windows")]
        { Key::Raw(0x2D) } // 0x2D 是 VK_INSERT
        #[cfg(not(target_os = "windows"))]
        { Key::Unicode('c') }
    } else {
        Key::Unicode('c')
    };
    
    match enigo.key(copy_key, enigo::Direction::Click) {
        ...
    }
    ```
  - 更新对应的日志信息，表明发送的是 `Ctrl+C` 还是 `Ctrl+Insert`。

## 假设与决策 (Assumptions & Decisions)
- **快捷键选择**: `Ctrl+Insert` 是 Windows 平台上最通用且安全的控制台复制快捷键（支持 CMD、PowerShell、Windows Terminal、Git Bash 等），且在未选中任何文本时按下不会产生副作用（不会像 `Enter` 键那样错误地执行命令）。
- **跨平台影响**: 控制台检测函数 `is_foreground_window_console` 目前仅在 Windows 下生效（其他平台返回 `false`）。因此 Linux 和 macOS 的划词行为将保持原样，这符合预期，因为 macOS 终端中 `Cmd+C` 本身就是复制且不会中断命令。
- **enigo 兼容性**: 已确认当前项目中使用的 `enigo 0.6.1` 支持 `Key::Raw(u16)` 枚举变体，可以安全地发送 `0x2D`。

## 验证步骤 (Verification steps)
1. 编译并运行项目。
2. 打开普通的文本编辑器，测试划词是否仍然正常工作（确保非终端环境未受影响）。
3. 打开 Windows Terminal (或 cmd/PowerShell)，输入一条长命令或运行一个耗时命令（如 `ping 127.0.0.1 -t`）。
4. 在终端中划词（拖拽选中一段文本）。
5. 验证：
   - 划词后应该能正常弹出工具栏或获取到所选文本。
   - 正在执行的命令**不能**被中断退出（即没有触发 SIGINT）。