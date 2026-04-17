# 前端 Vue 代码性能优化与 Bug 修复计划

## 1. 现状分析与问题定位

在深度分析了前端 Vue 相关的组件和逻辑后，发现以下几个核心的性能瓶颈和 Bug：

1. **Scroll 事件导致的布局抖动 (Layout Thrashing)**：
   - **涉及文件**：`ClipboardList.vue`, `ImageClipboardList.vue`, 对应的 `App.vue`
   - **问题**：列表组件中的 `@scroll` 事件未加任何节流，直接高频触发 `emit('content-scroll')`。在父组件的 `tryLoadMoreByScroll` 处理函数中，会同步读取 `scrollWidth`、`clientWidth` 和 `scrollLeft`。这种高频的 DOM 布局属性读取会导致浏览器强制同步重排（Synchronous Layout），严重影响滚动的帧率（FPS）。

2. **`v-for` 的 `:key` 反模式导致 DOM 重建**：
   - **涉及文件**：`ClipboardList.vue`
   - **问题**：文本剪贴板列表使用 `:key="entry.index"` 作为循环的键值。当有新的剪贴板记录插入到列表头部时，所有已有元素的 `index` 都会发生改变，导致 Vue 无法复用现有的 DOM 元素，而是销毁并重建整个列表的 DOM，开销极大。

3. **`v-memo` 缓存失效与依赖错误**：
   - **涉及文件**：`ClipboardList.vue`
   - **问题 1**：`v-memo` 数组中包含了全局的 `selectedIndex`，这意味着当用户选中任意一项时，`selectedIndex` 的改变会使列表中**所有**元素的 `v-memo` 缓存全部失效，失去了局部更新的意义。
   - **问题 2**：`v-memo` 中调用了 `getItemCategory(entry.id)`，而在文本剪贴板的数据结构中，`entry.id` 实际上是 `undefined`。正确的唯一标识应当是 `entry.content`。

4. **分类标签渲染 Bug**：
   - **涉及文件**：`ClipboardList.vue`
   - **问题**：DOM 渲染分类标签时使用了 `{{ getItemCategory(entry.id) }}`，由于 `entry.id` 为 `undefined`，导致文本剪贴板的分类可能无法正确渲染。

## 2. 拟定修改方案

### 2.1 优化 Scroll 事件分发 (使用 rAF 节流)
- **目标文件**：`src/pages/clipboard/components/ClipboardList.vue` 和 `src/pages/image_clipboard/components/ImageClipboardList.vue`
- **修改细节**：
  在 `setup` 中增加 `scrollRafId` 变量，使用 `requestAnimationFrame` 对 `handleScroll` 进行节流，保证每帧最多触发一次 `content-scroll` 事件。
  ```javascript
  let scrollRafId = 0
  const handleScroll = () => {
    if (!scrollRafId) {
      scrollRafId = requestAnimationFrame(() => {
        emit('content-scroll')
        scrollRafId = 0
      })
    }
  }
  ```

### 2.2 修复文本列表的 `v-for` Key
- **目标文件**：`src/pages/clipboard/components/ClipboardList.vue`
- **修改细节**：
  将 `<div v-for="(entry, index) in visibleHistory" :key="entry.index">` 
  修改为 `:key="entry.content"`。利用文本内容的唯一性（在现有逻辑中已被去重），确保在头部插入新元素时，已有 DOM 元素能够被 Vue 正确复用。

### 2.3 修复 `v-memo` 依赖与缓存失效
- **目标文件**：`src/pages/clipboard/components/ClipboardList.vue`
- **修改细节**：
  将 `v-memo` 的依赖数组：
  `[entry.content, entry.index, selectedIndex, getItemCategory(entry.id), isPinned(entry.content), entry.snippet]`
  修改为：
  `[entry.content, entry.index, selectedIndex === entry.index, getItemCategory(entry.content), isPinned(entry.content), entry.snippet]`。
  这样当 `selectedIndex` 改变时，只有被选中和被取消选中的两个 DOM 会触发重新渲染。

### 2.4 修复分类渲染的 Bug
- **目标文件**：`src/pages/clipboard/components/ClipboardList.vue`
- **修改细节**：
  将 `<div class="category-chip">{{ getItemCategory(entry.id) }}</div>`
  修改为 `<div class="category-chip">{{ getItemCategory(entry.content) }}</div>`。

## 3. 假设与决策
- **假设**：根据 `useClipboardHistory.js` 的逻辑，文本剪贴板的 `history` 数组是以文本内容 `content` 作为唯一标识去重的（`insertLocalIncomingContent` 保证了唯一性），因此使用 `entry.content` 作为 `key` 是安全的。
- **决策**：针对滚动事件，选择使用 `requestAnimationFrame` 进行节流，这是因为读取 `scroll` 属性的频率只需要匹配屏幕刷新率即可，既能保证流畅度，又不会丢失滚动意图。

## 4. 验证步骤
1. 修改完成后，启动前端开发服务器或构建桌面端应用。
2. 打开文本剪贴板，复制一段新文本，观察列表是否平滑地将新项目推入顶部，而不出现整体闪烁（DOM 重建）。
3. 使用键盘上下方向键快速切换选中项，观察性能和响应速度（此时 `v-memo` 将大幅降低渲染开销）。
4. 快速横向滚动文本和图片剪贴板列表，观察滚动是否流畅，不会因频繁触发 `loadMore` 意图而导致卡顿。
5. 为某条文本记录设置分类，检查分类标签是否能正确显示。