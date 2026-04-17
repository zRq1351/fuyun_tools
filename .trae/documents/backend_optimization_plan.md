# Rust 后端性能优化与 Bug 修复计划

## 1. 摘要 (Summary)
针对前端/后端全栈分析报告中指出的 Rust 后端问题，本计划旨在修复由于 `Vec::clear()` 导致的内存泄漏、未设置停止条件的监控线程泄漏、冗余等待线程造成的 CPU 浪费，以及文本相似度计算中的高时间/空间复杂度瓶颈。优化将使后端的内存占用更加稳定，降低后台资源消耗。

## 2. 现状分析 (Current State Analysis)
- **内存膨胀**: `image_clipboard.rs` 中的 LRU 淘汰机制仅调用了 `clear()`，未释放 `capacity`，导致内存只增不减。
- **线程泄漏**: `start_memory_monitor` 创建的无限循环监控线程缺少退出条件，导致每次重新实例化时泄漏旧线程。
- **并发与 CPU 浪费**: 
  - `wgc_capture.rs` 中的屏幕录制监控线程使用 30ms 的高频 `sleep` 轮询。
  - `native_wasapi.rs` 的 `start_process_loopback_wavs` 额外生成了一个无意义的阻塞线程来等待和 Join 子线程。
- **算法瓶颈**: `text_utils.rs` 的 `calculate_text_similarity` 使用了 $O(M \times N)$ 空间复杂度的二维数组；`ngram_similarity` 在循环中高频分配 `String`。

## 3. 拟议变更 (Proposed Changes)

### 3.1 修复图片剪贴板内存泄漏
- **文件**: `src-tauri/src/utils/image_clipboard.rs`
- **操作**: 
  - 在 `enforce_full_res_cache_budget_lru` 函数中，将 `history[idx].rgba_bytes.clear();` 替换为 `history[idx].rgba_bytes = Vec::new();`，确保底层堆内存被真正归还给系统。

### 3.2 修复后台监控线程泄漏
- **文件**: `src-tauri/src/utils/image_clipboard.rs`
- **操作**: 
  - 在 `start_memory_monitor` 的线程循环内部开头添加退出检查：`if Arc::strong_count(&current_memory_usage) <= 1 { break; }`。当 `ImageClipboardManager` 的实例被全部释放时，监控线程将自动感知并安全退出。

### 3.3 消除冗余的音频阻塞线程
- **文件**: `src-tauri/src/features/recording/native_wasapi.rs`
- **操作**:
  - 将 `WasapiCaptureHandle` 结构体中的 `join: Option<std::thread::JoinHandle<()>>` 修改为 `joins: Vec<std::thread::JoinHandle<()>>`。
  - 修改 `stop` 方法，遍历并 `join` 所有的线程句柄。
  - 在 `start_process_loopback_wavs` 函数中，直接将收集到的 `workers` 传递给 `WasapiCaptureHandle::joins`，删除专门用于等待 `thread_stop` 和回收线程的额外空转线程。
  - 同步更新该文件中其他返回 `WasapiCaptureHandle` 的函数，使其返回 `joins: vec![handle]`。

### 3.4 降低屏幕录制轮询频率
- **文件**: `src-tauri/src/features/recording/wgc_capture.rs`
- **操作**:
  - 将 `start_window_capture_to_mp4` 结尾的轮询等待间隔从 `thread::sleep(Duration::from_millis(30))` 增加至 `100ms`，以减少约 70% 的无意义上下文切换。

### 3.5 优化文本相似度算法复杂度
- **文件**: `src-tauri/src/utils/text_utils.rs`
- **操作**:
  - **`calculate_text_similarity` 降维**: 将原有的 `vec![vec![0; len2 + 1]; len1 + 1]` 二维 DP 矩阵优化为使用单行滚动数组 `let mut dp = vec![0; len2 + 1];`，将空间复杂度从 $O(M \times N)$ 降至 $O(N)$，消除长文本比较时海量的内存分配。
  - **`ngram_similarity` 零分配优化**: 利用 `chars.windows(n)` 配合 `HashSet<&[char]>`，代替原先在循环内不断构建和拼接 `String` 的操作，消除高频动态内存分配带来的 CPU 瓶颈。

## 4. 假设与决策 (Assumptions & Decisions)
- **决策**: `ImageClipboardManager` 现有的生命周期依赖于 `Arc` 引用计数，因此使用 `Arc::strong_count` 检查是一种低侵入性且安全可靠的线程退出机制。
- **决策**: 屏幕录制的结束检测延迟从 30ms 放宽至 100ms，这对用户的实际停止体验没有感知影响，但能显著降低后台的轮询功耗。

## 5. 验证步骤 (Verification Steps)
1. **编译验证**: 执行 `cargo check` 确保所有结构体修改、方法调用以及生命周期（如 `HashSet<&[char]>`）符合 Rust 的借用检查。
2. **算法逻辑验证**: 编写或运行已有的文本相似度单元测试，确保一维 DP 数组计算出的 LCS 长度与原二维数组结果完全一致。
3. **运行验证**: 模拟多次开启和停止录制，以及文本、图片的大量复制粘贴操作，确保应用稳定运行无 OOM 风险。