# 项目深度记忆库（性能优化建议 + 潜在 Bug 报告）

## 分析范围与方法

- 范围：仅分析 `git ls-files --cached --others --exclude-standard` 返回的文件集合，自动遵循 `.gitignore`，已排除 `node_modules`、`dist` 等忽略目录。
- 技术栈识别：前端为 `Vue3 + Vite + Element Plus + Tauri API`；后端为 `Rust + Tauri2 + SQLx(Sqlite) + WinAPI/WGC + ffmpeg`。
- 方法：静态代码深读（前端 UI 事件/响应式、后端并发/IO/数据库/线程生命周期），结合高风险模式检索（`expect/unwrap`、线程/锁、定时器、全量扫描/深度 watch 等）。

## 记忆索引（便于后续快速检索）

- `PERF-001`：截图页文本图层定位更新存在 O(n^2) 查找。
- `PERF-002`：设置页 `deep watch` 全量监听造成高频序列化与无差别保存。
- `PERF-003`：图片历史页多处重复全量统计与查找，列表变大后退化明显。
- `PERF-004`：文本历史库每次打开连接都执行 schema/migration/FTS 同步，冷热点路径过重。
- `PERF-005`：文本历史持久化以“全量快照”方式频繁落库，放大 IO 与锁竞争。
- `PERF-006`：固定截图窗口初始化采用多次 `sleep + eval` 重试，创建路径存在额外延迟与 CPU 消耗。
- `BUG-001`：图片历史页 `warmupBatchTimer` 未在卸载时清理，存在卸载后回调触发风险。
- `BUG-002`：图片数据库连接池初始化存在并发竞态，可能误报“连接池已初始化”并导致调用失败。
- `BUG-003`：剪贴板事件后端初始化超时后未停止后台线程，可能造成隐藏监听线程泄漏。
- `BUG-004`：`stable_history_item_id` 使用 `DefaultHasher`，跨 Rust 版本/实现变更存在 ID 不稳定风险。
- `BUG-005`：按内容删除历史时只清理单个 `item_id` 关联，重复内容场景可能残留脏关联数据。
- `BUG-006`：多个关键路径使用 `expect` 获取状态（音频路径/互斥锁），异常态会直接 panic 终止流程。

## 性能优化建议

### PERF-001 截图文本图层更新存在 O(n^2)

- **定位**：`src/pages/screenshot/App.vue:586-602`。
- **现象**：在 `watchPostEffect` 中遍历 `textOverlayRefMap` 时，对每个 id 再执行一次 `textItems.find(...)`，形成 O(n^2)。
- **影响**：文本图层数量上升时，拖拽/编辑过程的帧率下降，尤其在高 DPI 或复杂标注场景下明显。
- **建议**：
  1. 预构建 `Map<string, item>`（一次 O(n)），循环中 O(1) 读取。
  2. 将样式同步拆为“新增/更新/删除”三类增量分支，避免每次全量扫。

### PERF-002 图片历史页重复全量统计/查找

- **定位**：`src/pages/image_clipboard/App.vue:1297-1300, 1362-1365, 1467-1472, 1502-1525, 1604`。
- **现象**：多处重复使用 `history.value.filter(Boolean).length`、`findIndex` 等全量操作，且在频繁事件（增量同步、预览就绪、筛选）中反复执行。
- **影响**：历史记录规模大时（百/千级）CPU 占用抬升，影响滚动与键盘导航流畅度。
- **建议**：
  1. 维护 `loadedCount`、`id->index` 的增量索引，替代重复全量扫描。
  2. 合并同一刷新周期内的多次统计（单帧一次）。

### PERF-003 数据库连接打开路径过重

- **定位**：`src-tauri/src/utils/database.rs:74-83, 86-264`。
- **现象**：每次 `open_history_db_async()` 都会执行 `ensure_history_db_schema_async()`，其中包含多条 `UPDATE`、表结构迁移检测、FTS 重建/清理。
- **影响**：高频读写时额外引入迁移级开销，放大 IO 与锁等待，影响响应时间。
- **建议**：
  1. 将 schema 校验迁移为“启动一次性”流程（例如 `OnceLock`）。
  2. FTS 全量修复改为“版本号触发 + 增量维护”。

### PERF-004 文本历史持久化以全量快照驱动

- **定位**：`src-tauri/src/utils/clipboard.rs:73-83, 209, 257, 304, 367, 413, 480, 506, 543, 570, 607, 664, 756`；`src-tauri/src/utils/database.rs:669-848`。
- **现象**：多种操作最终都会 enqueue 快照，保存逻辑遍历并 upsert 全量历史项，再执行 stale 清理。
- **影响**：写放大明显，历史规模增长后对吞吐和磁盘寿命不友好。
- **建议**：
  1. 优先走增量 API（新增/删除/分类/置顶）而非全量快照。
  2. 快照仅用于恢复点（低频、批处理）或版本迁移。

