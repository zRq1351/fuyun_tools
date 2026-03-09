# 🚀 fuyun_tools

[中文](README.md) | [English](README_EN.md)

fuyun_tools is a desktop productivity tool running in the system tray, focused on two things:

- Better clipboard history management
- AI text selection translation/explanation on Windows

In addition, this project itself follows an AI full-process development workflow: AI deeply participates in requirement breakdown, solution design, coding implementation, and documentation maintenance.

---

## ✨ Feature Overview

### 📋 Clipboard Management

- Automatically records clipboard history for quick reuse
- Supports search, categorization, deletion, and history size limits
- Supports both keyboard and mouse workflows (arrow keys, enter, wheel)

### 🔤 AI Text Selection Assistant (Windows)

- Supports drag, double-click, and triple-click selection scenarios
- Automatically shows a selection toolbar for translate/explain/copy
- Result windows support streaming output for faster feedback

### 🤖 AI Service Configuration

- Built-in DeepSeek / Qwen / Xiaomi Mimo providers
- Supports adding any OpenAI-compatible custom provider
- Supports deleting custom providers directly in dropdown options
- API keys are stored locally in encrypted form

### ⚙️ System Integration

- Runs in the system tray and supports auto-start
- Global hotkey to open the clipboard window
- Light/Dark theme switching
- Built-in app update support

### 🧠 AI Full-Process Development

- Requirement analysis and task breakdown are AI-assisted
- Implementation, refactoring, and bug fixing are iteratively driven by AI
- Chinese and English documentation is maintained in sync by AI

---

## 🌍 Platform Compatibility

| Feature              | Windows | Linux | macOS |
|----------------------|---------|-------|-------|
| Clipboard Management | ✅       | ✅     | ✅     |
| AI Text Selection    | ✅       | ❌     | ❌     |
| Tray & Hotkeys       | ✅       | ✅     | ✅     |

> Note: AI text selection is currently implemented only on Windows.

---

## 📥 Download & Install

> GitHub Release is recommended.

| Channel | Link                                                              | Notes                 |
|---------|-------------------------------------------------------------------|-----------------------|
| GitHub  | [Latest Release](https://github.com/zRq1351/fuyun_tools/releases) | Recommended           |
| Gitee   | [China Mirror](https://gitee.com/zrq1351/fuyun_tools/releases)    | May lag behind GitHub |

Installation steps:

1. Download the installer for your platform
2. Windows: install `.exe`
3. Linux: use `.AppImage` or `.deb`
4. macOS: use `.dmg`
5. Configuration files are created automatically on first launch

---

## 🚀 Quick Start

1. Launch the app and find it in the system tray
2. Use the default hotkey to open clipboard history
    - Windows: `Ctrl+Shift+Z`
    - macOS: `Cmd+Shift+Z`
3. Configure provider/model/API key in `Settings → AI Settings`
4. On Windows, select text in any app and use the toolbar to translate or explain

---

## 🧭 Usage Guide

### Clipboard Window

- `↑ / ↓`: move selection
- `Enter`: fill selected item
- `Esc`: hide window
- Mouse wheel: scroll list

### Selection Toolbar (Windows)

- Appears automatically after text selection
- Click translate/explain to view streaming results
- Click outside to close

### AI Configuration Tips

- API endpoint must start with `http://` or `https://`
- Model name must be available on your provider
- Test connection before saving

---

## 🧰 Tray Menu

Available in production:

- Auto Start
- Clear History
- Settings
- Exit

Extra entries in development builds:

- Clear Logs
- Open Log Directory

---

## 🔒 Data & Security

- API keys are saved locally in encrypted form
- History and settings are saved locally in app files
- Production builds do not write log files by default (log file features are for development/debugging)

---

## Local Development

### Tech Stack

- Frontend: Vue 3 + Element Plus
- Desktop framework: Tauri 2 + Rust
- AI SDK: async-openai (OpenAI-compatible APIs)

### Common Commands

Frontend build:

```bash
cd src
npm run build
```

Tauri check:

```bash
cd src-tauri
cargo check
```

---

## ❓ FAQ

### 1) Why is AI text selection unavailable on Linux/macOS?

Current versions implement the text-selection pipeline only on Windows. Other platforms are planned.

### 2) How do I delete a custom provider?

In AI provider dropdown options, click the `X` button on the right side of the custom provider.

### 3) Why does closing settings sometimes feel delayed after saving?

The close flow has been optimized for responsiveness. Please update to the latest version.

---

## Demo

[Watch Demo Video](https://www.bilibili.com/video/BV1bwBSBUE8k)
