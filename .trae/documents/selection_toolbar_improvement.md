# 划词功能优化计划

## 摘要 (Summary)
优化 Windows 平台下划词工具栏的 UI 与交互体验。主要改进点包括：调整触发小图标（魔法棒）及工具栏的出现位置以避免遮挡光标和选中文本，以及支持用户在设置中自定义功能按钮（包括自定义 Prompt 按钮和网页快捷搜索按钮）。

## 现状分析 (Current State Analysis)
1. **触发位置**：目前 `src-tauri/src/features/mouse_listener.rs` 捕获到鼠标释放（MouseUp）坐标后，直接将该坐标作为锚点 (`anchor_pos`) 传递给 `show_selection_toolbar_impl`。`src/pages/selection_toolbar/App.vue` 以该坐标为中心展示工具栏，这容易遮挡用户刚刚选中的文本。
2. **功能按钮**：`App.vue` 中的“翻译”、“解释”、“复制”按钮是硬编码（Hardcoded）的，无法扩展。
3. **后端支持**：目前的 IPC 接口 (`src/services/ipc.js`) 仅提供了 `STREAM_TRANSLATE_TEXT` 和 `STREAM_EXPLAIN_TEXT`，缺少通用的“自定义 Prompt”处理接口。
4. **设置界面**：`src/pages/settings/components/SelectionSettings.vue` 目前没有针对划词工具栏自定义按钮的配置项。

## 提议的更改 (Proposed Changes)

### 1. 触发位置优化 (UI Positioning)
- **修改文件**: `src/pages/selection_toolbar/App.vue` (或 `src-tauri/src/ui/window_manager.rs`)
- **内容**: 
  - 在计算 `newPhysicalX` 和 `newPhysicalY` 时，向 Y 轴或 X 轴增加适当的偏移量（例如 `Y + 15px`, `X + 10px`）。
  - 确保添加偏移量后，窗口依然受当前显示器边界 (Monitor bounds) 的限制，防止越界。

### 2. 配置模型扩展 (Settings Model)
- **修改文件**: `src-tauri/src/utils/settings_model.rs` (及前端对应的状态管理)
- **内容**:
  - 在 `AppSettings` 中新增字段：
    - `selection_custom_prompts: Vec<CustomPrompt>`（包含 `name` 和 `prompt`）。
    - `selection_web_search_enabled: bool`（默认开启）。
    - `selection_web_search_engine: String`（可选 Bing/Google/Baidu 等，默认 Bing）。

### 3. 设置界面扩展 (Settings UI)
- **修改文件**: `src/pages/settings/components/SelectionSettings.vue`
- **内容**:
  - 增加一个“搜索引擎快捷入口”开关和下拉选择框。
  - 增加一个“自定义 AI 按钮”列表管理器，允许用户添加、编辑名称、编辑 Prompt 模板以及删除自定义按钮。

### 4. IPC 与后端 AI 接口支持 (IPC & Backend)
- **修改文件**: `src-tauri/src/ui/commands.rs`, `src-tauri/src/services/ai_services.rs`, `src/services/ipc.js`
- **内容**:
  - 在 Rust 后端新增一个通用流式请求接口 `stream_custom_prompt_text`。
  - 在前端 `AIService` 中暴露该 IPC 方法。
  - 支持将用户的选中文本插入到自定义的 Prompt 模板中。

### 5. 工具栏动态渲染 (Toolbar Dynamic Rendering)
- **修改文件**: `src/pages/selection_toolbar/App.vue`
- **内容**:
  - 组件挂载时（`onMounted`）或唤起时，读取最新的配置。
  - 移除硬编码，改为根据配置动态渲染按钮（保留默认的翻译/解释/复制，后面追加搜索和自定义 Prompt 按钮）。
  - 为网页搜索按钮实现逻辑：使用 `tauri::api::shell::open` 拼接查询参数并调用系统默认浏览器。

## 假设与决策 (Assumptions & Decisions)
- **交互逻辑**：保留现有的“先出现魔法棒小图标，鼠标悬停后展开完整工具栏”的逻辑不变，仅改变初始渲染坐标和按钮列表。
- **自定义 Prompt 机制**：将与现有的翻译/解释共用同一个“结果展示窗口 (`result_display`)”，以保持流式输出体验一致。

## 验证步骤 (Verification Steps)
1. **位置验证**：在任意应用中选中文本，验证弹出的魔法棒图标是否出现在鼠标右下方，不再遮挡当前选中的文字。
2. **搜索功能验证**：在设置中开启网页搜索，划词后点击“搜索”按钮，验证是否成功调用默认浏览器打开搜索引擎并带入选中词。
3. **自定义 Prompt 验证**：
   - 在设置中添加名为“总结”的自定义 Prompt，模板如“请总结以下内容：{text}”。
   - 划词后点击“总结”按钮，验证是否正常呼出结果窗口并流式输出总结内容。
