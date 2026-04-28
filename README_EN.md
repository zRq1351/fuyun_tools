# 🚀 fuyun_tools

[中文](README.md) | [English](README_EN.md)

![Version](https://img.shields.io/badge/version-0.6.78-blue)
![Platform](https://img.shields.io/badge/platform-Windows-lightgrey)
![License](https://img.shields.io/badge/license-GPL--2.0-green)

fuyun_tools is a desktop productivity tool running in the system tray, focused on four things:

- **Clipboard Management**: Efficiently record and manage text and image clipboard history
- **AI Text Selection Assistant**: Smart translation and explanation on Windows
- **Screenshot & OCR**: Quick screenshot capture with text extraction from images
- **Screen Recording**: Professional recording with system audio and microphone capture

Core positioning:

- One hotkey workflow to manage both text and image clipboard history
- One selection workflow to translate, explain, and copy in-place
- One screenshot + OCR workflow from capture to text extraction
- One recording workflow with capsule controls, audio capture, and recording parameter configuration
- One configurable strategy set to balance history limits and key item retention

In addition, this project itself follows an AI full-process development workflow: AI deeply participates in requirement breakdown, solution design, coding implementation, and documentation maintenance.

---

## ✨ Feature Overview

### 📋 Clipboard Management

- **Auto Capture**: Real-time clipboard change detection with quick paste-back to any application
- **Smart Search**: Keyword search to quickly locate historical records
- **Category Management**: Custom category tags for organizing important content by scenario
- **Pin Protection**: Important items can be pinned,不受 capacity limits
- **Dual Input Methods**: Supports both keyboard (arrow keys, enter) and mouse (wheel, click) operations
- **AI Integration**: Direct AI usage within the window (`T` translate / `E` explain)
- **Right-Click Menu**: One-click "translate/explain" for the selected item
- **Language Configuration**: Customize translation target language and explanation language
- **Unified Identifier**: Text and image clipboards use item_id at the底层 level for smoother switching

### 🖼️ Image Clipboard Management

- **Auto Detection**: Detects images in clipboard and automatically generates thumbnail lists
- **Smart Search**: Search image history by category and keywords
- **Quick Paste**: Double-click thumbnails to paste directly at cursor position
- **Large Preview**: Full-screen image viewing with zoom and pan support
- **Persistent Storage**: Image history and categories saved locally, available after restart
- **Disk Management**: Configurable image storage limit (default 2GB) with automatic cleanup of old images
- **Performance Optimization**: Async preview generation, fixed long-list scrolling lag issues

### 🔎 Image OCR (Windows)

- **Pinned Window OCR**: Right-click in pinned image window to trigger OCR recognition
- **Dedicated Result Display**: Recognition results shown in a separate text window for copying
- **Windows Native**: Based on Windows Media OCR API, no internet required
- **Platform Support**: Currently supports Windows 10/11 only

### 🎥 Screen Recording (Windows)

- **Floating Capsule**: Lightweight recording control bar with hotkey activation
- **One-Click Control**: Start, pause, resume, stop - simple and intuitive operation
- **Audio Capture**:
    - System audio output (based on native WASAPI capture pipeline)
    - Microphone device selection with multi-device switching support
    - Real-time microphone toggle during recording (hotkey `Ctrl+Space`)
- **Parameter Configuration**:
    - Frame Rate: 1-120 FPS (default 30)
    - Video Bitrate: 500-50000 kbps (default 6000)
    - Audio Bitrate: 32-512 kbps (default 160)
    - Cursor Capture: Optional mouse pointer capture
    - Toolbar Protection: Option to hide recording capsule in video
- **Region Selection**: Supports full-screen or specific window recording
- **ffmpeg Management**: Auto-detection and on-demand download on first enable with real-time progress
- **State Protection**: Capsule shows disabled state when feature is turned off to prevent accidental triggers
- **Audio Optimization**: Fixed system audio disappearance issue when recording microphone, improved stop logic
- **Audio Merge Enhancement**: FFmpeg audio file pre-validation, AAC stream copy auto-fallback to re-encoding, smart
  filtering of corrupted audio files, ensuring recorded videos are preserved

### 🔤 AI Text Selection Assistant (Windows)

- **Multi-Scenario Selection**: Supports drag, double-click, triple-click, and other text selection methods
- **Smart Popup**: Selection toolbar automatically appears after text selection
- **Core Functions**:
    - Translate: Translate selected text to target language
    - Explain: Explain professional content in plain language
    - Copy: Quickly copy selected text
    - Custom Prompts: Support for personalized AI instructions
- **Streaming Output**: Result window displays AI-generated content in real-time
- **One-Click Write-Back**: Copy and auto-paste back to original position
- **Web Search**: Optional Bing search engine for quick related content lookup

### 🤖 AI Service Configuration

- **Built-in Providers**:
    - DeepSeek: High-performance conversation model
    - Qwen (`qwen`): Alibaba's large language model
    - Xiaomi Mimo (`xiaomimimo`): Xiaomi's self-developed model
- **Custom Extension**: Support for adding any OpenAI-compatible custom provider
- **Easy Management**: Delete custom providers directly from dropdown options
- **Secure Storage**: API keys stored in system credential manager (keyring) with encryption
- **Connection Test**: Immediate connectivity validation after configuration
- **Prompt Templates**: Customize system prompts for translation and explanation

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

> Current versions support Windows only; Linux/macOS are not yet supported and are under development.

---

## 📥 Download & Install

> GitHub Release is recommended.

| Channel | Link                                                              | Notes                 |
|---------|-------------------------------------------------------------------|-----------------------|
| GitHub  | [Latest Release](https://github.com/zRq1351/fuyun_tools/releases) | Recommended           |
| Gitee   | [China Mirror](https://gitee.com/zrq1351/fuyun_tools/releases)    | May lag behind GitHub |

Installation steps:

1. Download the installer for your platform
2. On Windows, run and install the `.exe` package
3. Configuration files are created automatically on first launch

---

## 🚀 Quick Start

1. **Launch App**: Run fuyun_tools; the icon appears in the system tray
2. **Use Global Hotkeys**:
    - Text clipboard: Windows `Ctrl+Shift+Z`
    - Image clipboard: Windows `Ctrl+Shift+X`
    - Screenshot: Windows `Ctrl+Shift+S`
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

### Recording Capsule (Windows)

- **Trigger**: Open via recording hotkey or from settings after enabling recording
- **Core Controls**: Start, pause, resume, and stop recording directly from the capsule
- **Audio Routing**: Select system output and microphone devices in capsule settings
- **First-Run Dependency Check**: Automatically checks `ffmpeg.exe`; if missing, prompts download with real-time
  progress
- **Disabled State**: When recording is disabled, the capsule shows a disabled state and prevents accidental actions
- **Audio Merge Reliability**: Audio files are pre-validated with FFmpeg before merge; corrupted/empty files are
  filtered out;
  AAC stream copy failures auto-fallback to re-encoding; video files are preserved even if audio merge fails

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

## 📄 Third-Party Licenses and Compliance

- The recording pipeline invokes `ffmpeg.exe` as an external process.
- The currently distributed FFmpeg build includes GPL components (for example, `libx264`), and that binary is
  distributed under GPL/LGPL obligations.
- The installer does not bundle `ffmpeg.exe` by default; it is downloaded on demand into local `bin` directory when
  recording is enabled.
- OCR functionality uses PaddleOCR PP-OCRv5 model (Apache 2.0 License) and MNN inference engine (Apache 2.0 License).
- Corresponding source and license disclosure for FFmpeg, PaddleOCR, MNN are documented in
  `docs/THIRD_PARTY_NOTICES.md`.
- License texts should be shipped with releases: `docs/GPLv2`, `docs/LGPLv2.1`, `docs/Apache-2.0` (and
  `docs/OpenCV_LICENSE` when
  OpenCV-related build is distributed).

---

## 🛠️ Local Development

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

Current versions implement the text-selection pipeline only on Windows; Linux/macOS are not yet supported and are under
development.

### 2) How do I delete a custom provider?

In AI provider dropdown options, click the `X` button on the right side of the custom provider.

### 3) Why does closing settings sometimes feel delayed after saving?

The close flow has been optimized for responsiveness. Please update to the latest version.

### 4) Why am I prompted to download ffmpeg when enabling recording?

On first enable, the app checks whether `ffmpeg.exe` exists in the expected `bin` path. If missing, it guides an
on-demand download.

### 5) How to quickly switch microphone during recording?

Use either of these methods:

- **Click icon**: Click the microphone icon on the recording capsule
- **Hotkey**: Use `Ctrl+Space` (customizable in settings)

Press to enable, release to disable — perfect for impromptu commentary.

### 6) Recording shows "audio merge failed" — what should I do?

v0.6.78 has significantly improved audio merge reliability. If this still occurs:

- **Video is preserved**: Even if audio merge fails, the video file itself is not lost
- **Check system audio**: Ensure the system audio device was working during recording
- **Disable system audio**: If system audio is not needed, turn it off in the recording capsule
- **Update version**: Ensure you are using the latest version, which automatically handles corrupted audio files

---

## 📦 Latest Release (v0.6.78)

### 🔊 Recording Audio Merge Enhancement

- **FFmpeg Audio Pre-validation**: Audio files are validated with FFmpeg actual decoding before merge, filtering
  corrupted/empty files
- **AAC Stream Copy Auto-Fallback**: When AAC stream copy fails, automatically switches to re-encoding mode
- **Smart Corrupted File Filtering**: Raised AAC file validity check threshold to prevent false positives
- **Graceful Degradation**: Audio merge failure preserves the video-only file instead of causing total recording failure
- **Enhanced Diagnostics**: FFmpeg AAC encoding output file size is automatically checked with diagnostic logging

---

## 📦 Previous Release (v0.6.77)

### 🎥 Recording Enhancement

- **Real-time Microphone Switch**: New real-time microphone switching with dedicated hotkey support (default
  `Ctrl+Space`)
- **Flexible Control**: More flexible device switching during recording, ideal for impromptu commentary
- **Press-to-Talk**: Hotkey uses press-to-enable, release-to-disable interaction

### 🔊 Audio Optimization

- **Fix System Audio Disappearance**: Fixed occasional system audio disappearance when recording microphone
- **Improved Stop Logic**: Enhanced audio stop logic for more stable recording completion
- **WASAPI Pipeline**: Based on native WASAPI capture pipeline for better audio quality

### 📋 Smoother Clipboard

- **Unified Identifier**: Text and image clipboard logic switched to `item_id`
- **Better Switching Experience**: Significantly improved switching fluency between text and image clipboards
- **Seamless Transition**: Switching between the two windows feels more natural

### 🛠️ History Fixes

- **Fix Scrolling Lag**: Completely fixed clipboard history scrolling lag issues
- **Fix Data Anomalies**: Resolved image data anomaly issues
- **Long List Optimization**: Long lists are more reliable with significant performance improvements

### ✨ Cleaner Interaction

- **Remove Popup Alerts**: Removed auto-fill success popup notifications
- **Less Distraction**: Provides a purer, non-intrusive experience
- **Stay Focused**: Keeps users focused on their current task

### ⚡ Performance Boost

- **Code Refactoring**: Comprehensive deep analysis and refactoring of codebase
- **Clean Redundancy**: Removed redundant tests and dependencies
- **Smoother System**: Overall smoother operation and faster response

---

## Demo

[Watch Demo Video](https://www.bilibili.com/video/BV1bwBSBUE8k)
