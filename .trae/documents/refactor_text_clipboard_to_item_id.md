# 文本剪贴板 `item_id` 彻底重构计划

## 1. 概述 (Summary)
彻底将文本剪贴板的前后端通信、数据库关联表和后端内存状态从依赖 `content`（完整文本内容）和 `index`（数组索引）切换为轻量级且唯一稳定的 `item_id`。此举旨在消除由于前后端异步更新导致的索引越界、操作错位、内存膨胀以及关系表数据冗余等潜在 Bug。

## 2. 当前状态分析 (Current State Analysis)
根据前期的代码探索和检索，当前系统存在以下未彻底切换的痛点：
- **IPC通信**：前端 `ipc.js` 和后端 `commands.rs` 的 API（如删除、置顶、填充等）依然保留并允许使用 `index` 参数作为后备查找手段。
- **数据库操作**：`categories` 和 `pinned_items` 等关联表在迁移和插入时，仍然显式保留并写入了冗余的 `content TEXT` 字段，甚至会为了获取 `content` 而多做一次查询。
- **内存状态更新**：后端的核心状态 `ClipboardManager` 的 `history` 直接存储了全量的 `Vec<String>`，而 `exact_index_cache` 强依赖数组索引。前端的 `useClipboardHistory.js` 也遗留了基于位置的映射逻辑。

## 3. 拟议更改 (Proposed Changes)

### Step 1: 数据库层重构 (Database)
**目标文件**：`src-tauri/src/utils/database.rs`
- **更改内容**：
  1. 移除 `categories_new` 和 `pinned_items_new` 建表语句中的 `content TEXT` 字段，仅保留 `item_id` 及相关元数据。
  2. 重写涉及这些表的增量 CRUD 操作（如 `save_categories_state_async`、`save_pinned_items_order_async`、`pin_item`）。不再执行 `SELECT content`，直接使用传入的 `item_id` 进行关联表的 `INSERT/UPDATE/DELETE`。
- **原因**：消除数据冗余，保证主表与关联表数据一致性。

### Step 2: 核心状态与内存模型重构 (State)
**目标文件**：`src-tauri/src/utils/clipboard.rs`
- **更改内容**：
  1. 定义标准的 `ClipboardItem` 结构体（包含 `id` 和 `content` 等必要信息），将 `history: Arc<Mutex<Vec<String>>>` 重构为 `history: Arc<Mutex<Vec<ClipboardItem>>>`。
  2. 废弃或重构强依赖数组索引的 `exact_index_cache`。改为基于 `item_id` 的 $O(1)$ 查找映射（如 `HashMap<String, usize>` 或直接使用结构体引用）。
  3. 更新相应的内部查找逻辑（如 `get_content`、`add_to_history`、`remove_item` 等），全部基于 `item_id` 匹配。
- **原因**：使内存状态具备唯一标识符能力，脱离对不稳定的 `index` 的依赖。

### Step 3: 前后端 IPC 接口清洗 (IPC)
**目标文件**：`src/services/ipc.js`, `src-tauri/src/ui/commands.rs`
- **更改内容**：
  1. **前端**：在 `ipc.js` 中，移除 `removeItem`、`setItemPinned`、`promoteItem`、`selectAndFill` 等方法中的 `index` 参数，仅保留并强制要求 `itemId`。
  2. **后端**：在 `commands.rs` 中，移除请求结构体（如 `SelectAndFillRequest`）中的 `index` 字段，并调整对应的 `execute_*` 指令函数，严格通过 `item_id` 寻找目标项并执行逻辑。
- **原因**：切断前后端的 `index` 耦合，彻底杜绝操作错位（如前端点击删除 A，后端因为索引偏移删除了 B）。

### Step 4: 状态同步与前端清理 (Frontend & Events)
**目标文件**：`src-tauri/src/services/clipboard_manager.rs`, `src/pages/clipboard/composables/useClipboardHistory.js`
- **更改内容**：
  1. 检查后端 `clipboard-history-item-updated` 事件的 Payload，确保推送时明确包含 `latest_item_id`，前端据此进行列表增量更新。
  2. 清理 `useClipboardHistory.js` 中遗留的、无用的基于 `position` 的 `historyMap` 映射逻辑，确保所有列表操作（Vue 渲染）严格绑定 `:key="item.id"`。
- **原因**：保持前端代码整洁，确保 UI 渲染与真实数据 ID 强绑定。

## 4. 假设与决策 (Assumptions & Decisions)
- **决策：内存保留 Content**。为了避免引入复杂的“列表滚动按需加载文本”的异步逻辑（可能导致 UI 滚动卡顿或白屏），本次重构在后端的 `ClipboardManager` 内存中暂时保留 `content`（即采用 `{id, content}` 结构体），但彻底废除使用 `content` 本身或 `index` 作为业务逻辑寻址的依据。
- **假设：前端支持 ID 渲染**。假设 Vue 前端组件已经具备或可以轻松适配完全通过 `item_id` 来跟踪和操作剪贴板项目。

## 5. 验证步骤 (Verification Steps)
1. **编译检查**：执行 `cargo check` 和 `npm run build`，确保所有因签名更改导致的编译错误被修复。
2. **数据库验证**：运行应用后，使用 SQLite 客户端检查本地数据库文件，确认 `categories` 和 `pinned_items` 表中不再有 `content` 列。
3. **功能回归测试**：
   - 复制多条不同文本，确保列表正确展示。
   - 快速连续点击不同的条目进行“删除”、“置顶”、“取消置顶”和“填充”操作，验证是否会发生错位或无响应。
   - 重启应用，验证状态（历史、分类、置顶）能否正确从数据库恢复。