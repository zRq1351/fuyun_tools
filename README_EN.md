# 🚀 fuyun_tools

[中文](README.md) | [English](README_EN.md)

fuyun_tools is a desktop productivity tool running in the system tray, focused on three things:

- Better clipboard history management
- AI text selection translation/explanation on Windows
- Fast screenshot capture and image OCR on Windows
- Screen recording with audio capture on Windows

Core positioning:

- One hotkey workflow to manage both text and image clipboard history
- One selection workflow to translate, explain, and copy in-place
- One screenshot + OCR workflow from capture to text extraction
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
- Supports large-image preview for detailed viewing
- Image history and categories are persisted locally and available after restart

### 🔎 Image OCR (Windows)

- Pinned image window supports right-click OCR recognition
- Recognition results are shown in a dedicated OCR text window for copy/reuse
- OCR is currently available on Windows only

### 🎥 Screen Recording (Windows)

- Adds a recording capsule toolbar with hotkey trigger and one-click start/pause/resume/stop
- Supports system output audio and microphone device selection (native WASAPI pipeline)
- Supports FPS, video bitrate, audio bitrate, and cursor capture settings
- On first enable, automatically checks `ffmpeg.exe`; if missing, downloads on demand with real-time progress
- When recording is disabled, the capsule shows a direct disabled state to prevent accidental actions

### 🔤 AI Text Selection Assistant (Windows)

- Supports drag, double-click, and triple-click selection scenarios
- Automatically shows a selection toolbar for translate/explain/copy
- Result windows support streaming output for faster feedback
- Result windows support one-click write-back to the source app (copy + auto paste)

### 🤖 AI Service Configuration

- Built-in DeepSeek / Qwen (`qwen`) / Xiaomi Mimo (`xiaomimimo`) providers
- Supports adding any OpenAI-compatible custom provider
- Supports deleting custom providers directly in dropdown options
- API keys are stored in the system credential manager (keyring)

### ⚙️ System Integration

- Runs in the system tray and supports auto-start
- Global hotkeys for text clipboard / image clipboard / screenshot
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
| Image OCR            | ✅       | ❌     | ❌     |
| AI Text Selection    | ✅       | ❌     | ❌     |
| Screen Recording     | ✅       | ❌     | ❌     |
| Tray & Hotkeys       | ✅       | ✅     | ✅     |

> Note: AI text selection, image OCR, and screen recording are currently implemented only on Windows.

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

1. **Launch App**: Run fuyun_tools; the icon appears in the system tray
2. **Use Global Hotkeys**:
    - Text clipboard: Windows `Ctrl+Shift+Z` / macOS `Cmd+Shift+Z`
    - Image clipboard: Windows `Ctrl+Shift+X` / macOS `Cmd+Shift+X`
    - Screenshot: Windows `Ctrl+Shift+S` / macOS `Cmd+Shift+S`
3. **Configure AI Service**: Go to 「Settings → AI Settings」
    - Choose built-in provider (DeepSeek/Qwen/Mimo) or add custom OpenAI-compatible endpoint
    - Enter API URL, model name, and key
    - Click "Test Connection" to validate configuration
4. **Try Text Selection**: Select text in any Windows application
    - The 「Selection Toolbar」automatically pops up
    - Click [Translate] or [Explain]
    - View streaming results in the 「Result Window」with one-click write-back
5. **Try Screen Recording**: Go to 「Settings → Recording」, enable recording, then use the recording capsule
    - On first enable, ffmpeg is auto-checked and downloaded on demand if missing
    - You can choose system/microphone audio devices and adjust recording parameters in capsule settings

---

## 🧭 Usage Guide

### Clipboard Window

- **Navigation**: `← / →` to switch items, `Enter` to fill
- **Scroll Control**: Use mouse wheel to navigate through history
- **AI Actions**: Press `T` to translate or `E` to explain the selected item
- **UI Interaction**: Drag the "Raise" handle to adjust window's vertical position
- **Advanced Feature**: Click the blank area between the "Raise" handle and search box to expand/collapse the AI
  language settings panel

### Image Clipboard Window

- **Select Image**: Click a thumbnail card to select
- **Quick Paste**: Double-click a card to paste directly at cursor position
- **View Large Image**: Click the top-right "Fullscreen" button for large-image preview
- **Manage History**: Click the top-right "Delete" icon to remove record
- **Bulk Operations**: Use `← / →` to browse images, `Enter` to paste current item

### Pinned Image OCR (Windows)

- **How to Trigger**: Right-click in the pinned image window and choose OCR
- **Result Output**: Recognition text opens in a dedicated OCR text window
- **Platform Scope**: Currently supported on Windows only

### Limit Strategy (Text + Image)

- **Access Path**: Settings → Clipboard → Limit Strategy
- **Smart Protection**: When "Limit Ungrouped Items Only" is enabled, all categorized/pinned items are protected from
  capacity limits
- **Full Cleanup**: When disabled, the history limit applies to all records (including important items)

### Selection Toolbar (Windows)

- **Trigger Condition**: Automatically appears upon completing text selection in any application
- **Core Actions**: Provides three primary buttons: [Translate], [Explain], [Copy]
- **Instant Feedback**: Clicking an action immediately displays streaming AI output in the "Result Window"
- **UI Control**: Click outside the toolbar or press ESC to dismiss

### AI Configuration Tips

- **Secure Storage**: API keys are encrypted and stored in the system credential manager (keyring), never saved as plain
  text
- **Connection Validation**: Always click "Test Connection" to verify configuration validity before saving
- **Custom Extension**: Supports adding any OpenAI-compatible interface, enabling flexible integration with private
  model deployments
- **Prompt Templates**: Modify translation/explanation system prompts in settings to achieve personalized output

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
