# 性能优化与 Bug 修复 Spec

## Why
根据前期的深度记忆库分析报告，项目中存在若干影响性能和系统稳定性的关键问题，包括前端内存泄漏、渲染重排、大数组内存浪费，以及后端 Rust 的 N+1 查询、OOM 风险、异步阻塞和 Unsafe 死锁风险等。需要对这些问题进行针对性优化和修复，以提升整体运行效率与安全性。

## What Changes
- **前端优化**：
  - 修复 `ClipboardList.vue` 和 `ImageClipboardList.vue` 中的事件监听器内存泄漏，移入组件生命周期中。
  - 修复 `ClipboardList.vue` 中 `v-memo` 缺少 `highlightKeyword` 依赖的问题。
  - 优化 `useClipboardHistory.js` 中使用 `new Array` 造成稀疏大数组的内存浪费问题。
  - 优化 `useClipboardHistory.js` 中的列表过滤逻辑，减轻主线程负担。
  - 优化 `App.vue` (图片剪贴板) 中图片对象整体替换造成的重排，改为属性精确修改。
  - 完善核心操作异常的错误处理反馈机制，增加 UI 提示。
  - 修复 `useCategoryManager.js` 对 `categories` 直接 `push` 造成的单向数据流破坏。
  - 修复 `App.vue` (图片剪贴板) 全量同步截断引发的越界风险。
- **后端优化**：
  - **BREAKING** 改造 `database.rs` 和 `image_store.rs` 中的 N+1 数据库循环查询，改为批量操作。
  - **BREAKING** 修改历史数据和图片数据的全量加载，加入分页或限制加载机制，避免 OOM 风险。
  - 使用 `tokio::task::spawn_blocking` 改造 `backup_restore.rs` 和 `backup_archive.rs` 中的重度同步磁盘 I/O。
  - **BREAKING** 在 `image_clipboard.rs` 中引入 `scopeguard` 或 RAII 防止 Panic 引起的 `CloseClipboard` 遗漏导致的系统级剪贴板死锁。
  - 缩减 `commands_recording.rs` 等模块中并发锁 `Mutex` 的作用域，清理隐患 `unwrap()`。

## Impact
- Affected specs: 剪贴板历史渲染、图片剪贴板预览、录屏并发状态、数据导入与备份恢复。
- Affected code: 
  - `src/pages/clipboard/components/ClipboardList.vue`
  - `src/pages/image_clipboard/components/ImageClipboardList.vue`
  - `src/pages/clipboard/composables/useClipboardHistory.js`
  - `src/pages/image_clipboard/App.vue`
  - `src/pages/clipboard/composables/useCategoryManager.js`
  - `src-tauri/src/utils/database.rs`
  - `src-tauri/src/utils/image_store.rs`
  - `src-tauri/src/utils/backup_restore.rs`
  - `src-tauri/src/utils/backup_archive.rs`
  - `src-tauri/src/utils/image_clipboard.rs`
  - `src-tauri/src/ui/commands_recording.rs`

## ADDED Requirements
### Requirement: 安全的 FFI 调用
系统应当保证全局剪贴板锁的安全释放。
#### Scenario: Success case
- **WHEN** 访问 Windows 剪贴板遇到错误或异常 panic
- **THEN** 自动调用 CloseClipboard 释放锁，系统剪贴板不被卡死

## MODIFIED Requirements
### Requirement: 高效的数据库合并与恢复
批量数据写入不应当阻塞异步线程。
### Requirement: 前端列表高效渲染
使用精确的数据绑定和事件生命周期管理。

## REMOVED Requirements
无
