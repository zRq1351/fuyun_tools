# 文本剪贴板 `content` 到 `item_id` 切换审计与修复计划

## 1. 概述 (Summary)
本次审计按照文本剪贴板的功能链路（监听、列表获取、操作、存储等）对前后端代码进行了全面梳理。虽然之前的重构在数据库层（`categories`, `pinned_items`）成功移除了冗余的 `content` 字段并启用了 `item_id`，但**核心状态模型和前后端的解耦并没有完全落实**。系统中依然存在对 `content` 字符串和数组 `index` 的深度依赖，并由此引发了严重的并发竞态 Bug 和前端 UI 状态错位风险。本计划旨在彻底清理这些技术债务，完成向轻量级 `item_id` 的完全切换。

## 2. 当前状态与 Bug 分析 (Current State & Identified Bugs)

按照功能链路梳理，目前存在以下未完全切换的遗留问题和严重 Bug：

### 2.1 后端内存状态与并发操作 (Rust)
- **遗留依赖**：`src-tauri/src/utils/clipboard.rs` 中的核心状态 `history` 依然是 `Arc<Mutex<Vec<String>>>`，并未重构为包含 `id` 的结构体。同时，`exact_index_cache` 依然将 Hash 映射到不稳定的数组索引（`usize`）。
- **🔥 严重 Bug（TOCTOU 并发漏洞）**：在执行删除 (`remove_from_history`) 和置顶 (`promote_to_top_async`) 等操作时，系统先调用 `find_index_by_id` 获取 `index`（期间释放了锁），然后重新获取锁并通过该 `index` 操作数组。如果在这两次获取锁的微小间隙内，剪贴板监听器刚好插入了新数据，数组发生移位，原 `index` 将指向错误的数据。这会导致**静默删除或置顶错误的剪贴板条目**，甚至引发越界崩溃（Panic）。

### 2.2 前端列表渲染与焦点追踪 (Vue/JS)
- **遗留依赖**：在 `ClipboardList.vue` 和 `useClipboardHistory.js` 中，前端严重依赖后端返回的 `position`（映射为组件内的 `index`）来追踪用户的选中状态（`selectedIndex`）。
- **🐛 潜在 Bug（UI 选区错位与焦点跳跃）**：由于 `selectedIndex` 存储的是数字索引，当列表发生过滤（搜索/分类切换）、重排序（置顶）或后端推送新数据导致数组长度和顺序变化时，原条目的索引会改变。此时选中的高亮框会瞬间**跳跃到另一个无关的条目上**，导致用户的键盘导航（上下键）完全错乱。

### 2.3 前后端 IPC 通信 (JS)
- **遗留依赖**：虽然 Rust 端的指令已清理，但前端 `src/services/ipc.js` 的接口封装（如 `removeItem`, `setItemPinned`, `promoteItem`, `selectAndFill`）依然保留了对 `index` 参数的接收和向下传递，导致 API 签名混乱。

## 3. 拟议更改 (Proposed Changes)

### Step 1: 重构后端内存模型并修复竞态漏洞
**目标文件**：`src-tauri/src/utils/clipboard.rs`
- **更改内容**：
  1. 定义 `ClipboardItem` 结构体：`pub struct ClipboardItem { pub id: String, pub content: String }`。
  2. 将核心状态 `history: Arc<Mutex<Vec<String>>>` 修改为 `history: Arc<Mutex<Vec<ClipboardItem>>>`。
  3. **修复 TOCTOU 漏洞**：在 `remove_from_history` 和 `promote_to_top` 等方法中，**必须在同一个互斥锁（MutexGuard）的作用域内**，直接通过 `item.id == target_id` 查找到位置并立即执行 `remove`。严禁跨锁传递数组索引。
  4. 废弃或重写 `exact_index_cache`，使其直接映射到 `id` 或彻底移除（在内存中遍历 `id` 足够快）。

### Step 2: 修复前端 UI 选区错位 Bug
**目标文件**：`src/pages/clipboard/composables/useClipboardHistory.js`, `src/pages/clipboard/components/ClipboardList.vue`, `src/pages/clipboard/App.vue`
- **更改内容**：
  1. 将状态变量 `selectedIndex` 彻底重构为 `selectedItemId`（存储字符串 ID 而不是数字）。
  2. 更新上下键导航逻辑（`moveSelection`）：根据当前的 `selectedItemId` 找到其在数组中的位置，加减偏移量后，将新的条目 ID 赋给 `selectedItemId`。
  3. 更新 `ClipboardList.vue` 中的高亮判断逻辑：`:class="{ selected: selectedItemId === entry.id }"`。

### Step 3: 彻底清理前端 IPC 接口
**目标文件**：`src/services/ipc.js`
- **更改内容**：
  1. 移除 `normalizeOptionalIndex` 工具函数。
  2. 将 `removeItem`, `setItemPinned`, `promoteItem`, `selectAndFill` 等方法的签名从 `(index, itemId)` 彻底简化为 `(itemId)`，并清理对应的 `invoke` 传参。
  3. 全局搜索并修复前端组件中调用这些 IPC 方法时遗留的冗余参数。

## 4. 假设与决策 (Assumptions & Decisions)
- **决策：后端内存保留 Content**。尽管我们的目标是切换为 `item_id`，但为了保证搜索和列表渲染的极速响应，后端内存中依然需要保留文本内容（即采用 `{id, content}` 结构体），但**业务逻辑的寻址和唯一标识必须 100% 依赖 `id`**。
- **决策：直接在锁内遍历查找**。考虑到剪贴板历史记录通常在几百条以内，在锁内直接通过 `.iter().position(|item| item.id == target_id)` 查找的性能损耗极小（微秒级），这比维护复杂的外部索引缓存更安全、更不易出错。

## 5. 验证步骤 (Verification Steps)
1. **编译检查**：确保 Rust 和前端代码均无类型和签名报错。
2. **并发漏洞验证**：在应用后台疯狂复制文本（模拟高频监听写入）的同时，在前端界面手动点击某一条目的“置顶”或“删除”，确认操作依然精准落在目标条目上，未发生错位或崩溃。
3. **前端 UI 验证**：
   - 选中列表中的第 3 项。
   - 切换到一个分类（列表被过滤），确认选中高亮框依然停留在该条目上（如果该条目存在于分类中）。
   - 按下 `Up/Down` 箭头键，确认焦点能在新的过滤列表中正常上下移动。