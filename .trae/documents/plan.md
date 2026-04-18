# 性能优化与潜在 Bug 修复计划

## 1. 概述 (Summary)
本项目计划旨在系统性地修复和优化 Tauri 后端（Rust）与 Vue 3 前端代码库中存在的 10 项关键性能瓶颈与潜在 Bug。这些问题涵盖了前端大列表渲染卡顿、后端高频内存分配、并发队列竞态条件、死锁风险以及截图与录屏的稳定性缺陷。本次更新将全面提升应用的响应速度、内存效率及稳定性。

## 2. 现状分析 (Current State Analysis)
根据此前的代码库探索，项目目前存在以下主要问题：
- **前端性能**：剪贴板列表（`ClipboardList.vue`, `ImageClipboardList.vue`）未使用虚拟滚动，大量 DOM 节点导致内存膨胀和滚动卡顿；同时大数组的排序和过滤在主线程中执行，阻塞 UI 响应。
- **后端历史记录缩容**：清理超过容量的历史记录时，使用了 $O(N)$ 的 `Vec::remove` 和全表索引遍历，导致 $O(N^2)$ 的线程阻塞。
- **并发与资源管理**：图片剪贴板队列存在 TOCTOU 竞态条件导致永久阻塞；WASAPI 实时音频回调中使用了互斥锁进行阻塞式管道写入，存在死锁风险并造成高频的静音缓冲区堆分配。
- **截图模块**：全屏截图时冗余拷贝极大放大峰值内存；并发截图拦截状态未正确返回错误；屏幕边缘区域截图遇到越界时会静默失败。
- **录屏模块**：FFmpeg 进程结束时直接被 `kill()` 导致 MP4/AAC 文件损坏；进程录音时 `VecDeque` 逐字节提取拼装导致极高的 CPU 消耗；目标窗口调整大小时，WGC 录像编码器因尺寸不匹配而崩溃。

## 3. 建议的修改 (Proposed Changes)

### 3.1 前端性能优化 (Frontend Optimization)
1. **引入虚拟滚动 (Virtual Scrolling)**
   - **文件**: `/workspace/src/pages/clipboard/components/ClipboardList.vue` 和 `ImageClipboardList.vue`
   - **修改**: 引入 `@vueuse/core` 的 `useVirtualList` 或引入 `vue-virtual-scroller`，将原生的 `v-for` 替换为虚拟列表渲染，确保视口外节点被及时卸载。
2. **将过滤排序逻辑下放到 Rust 后端**
   - **文件**: `/workspace/src/pages/clipboard/composables/useClipboardHistory.js`
   - **修改**: 移除前端 `watch` 中的 `.filter().sort()` 全量比对。修改前端请求参数，将 `keyword` 和 `category` 直接传给 `load_history_page_data_async` 接口，仅接收当前页的数据进行渲染。

### 3.2 后端核心架构优化 (Backend Core & Concurrency)
3. **修复剪贴板历史缩容的 $O(N^2)$ 阻塞**
   - **文件**: `/workspace/src-tauri/src/utils/clipboard.rs` 和 `image_clipboard.rs`
   - **修改**: 将循环 `remove` 替换为单次 $O(N)$ 的 `Vec::retain` 操作。在 `image_clipboard.rs` 中，重构 `signature_index_remove` 以避免每次删除时的全表遍历，改为在 `retain` 后统一重建或批量更新索引字典。
4. **消除图片处理队列的 TOCTOU 竞态死锁**
   - **文件**: `/workspace/src-tauri/src/services/image_clipboard_manager.rs`
   - **修改**: 废弃双锁分离和手动维护的 `Condvar`，改用标准库 `std::sync::mpsc::channel`（或 `crossbeam_channel`）替代 `VecDeque` 传递 `PendingImageTask`，利用其原生的阻塞唤醒机制彻底解决并发唤醒丢失问题。

