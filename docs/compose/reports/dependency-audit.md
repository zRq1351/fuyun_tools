# 依赖审计报告

**项目:** fuyun_tools v0.8.1  
**日期:** 2026-07-23  
**审计员:** MiMoCode Agent  

---

## 执行摘要

本次审计发现前端依赖存在**8个安全漏洞**（已修复），Rust依赖需要更新**275个包**（已完成）。项目模块结构清晰，耦合度适中。

---

## 1. 前端依赖审计 (package.json)

### 1.1 安全漏洞（已修复）

执行 `npm audit fix` 后，所有安全漏洞已修复：

| 包名 | 漏洞 | 严重程度 | 状态 |
|------|------|---------|------|
| vite | NTLMv2 hash disclosure | 高 | ✅ 已修复 (更新到8.1.5) |
| vite | `server.fs.deny` bypass | 高 | ✅ 已修复 |
| marked | OOM DoS via Infinite Recursion | 高 | ✅ 已修复 |
| dompurify | Multiple XSS vulnerabilities | 高 | ✅ 已修复 |
| lodash | Code Injection via `_.template` | 高 | ✅ 已修复 |
| lodash-es | Code Injection via `_.template` | 高 | ✅ 已修复 |
| brace-expansion | DoS via exponential-time expansion | 高 | ✅ 已修复 |
| immutable | DoS via trie overflow and hash collision | 高 | ✅ 已修复 |
| postcss | XSS via Unescaped </style> | 中 | ✅ 已修复 |

### 1.2 过时的依赖

| 包名 | 当前版本 | 最新版本 | 优先级 |
|------|---------|---------|--------|
| @tauri-apps/api | 2.10.1 | 2.11.1 | 中 |
| @tauri-apps/cli | 2.10.1 | 2.11.4 | 中 |
| @tauri-apps/plugin-dialog | 2.7.0 | 2.7.2 | 低 |
| @tauri-apps/plugin-opener | 2.5.3 | 2.5.4 | 低 |
| @vitejs/plugin-vue | 6.0.5 | 6.0.8 | 低 |
| @vueuse/core | 14.2.1 | 14.3.0 | 低 |
| element-plus | 2.13.7 | 2.14.3 | 中 |
| eslint | 10.3.0 | 10.7.0 | 低 |
| eslint-plugin-vue | 10.9.1 | 10.10.0 | 低 |
| sass | 1.99.0 | 1.101.3 | 低 |
| unplugin-vue-components | 32.0.0 | 32.1.0 | 低 |
| vue | 3.5.32 | 3.5.40 | 中 |
| vue-i18n | 11.1.10 | 11.4.7 | 中 |

---

## 2. Rust依赖审计 (Cargo.toml)

### 2.1 已更新的依赖

执行 `cargo update` 更新了**275个包**到最新兼容版本，包括：

- tauri: 2.10.2 → 2.11.5
- tauri-build: 2.5.5 → 2.6.3
- serde: 1.0.228 → 1.0.229
- serde_json: 1.0.149 → 1.0.151
- tokio: 1.52.1 → 1.53.1
- reqwest: 0.13.3 → 0.13.4
- 以及其他200+间接依赖

### 2.2 过时的直接依赖

| 包名 | 当前版本 | 最新兼容版本 | 优先级 |
|------|---------|-------------|--------|
| log | 0.4.29 | 0.4.33 | 低 |
| regex | 1.11.1 | 1.13.1 | 中 |
| lru | 0.18.0 | 0.18.1 | 低 |
| tokio | 1.52.1 | 1.53.1 | 中 |
| reqwest | 0.13.3 | 0.13.4 | 低 |
| sysinfo | 0.39.2 | 0.39.6 | 低 |
| xxhash-rust | 0.8.15 | 0.8.18 | 低 |
| chrono | 0.4.43 | 0.4.45 | 低 |

### 2.3 安全漏洞

由于 `cargo-audit` 未安装，无法自动检查Rust依赖的安全漏洞。建议：

1. 安装cargo-audit: `cargo install cargo-audit`
2. 运行: `cargo audit`

---

## 3. 构建兼容性问题

### 3.1 Rust构建问题

`ocr-rs` 依赖在使用 `build-mnn-from-source` 特性时构建失败：
- 错误原因：CMake构建MNN库时失败
- 影响：无法运行 `cargo check` 验证Rust代码
- 建议：需要修复MNN构建环境或更新ocr-rs依赖

### 3.2 前端构建验证

✅ 前端构建成功（`npm run build` 完成，无错误）

---

## 4. 模块结构分析

### 4.1 前端Vue组件结构

```
src/
├── components/          # 3个共享组件
├── composables/         # 4个组合式函数
├── pages/              # 16个页面模块（多页面架构）
├── services/           # 1个IPC服务文件
├── utils/              # 6个工具模块
└── locales/            # 国际化翻译
```

**评估:** 结构清晰，按功能模块划分，符合Vue 3最佳实践。

### 4.2 Rust后端模块组织

```
src-tauri/src/
├── core/              # 8个模块（状态、配置、错误处理）
├── features/          # 4个模块（鼠标、录制、截图、文本选择）
├── services/          # 16个模块（AI、剪贴板、OCR、启动器等）
├── ui/                # 14个模块（命令、托盘、窗口管理）
├── utils/             # 15个模块（备份、数据库、图像、设置）
├── lib.rs            # 主入口（725行）
└── main.rs           # 入口点
```

**评估:** 模块划分合理，职责分离清晰。

### 4.3 模块间耦合度

**低耦合（良好）:**
- 前端页面是独立的HTML入口点
- 服务层与UI命令清晰分离
- 功能模块（录制、截图等）模块化

**中等耦合（可接受）:**
- `AppState` 在所有层（核心、服务、UI）中被访问
- `lib.rs` 直接协调多个模块

**高耦合（需关注）:**
- `ui/commands.rs` 导入几乎所有其他模块
- 前端 `ipc.js` 在一个地方定义所有IPC命令（必要但较大）

---

## 5. 依赖安全性评估

### 5.1 前端依赖安全

| 风险类别 | 状态 | 说明 |
|---------|------|------|
| XSS漏洞 | ✅ | dompurify漏洞已修复 |
| DoS漏洞 | ✅ | marked、brace-expansion、immutable漏洞已修复 |
| 路径遍历 | ✅ | vite Windows路径漏洞已修复 |
| 代码注入 | ✅ | lodash/lodash-es漏洞已修复 |

### 5.2 Rust依赖安全

| 风险类别 | 状态 | 说明 |
|---------|------|------|
| 已知漏洞 | ❓ | 无法自动检测，需安装cargo-audit |
| 过时依赖 | ✅ | 275个依赖已更新 |

---

## 6. 总结

### 已完成的更新

1. ✅ 执行 `npm audit fix` 修复前端安全漏洞
2. ✅ 更新 vite 到 8.1.5 修复Windows路径安全漏洞
3. ✅ 执行 `cargo update` 更新Rust依赖（275个包）
4. ✅ 前端构建验证通过

### 待解决的问题

1. [ ] 修复 ocr-rs 构建问题（MNN编译失败）
2. [ ] 安装并运行 `cargo-audit` 检查Rust依赖安全
3. [ ] 完整的Rust构建验证（需要修复构建环境后）
4. [ ] 更新前端过时的依赖（非安全相关）
