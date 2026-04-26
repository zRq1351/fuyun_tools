# 文本剪贴板功能增强计划

## 摘要
用户要求放开文本剪贴板的字数限制，并增加对代码格式、Markdown格式和HTML等格式的支持。通过分析代码库并与用户确认，我们将移除前端在调用AI功能时硬编码的5000字截断限制。同时，为了支持富文本格式展示，我们将在剪贴板列表中为每条文本记录增加一个“预览”按钮。点击后，会在独立的弹窗中利用 `marked` 将纯文本渲染为格式化的Markdown/HTML/代码块，既满足了富文本预览需求，又保证了主列表的性能和排版一致性。

## 当前状态分析
1. **字数限制**：在 `src/pages/clipboard/App.vue` 的 `triggerAiFlow` 方法中，存在对超过5000字符的文本进行强制截断的逻辑 (`text.length > 5000`)。在Rust后端或数据库层面，并没有对单条记录的文本长度进行硬性截断。
2. **格式支持**：当前 `ClipboardList.vue` 仅通过纯文本形式 (`{{ entry.content }}`) 展示剪贴板内容，不支持代码高亮或Markdown解析。项目中的 `result_display/App.vue` 已经引入了 `marked` 库，具备成熟的富文本渲染能力。

## 提议的更改

### 1. 移除前端字数截断限制
- **文件**: `src/pages/clipboard/App.vue`
- **修改内容**: 在 `triggerAiFlow` 函数中，删除限制文本长度的逻辑：
  ```javascript
  // 删除以下代码：
  if (text.length > 5000) {
    text = Array.from(text).slice(0, 5000).join('')
  }
  ```
- **目的**: 允许完整的超长文本传递给AI处理窗口（大模型本身的上下文窗口将作为自然限制）。

### 2. 在列表中增加“预览”按钮
- **文件**: `src/pages/clipboard/components/ClipboardList.vue`
- **修改内容**:
  - 在剪贴板项的操作按钮区域（如置顶、删除按钮旁），新增一个“预览”按钮（使用 `<el-icon><View/></el-icon>`）。
  - 为该按钮绑定点击事件，当点击时向父组件触发 `preview` 事件，并传递当前项的 `content`。

### 3. 实现富文本预览弹窗 (Markdown/HTML/代码)
- **文件**: `src/pages/clipboard/App.vue`
- **修改内容**:
  - **引入依赖**: 引入项目中已有的 `marked` 库，并配置安全的 renderer（可参考 `result_display/App.vue` 的实现，开启 `gfm: true` 和 `breaks: true`）。
  - **状态管理**: 增加 `previewVisible` (布尔值) 和 `previewContent` (字符串) 的响应式变量。
  - **UI组件**: 引入 `<el-dialog>` 组件作为预览容器。
  - **渲染逻辑**: 添加一个 `computed` 属性 `renderedPreviewHtml`，利用 `marked.parse(previewContent)` 将文本解析为 HTML，并在弹窗中使用 `v-html` 渲染。
  - **样式适配**: 为预览弹窗添加必要的 CSS 样式，确保 `pre`, `code`, `blockquote`, `table` 等 Markdown 元素能够正确且美观地显示（支持代码块的滚动和高亮背景）。

## 假设与决策
- **假设**: 剪贴板“字数限制”主要指的是前端AI调用时的5000字截断限制，因为底层数据存储并没有长度限制。
- **决策**: 根据用户的选择，采用“增加单独的预览视图”而不是直接在列表中渲染富文本。这样可以避免长篇Markdown或复杂HTML破坏列表的视觉整齐度，并确保列表滚动时的性能。
- **决策**: 复用项目中现有的 `marked` 库来支持 MD/HTML/代码 的渲染，不引入新的第三方依赖，保持应用体积小巧。

## 验证步骤
1. **测试长文本**: 复制一段超过5000字的极长文本，点击“AI翻译”或“AI解释”，验证打开的结果窗口中是否包含了完整的文本内容。
2. **测试格式渲染**: 复制一段包含代码块（如 ` ```javascript ... ``` `）和标题（如 `# Title`）的 Markdown 文本。
3. **测试预览功能**: 在剪贴板历史列表中，点击该记录的“预览”按钮。
4. **验证展示效果**: 确认弹出的对话框中，Markdown 语法被正确解析，代码块有适当的背景和等宽字体样式，HTML标签（如 `<br>` 或 `<b>`）能正常渲染。
