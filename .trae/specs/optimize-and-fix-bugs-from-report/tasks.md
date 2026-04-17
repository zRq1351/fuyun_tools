# Tasks
- [x] Task 1: 修复前端组件事件监听器内存泄漏
  - [x] SubTask 1.1: 调整 `ClipboardList.vue` 中的 window/document 事件监听至 `onMounted` / `onUnmounted`
  - [x] SubTask 1.2: 调整 `ImageClipboardList.vue` 中的事件监听至 `onMounted` / `onUnmounted`
- [x] Task 2: 修复前端渲染及数据流问题
  - [x] SubTask 2.1: 为 `ClipboardList.vue` 中的 `v-memo` 补充 `highlightKeyword` 依赖
  - [x] SubTask 2.2: 优化 `useClipboardHistory.js` 中创建大数组的逻辑，避免稀疏数组
  - [x] SubTask 2.3: 优化 `useClipboardHistory.js` 中大列表过滤的性能，添加防抖或优化算法
  - [x] SubTask 2.4: 修复 `App.vue` (Image Clipboard) 中全量对象替换问题，改为精准修改属性
  - [x] SubTask 2.5: 修复 `useCategoryManager.js` 中直接 `push` 打破单向数据流的问题
  - [x] SubTask 2.6: 修复 `App.vue` (Image Clipboard) 中截断引发的越界风险
- [x] Task 3: 补充前端操作的错误处理与反馈
  - [x] SubTask 3.1: 为相关核心 I/O（如删除、打开链接）操作补充 UI 提示反馈
- [x] Task 4: 修复后端 Rust 剪贴板 FFI 死锁风险
  - [x] SubTask 4.1: 在 `image_clipboard.rs` 中引入 RAII 确保 `CloseClipboard` 一定执行
- [x] Task 5: 优化后端 Rust 数据库 N+1 查询与 OOM 风险
  - [x] SubTask 5.1: 改造 `database.rs` 与 `image_store.rs` 中的逐条写入为批量更新
  - [x] SubTask 5.2: 改造 `database.rs` 与 `image_store.rs` 中的全量数据加载，增加懒加载或分页机制
- [x] Task 6: 修复后端异步阻塞与并发锁问题
  - [x] SubTask 6.1: 使用 `spawn_blocking` 包装 `backup_restore.rs` 和 `backup_archive.rs` 的同步 I/O
  - [x] SubTask 6.2: 缩短 `commands_recording.rs` 的 `Mutex` 锁作用域并清理高危 `unwrap()`

# Task Dependencies
- [Task 1] 独立进行
- [Task 2] 独立进行
- [Task 3] 独立进行
- [Task 4] 独立进行
- [Task 5] 独立进行
- [Task 6] 独立进行
