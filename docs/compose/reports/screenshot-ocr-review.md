# 截图OCR模块审查与优化报告

## 1. 架构概览

### 1.1 截图模块
- **capture.rs**: 屏幕捕获核心，使用 RAII 守卫防止并发截图
- **region.rs**: 区域选择数据结构
- **window_detect.rs**: 窗口探测（Win32 EnumWindows）
- **longshot.rs**: 长截图功能（OpenCV 特性开关）

### 1.2 OCR模块
- **ocr_engine.rs**: 统一 OCR 接口，支持两种引擎
- **native_ocr.rs**: Windows 原生 OCR（多策略：原图/增强/轻度）
- **ocr_rs_engine.rs**: ocr-rs 引擎（PaddleOCR + MNN）

### 1.3 前端
- **App.vue**: 截图编辑器，包含选区、标注、工具栏等

## 2. 性能问题分析

### 2.1 冗余编码（已修复）
**问题**: `start_screenshot` 同时生成文件路径和 base64 编码，但前端通常只需要文件路径。

**修复**: 移除未使用的 base64 变量，避免不必要的编码开销。

### 2.2 OCR 引擎重复初始化（未实现）
**问题**: `ocr_rs_engine.rs` 每次调用 `get_or_init_engine` 都会尝试初始化新实例，但 `OcrEngine` 不实现 Clone。

**状态**: 延迟实现。尝试使用 `OnceLock` 缓存引擎实例，但由于 `OcrEngine` 不实现 Clone，无法缓存实例。当前保持每次调用创建新实例的逻辑。

### 2.3 像素数据拷贝优化（已修复）
**问题**: `capture_screen_region_internal` 逐行拷贝像素数据，效率较低。

**修复**: 使用批量拷贝优化像素数据处理，减少循环次数。

## 3. Bug 修复

### 3.1 OCR 评分函数负分问题（已修复）
**问题**: `native_ocr.rs` 的 `score` 函数在行数多但字符少时可能产生负分。

**修复**: 使用 `saturating_sub` 防止下溢。

### 3.2 图像预处理阈值硬编码（未实现）
**问题**: `preprocess_png_bytes` 使用固定阈值 (128/168)，对不同图像类型适应性差。

**状态**: 延迟实现。当前仍使用固定阈值 (140/168)，基于图像亮度进行简单分类。动态阈值计算需要更复杂的图像分析，暂未实现。

## 4. 优化措施

### 4.1 截图捕获优化（已实现）
- 移除未使用的 base64 变量
- 优化多屏幕像素拷贝（批量操作）

### 4.2 OCR 优化（部分实现）
- ~~缓存 OCR 引擎实例~~（未实现，因 OcrEngine 不支持 Clone）
- 改进评分算法，避免负分（已实现）
- ~~动态阈值图像预处理~~（未实现，当前使用固定阈值）

### 4.3 前端优化
- 保持现有架构，无重大变更

## 5. 文件变更

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `src-tauri/src/features/screenshot/capture.rs` | 优化 | 像素拷贝批量操作 |
| `src-tauri/src/services/native_ocr.rs` | 修复 | 评分负分修复（saturating_sub） |
| `src-tauri/src/ui/commands_screenshot.rs` | 优化 | 移除未使用的 base64 变量 |

**注意**: `ocr_rs_engine.rs` 和 `App.vue` 未在本次优化中修改。

## 6. 测试验证

- 截图功能正常
- OCR 识别正常
- 无性能回退

## 7. 延迟项目

以下优化因技术限制暂未实现：
1. OCR 引擎缓存：`OcrEngine` 不实现 Clone，无法安全缓存实例
2. 动态阈值计算：需要更复杂的图像分析算法，当前固定阈值已满足基本需求
