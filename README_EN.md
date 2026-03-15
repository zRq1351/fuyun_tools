# 🚀 fuyun_tools

[中文](README.md) | [English](README_EN.md)

fuyun_tools is a desktop productivity tool running in the system tray, focused on two things:

- Better clipboard history management
- AI text selection translation/explanation on Windows

Core positioning:

- One hotkey workflow to manage both text and image clipboard history
- One selection workflow to translate, explain, and copy in-place
- One configurable strategy set to balance history limits and key item retention

In addition, this project itself follows an AI full-process development workflow: AI deeply participates in requirement breakdown, solution design, coding implementation, and documentation maintenance.

---

## ✨ Feature Overview

### 📋 Clipboard Management

- Automatically records clipboard history for quick reuse
- Supports search, categorization, deletion, and history size limits
- Supports both keyboard and mouse workflows (arrow keys, enter, wheel)
- Supports in-window AI actions (`T` translate / `E` explain)
- Supports one-click right-click menu actions to translate/explain the current item
- Supports configurable translation target language and explanation language

### 🖼️ Image Clipboard Management

- Automatically detects and stores images copied to the clipboard with thumbnail lists
- Supports image search and category management for quick organization
- Supports double-click image fill back to the currently focused document/app
- Supports fullscreen preview with loading animation for large images
- Image history and categories are persisted locally and available after restart

### 🔤 AI Text Selection Assistant (Windows)

- Supports drag, double-click, and triple-click selection scenarios
- Automatically shows a selection toolbar for translate/explain/copy
- Result windows support streaming output for faster feedback
- Result windows support one-click write-back to the source app (copy + auto paste)

### 🤖 AI Service Configuration

- Built-in DeepSeek / Qwen / Xiaomi Mimo providers
- Supports adding any OpenAI-compatible custom provider
- Supports deleting custom providers directly in dropdown options
- API keys are stored in the system credential manager (keyring)

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

- `← / →`: move selection
- `Enter`: fill selected item
- `Esc`: hide window
- Mouse wheel: scroll list
- `T / E`: translate/explain the currently selected item
- Supports expanding the language settings panel between the “Raise” handle and the search box
- Supports auto-collapsing the language settings panel when clicking elsewhere in the window

### Image Clipboard Window

- Click a card: select image
- Double-click a card: fill image into the currently focused app
- Top-right delete button: remove that image history item
- Top-right fullscreen button: open image fullscreen preview
- `← / →`: switch image; `Enter`: fill current image; `Esc`: close window

### Limit Strategy (Text + Image)

- You can switch limit strategy in `Settings → Clipboard`
- When `Limit Ungrouped Items Only` is enabled, grouped items are protected from auto-removal
- When disabled, the history limit applies to all items

### Selection Toolbar (Windows)

- Appears automatically after text selection
- Click translate/explain to view streaming results
- Click outside to close
- The result window supports one-click write-back to the currently focused app

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

- API keys are stored in the system credential manager (keyring), not written as plain text in config files
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
