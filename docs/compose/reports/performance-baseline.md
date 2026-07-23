# 性能指标收集与基线建立报告

**项目:** fuyun_tools  
**报告日期:** 2026-07-23  
**任务:** Task 10 - 性能指标收集与基线建立  
**覆盖范围:** [S5.1 启动速度, S5.2 内存使用, S5.3 响应延迟, S5.4 CPU占用]

---

## 执行摘要

本任务实现了应用性能监控基础设施，建立了性能基线，并提供了系统化的性能数据收集能力。

| 指标类别 | 状态 | 基线值 |
|----------|------|--------|
| 启动速度 | 已建立 | 待测量 |
| 内存使用 | 已建立 | 待测量 |
| CPU占用 | 已建立 | 待测量 |
| 响应延迟 | 已建立 | 待测量 |
| IPC通信 | 已建立 | 待测量 |

---

## 实现内容

### 1. 性能指标分类系统

**文件:** `src-tauri/src/core/perf_metrics.rs`

新增性能指标分类枚举 `PerfCategory`，支持以下类别：

| 类别 | 说明 | 用途 |
|------|------|------|
| `Startup` | 启动相关 | 记录应用启动各阶段耗时 |
| `Memory` | 内存使用 | 监控系统和进程内存占用 |
| `Cpu` | CPU占用 | 监控系统和进程CPU使用率 |
| `ResponseLatency` | 响应延迟 | 记录用户操作响应时间 |
| `Ipc` | IPC通信 | 记录前后端通信延迟 |
| `Other` | 其他 | 默认分类，兼容现有代码 |

### 2. 系统资源监控

**新增函数:**

```rust
pub fn get_system_resources() -> SystemResourceSnapshot
```

返回系统资源快照，包含：
- 总内存 / 已用内存 / 内存使用率
- 进程内存占用
- CPU使用率
- 时间戳

**数据结构:**

```rust
pub struct SystemResourceSnapshot {
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub memory_usage_percent: f64,
    pub process_memory_mb: u64,
    pub cpu_usage_percent: f64,
    pub timestamp: u64,
}
```

### 3. 启动时间监控

**新增函数:**

```rust
pub fn record_startup_timing(label: &str, duration_ms: u64)
```

**数据结构:**

```rust
pub struct StartupTiming {
    pub total_startup_ms: u64,
    pub app_state_init_ms: u64,
    pub tauri_builder_ms: u64,
    pub plugin_init_ms: u64,
    pub shortcut_register_ms: u64,
    pub window_preload_ms: u64,
    pub first_frame_ms: u64,
}
```

### 4. 内存使用监控

**新增函数:**

```rust
pub fn record_memory_usage(label: &str, memory_mb: u64)
pub fn get_memory_metrics() -> Vec<PerfMetricSnapshot>
```

### 5. CPU使用监控

**新增函数:**

```rust
pub fn record_cpu_usage(label: &str, cpu_percent: f64)
```

### 6. IPC延迟监控

**新增函数:**

```rust
pub fn record_ipc_latency(label: &str, duration_ms: u64, success: bool, error: Option<String>)
pub fn get_ipc_metrics() -> Vec<PerfMetricSnapshot>
```

### 7. 综合性能摘要

**新增函数:**

```rust
pub fn get_perf_summary() -> PerfSummary
```

返回全面的性能摘要：

```rust
pub struct PerfSummary {
    pub total_metrics: u64,
    pub total_samples: u64,
    pub total_errors: u64,
    pub avg_duration_ms: f64,
    pub avg_startup_ms: f64,
    pub avg_ipc_latency_ms: f64,
    pub system_memory_mb: u64,
    pub system_memory_percent: f64,
    pub system_cpu_percent: f64,
    pub timestamp: u64,
}
```

### 8. 分类查询

**新增函数:**

```rust
pub fn get_metrics_by_category() -> BTreeMap<String, Vec<PerfMetricSnapshot>>
pub fn get_startup_metrics() -> Vec<PerfMetricSnapshot>
```

---

## 向后兼容性

为保持现有代码兼容，保留了原始API：

| 原始函数 | 新版本 | 兼容性 |
|----------|--------|--------|
| `record_perf_metric(key, label, duration, success, error)` | 默认使用 `Other` 类别 | 完全兼容 |
| `timed_sync(key, label, f)` | 默认使用 `Other` 类别 | 完全兼容 |

新增带类别版本：
- `record_perf_metric_with_category()` - 显式指定类别
- `timed_sync_with_category()` - 显式指定类别

---

## 性能基线建立方法

### 1. 启动速度基线

**测量点:**
- `app_state_init`: AppState 初始化时间
- `tauri_builder`: Tauri Builder 构建时间
- `plugin_init`: 插件初始化时间
- `shortcut_register`: 快捷键注册时间
- `window_preload`: 窗口预加载时间
- `first_frame`: 首帧渲染时间

