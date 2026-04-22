已完成以下修改以满足您的要求：

1. **实现录制工具栏的可拖拽及特定图标拖拽：**
   - 在 `App.vue` 中引入了 `GripVertical` 拖拽图标。
   - 将原有的 `.bar` 及 `.bar.bar-collapsed` 的拖拽属性 (`-webkit-app-region: drag`) 和鼠标样式移除，替换为 `no-drag`。
   - 在左侧新增了带有 `drag-handle` 类的拖拽热区，设置 `cursor: move` 及 `-webkit-app-region: drag`，确保现在只有按住拖拽图标时才能拖动工具栏。
   - 配合新增的拖拽图标，将工具栏胶囊模式的基础宽度从 210px 增加至 226px，防止挤压内部文本。

2. **工具栏只在唤出来时屏幕顶部居中：**
   - 在 `App.vue` 中触发展开/折叠设置面板的动画重算逻辑 (`syncCapsuleLayout`) 里，将调用 `RecordingService.resizeToolbar` 的 `recenter` 参数全部改为 `false`。
   - 这样修改后，工具栏仅在后端首次被拉起（`show_recording_toolbar` 及 `toggle_recording_from_shortcut`）时在屏幕顶部居中，而在此之后无论怎么展开折叠或者状态变更，都不会强制跳转回屏幕中央，完美保留用户自定义的拖拽位置。