### 3.3 音视频录制稳定性 (Recording & Audio Fixes)
5. **解除音频回调的 I/O 阻塞与死锁风险**
   - **文件**: `/workspace/src-tauri/src/features/recording/native_wasapi.rs`
   - **修改**: 引入无锁环形缓冲区（RingBuffer）或 `mpsc::sync_channel`。音频回调闭包中仅执行 `try_send` 将采样数据推入通道；启动一个专门的消费线程从通道读取数据并执行 `writer.write_all` 写入 FFmpeg 管道。
6. **消除音频回调的高频内存分配**
   - **文件**: `/workspace/src-tauri/src/features/recording/native_wasapi.rs`
   - **修改**: 对于暂停状态（`!enabled`）的静音数据，使用静态的预分配全局 `ZERO_BUFFER` 或多次写入 `[0u8; 4]` 代替 `let silence = vec![0u8; data.len() * 4];`。
7. **优化进程音频流的逐字节提取**
   - **文件**: `/workspace/src-tauri/src/features/recording/native_wasapi.rs` (第 146-160 行)
   - **修改**: 废除连续 4 次 `queue.pop_front()`。利用 `VecDeque::make_contiguous()` 或 `as_slices()` 以切片方式按 4 字节块直接转化为 `f32`，然后批量 `queue.drain()`。
8. **防止强杀 FFmpeg 导致媒体文件损坏**
   - **文件**: `/workspace/src-tauri/src/features/recording/native_wasapi.rs`
   - **修改**: 在结束录像时，通过关闭 stdin 来向 FFmpeg 发送 EOF 信号，将 `try_wait` 超时时间从 5 秒延长至 15 秒，确保 FFmpeg 正常写入 MOOV atom 后自行退出。
9. **修复目标窗口调整尺寸导致 WGC 录像崩溃**
   - **文件**: `/workspace/src-tauri/src/features/recording/wgc_capture.rs`
   - **修改**: 在帧处理循环中监控收到的帧尺寸，使用 `image` crate 的 resize 功能（如 `imageops::resize`）将变动后的帧缩放回初始化的目标宽高后再送入编码器；或在初始化时锁死帧尺寸要求。

### 3.4 截图功能内存与逻辑优化 (Screenshot Fixes)
10. **修复全屏截图冗余拷贝与内存峰值放大**
    - **文件**: `/workspace/src-tauri/src/features/screenshot/capture.rs`
    - **修改**: 在 `capture_full_screen` 中避免初始化全尺寸零数组再逐行拷贝的低效逻辑；在 `rgba_to_base64_png` 中直接将 PNG 编码器的输出流对接 Base64 编码流（`base64::write::EncoderWriter`），消除中间的 `Vec<u8>`。
11. **修复并发拦截失效与边缘越界崩溃**
    - **文件**: `/workspace/src-tauri/src/features/screenshot/capture.rs`
    - **修改**: 在 `try_begin_screenshot` (或 `capture_screen_region`) 获取状态失败时立即 `return Err("...")`。对于边缘截取 `end > image.as_raw().len()`，计算有效片段边界进行拷贝，并使用零填充补齐剩余部分。

## 4. 假设与决策 (Assumptions & Decisions)
- **全部修复**：依据用户指令，将覆盖上述发现的所有 10 余个相关性能与逻辑问题。
- **逐步重构**：修改涉及底层多线程同步与通道机制，将优先从 Rust 核心模块的 Bug 入手，随后处理性能问题，最后调整前端 UI 渲染。
- **后向兼容**：修改 SQLite 排序和过滤时，将确保与原前端展示逻辑及分类规则对齐，不破坏现有的剪贴板记录展示。

## 5. 验证步骤 (Verification Steps)
1. **前端滚动**：填充 1000+ 条剪贴板历史，上下滚动验证无卡顿，无严重内存泄露。
2. **音频并发与停止**：开启多路系统声音和麦克风录制，多次迅速暂停、继续和停止，验证不发生死锁，生成的 `.wav` / `.aac` 文件完整可播放。
3. **截图稳定性**：在屏幕最右下角触发区域截图，验证不再出现 "裁剪图片数据长度不匹配" 的崩溃；并发按多次快捷键，验证不再引发状态机紊乱。
4. **内存消耗**：执行 4K 全屏长截图，观测内存峰值是否较之前明显下降。