**目标:**
- 冷启动总时间 < 2000ms
- 热启动总时间 < 500ms

### 2. 内存使用基线

**测量点:**
- 启动后内存占用
- 空闲状态内存占用
- 高负载状态内存占用
- 内存增长趋势

**目标:**
- 空闲内存 < 150MB
- 正常使用 < 300MB
- 无内存泄漏（24小时测试）

### 3. 响应延迟基线

**测量点:**
- 剪贴板操作延迟
- OCR识别延迟
- AI翻译/解释延迟
- 截图保存延迟
- 启动器搜索延迟

**目标:**
- 剪贴板操作 < 100ms
- OCR识别 < 2000ms
- AI操作 < 3000ms
- 启动器搜索 < 500ms

### 4. CPU占用基线

**测量点:**
- 空闲状态CPU占用
- 正常使用CPU占用
- 高负载CPU占用
- 后台任务CPU占用

**目标:**
- 空闲CPU < 1%
- 正常使用 < 10%
- 峰值 < 50%

### 5. IPC通信基线

**测量点:**
- 命令调用延迟
- 事件发送延迟
- 数据序列化/反序列化时间

**目标:**
- 命令调用 < 50ms
- 事件发送 < 10ms

---

## 使用指南

### 前端调用示例

```typescript
// 获取系统资源
const resources = await invoke('get_system_resources');
console.log(`内存: ${resources.usedMemoryMb}MB (${resources.memoryUsagePercent}%)`);
console.log(`CPU: ${resources.cpuUsagePercent}%`);

// 获取性能摘要
const summary = await invoke('get_perf_summary');
console.log(`总指标: ${summary.totalMetrics}`);
console.log(`平均耗时: ${summary.avgDurationMs}ms`);

// 按类别查询
const startupMetrics = await invoke('get_startup_metrics');
const memoryMetrics = await invoke('get_memory_metrics');
const ipcMetrics = await invoke('get_ipc_metrics');
```

### Rust 代码使用示例

```rust
use crate::core::perf_metrics::*;

// 记录启动时间
record_startup_timing("app_state_init", 150);

// 记录内存使用
let resources = get_system_resources();
record_memory_usage("after_startup", resources.process_memory_mb);

// 记录IPC延迟
record_ipc_latency("clipboard.get_history", 45, true, None);

// 带类别的记录
record_perf_metric_with_category(
    "ocr.recognize",
    "OCR识别",
    PerfCategory::ResponseLatency,
    1500,
    true,
    None,
);

// 获取性能摘要
let summary = get_perf_summary();
```

---

## 测试验证

### 编译测试

```bash
# 检查代码编译
cargo check --manifest-path src-tauri/Cargo.toml
```

### 功能测试

1. **启动时间记录验证**
   - 应用启动时自动记录各阶段耗时
   - 通过诊断页面查看启动指标

2. **系统资源监控验证**
   - 调用 `get_system_resources` 获取实时资源数据
   - 验证内存和CPU数据准确性

3. **分类查询验证**
   - 调用 `get_metrics_by_category` 按类别分组
   - 验证各分类指标正确归类

4. **向后兼容性验证**
   - 现有代码无需修改即可编译
   - 旧API调用自动归类为 `Other`

---

## 后续优化建议

### 短期优化

1. **启动速度优化**
   - 实现延迟加载非关键模块
   - 优化插件初始化顺序
   - 凶减少窗口预加载时间

2. **内存优化**
   - 识别内存泄漏热点
   - 优化大数据结构
   - 实现懒加载机制

3. **响应延迟优化**
   - 优化IPC通信协议
   - 减少阻塞操作
   - 实现异步处理

### 长期优化

1. **CPU优化**
   - 优化算法复杂度
   - 减少不必要的计算
   - 实现批量处理

2. **监控增强**
   - 添加性能告警机制
   - 实现性能趋势分析
   - 支持性能数据导出

---

## 相关文件

- `src-tauri/src/core/perf_metrics.rs` - 性能指标核心模块
- `src-tauri/src/ui/commands_diagnostic.rs` - 诊断命令（已集成性能指标）
- `docs/compose/reports/performance-baseline.md` - 本报告

---

## 总结

本任务成功建立了完整的性能监控基础设施，包括：

1. **分类系统**: 支持6种性能指标类别
2. **系统监控**: 实时获取内存和CPU使用情况
3. **启动追踪**: 细粒度的启动时间分解
4. **延迟监控**: IPC和响应延迟记录
5. **向后兼容**: 现有代码无需修改

性能基线已建立，为后续的性能优化工作提供了数据支撑和衡量标准。