### PERF-005 固定截图窗口初始化重试策略偏重

- **定位**：`src-tauri/src/ui/commands.rs:3222-3236`。
- **现象**：每次固定图片窗口都创建线程，`sleep` 后最多循环 8 次 `window.eval(...)`。
- **影响**：高频 pin 图片时线程与脚本执行开销增加，且固定延迟影响首次可交互时间。
- **建议**：
  1. 改为单次注入 + 前端 `ready` 事件握手。
  2. 设置超时与取消机制，窗口关闭后提前退出。

## 潜在 Bug 报告

### BUG-001 卸载后定时器回调风险（图片历史页）

- **定位**：`src/pages/image_clipboard/App.vue:991-1015, 1803-1850`。
- **问题**：定义了 `warmupBatchTimer`，但 `onBeforeUnmount` 未清理该定时器。
- **风险**：组件卸载后回调仍可能执行，触发状态读写或异步请求，造成潜在异常与“幽灵任务”。
- **修复建议**：在 `onBeforeUnmount` 中补充 `clearTimeout(warmupBatchTimer)` 并置空。

### BUG-002 图片数据库连接池并发初始化竞态

- **定位**：`src-tauri/src/utils/image_store.rs:168-189`。
- **问题**：`get_pool()` 在并发首访时，多个调用都可能创建连接池；后到者 `DB_POOL.set(...)` 失败直接返回错误 `"连接池已初始化"`。
- **风险**：调用方可能收到误报失败（虽然全局池已存在），引发链路抖动。
- **修复建议**：
  1. 使用 `OnceCell::get_or_try_init` 风格原子初始化。
  2. 若 `set` 失败，回退读取 `DB_POOL.get()` 并返回已存在池，而非报错。

### BUG-003 剪贴板事件后端超时后可能残留后台线程

- **定位**：`src-tauri/src/services/clipboard_wakeup.rs:173-327`。
- **问题**：后台线程先启动并初始化窗口；主线程 `recv_timeout(600ms)` 超时即返回 `None`，但没有向后台线程发送停止信号。
- **风险**：可能出现“逻辑上已降级轮询，但事件监听线程仍存活”的资源泄漏或双后端并存。
- **修复建议**：
  1. 引入初始化取消 token（超时后通知线程销毁窗口并退出消息循环）。
  2. 仅在线程确认 ready 后对外暴露后端实例。

### BUG-004 历史项 ID 的“稳定性”实现不稳定

- **定位**：`src-tauri/src/utils/database.rs:59-63`；`src-tauri/src/utils/clipboard.rs:43-47`。
- **问题**：`stable_history_item_id` / `stable_text_hash` 使用 `DefaultHasher`，其实现稳定性不应跨版本假设。
- **风险**：Rust 版本升级或实现变化后，历史项映射（分类/置顶/关联）可能失配。
- **修复建议**：改为显式稳定哈希（如 `xxhash-rust` / `sha256`）并做一次性迁移。

### BUG-005 按内容删除时关联清理可能不完整

- **定位**：`src-tauri/src/utils/database.rs:1025-1060`。
- **问题**：删除前只读取一个 `item_id`，但主表按 `content` 删除可能删掉多行；随后仅按单个 `item_id` 清理 FTS/分类/置顶。
- **风险**：重复内容或历史脏数据场景下，关联表残留“孤儿数据”。
- **修复建议**：先查询并收集该 `content` 对应全部 `item_id`，再批量清理关联表。

### BUG-006 `expect` 在关键链路会直接 panic

- **定位**：
  - `src-tauri/src/features/recording/recorder_service.rs:629, 688`
  - `src-tauri/src/ui/commands.rs:347`
  - `src-tauri/src/lib.rs:36`
- **问题**：关键运行路径对可恢复状态使用 `expect(...)`。
- **风险**：一旦出现异常状态（锁中毒、状态未按预期设置），应用直接崩溃而非可恢复报错。
- **修复建议**：统一替换为 `Result` 传播 + 业务错误码上报，保留运行态可恢复能力。

## 说明

- 本报告聚焦“高价值热点路径”与“高风险缺陷模式”，已覆盖前端多窗口页面、Tauri 命令层、录屏链路、数据库与剪贴板核心模块。
- 若你需要，我可以继续基于本记忆库直接输出下一步“可执行修复清单（按文件逐条 patch 方案）”。

