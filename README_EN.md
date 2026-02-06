# 🚀 fuyun_tools

[中文](./) | [English](README_EN.md)

## 🎯 Introduction

fuyun_tools is a desktop efficiency tool that combines clipboard management and AI text selection, running in the system
tray. With intelligent history management and AI assistance, it significantly improves daily work efficiency.

## 💻 Tech Stack

- **Frontend**: Vue 3 + Element Plus
- **Backend**: Rust + Tauri 2.0
- **AI Integration**: async-openai library, supporting various OpenAI-compatible services
- **Security**: API key encrypted storage to protect sensitive credentials

## 🌍 Compatibility

| Feature               | Windows           | Linux             | macOS             |
|-----------------------|-------------------|-------------------|-------------------|
| **Clipboard Manager** | ✅ Fully Supported | ✅ Fully Supported | ✅ Fully Supported |
| **Text Selection**    | ✅ Fully Supported | ❌ Not Supported   | ❌ Not Supported   |
| **System Tray**       | ✅ Fully Supported | ✅ Fully Supported | ✅ Fully Supported |

> ⚠️ **Note on Text Selection**: Currently, the text selection feature is only fully supported on Windows. It is not yet
> implemented for Linux and macOS.

## 🌟 Core Features

### 📋 Clipboard Management

- **Smart History**: Automatically records all clipboard content, supporting text, images, and other formats.
- **Quick Access**: Default shortcut `Ctrl+Shift+K` (macOS: `Cmd+Shift+K`) to quickly access history.
- **Multi-dimensional Navigation**: Supports keyboard arrow keys, mouse wheel, and touch gestures.
- **Smart Fill**: Automatically copies selected history to clipboard and fills into the target input box upon selection.
- **Capacity Management**: Configurable maximum history count (1-1000 items), with auto-cleanup of old records.

### 🔤 AI Text Selection Assistant (Windows Only)

- **Smart Interaction**:
    - **Double/Triple Click**: Optimized text selection experience, supporting double-click for words and triple-click
      for paragraphs.
    - **Auto Toolbar**: Toolbar automatically pops up after selecting text and closes when clicking outside.
- **AI Capabilities**:
    - **Real-time Translation**: Supports multi-language translation with streaming output.
    - **Smart Explanation**: Provides in-depth analysis of selected text.
- **Interface Experience**:
    - **Frameless Window**: Immersive result display, supports dragging.
    - **Streaming Response**: Real-time display of AI thinking process like a typewriter.

### 🤖 AI Service Integration

- **Multi-Provider Support**: Built-in support for DeepSeek, Qwen (Tongyi Qianwen), Xiaomi Mimo, and other mainstream AI
  services.
- **Custom Configuration**: Supports adding any OpenAI-compatible third-party AI service.
- **Secure Key Management**: API keys are encrypted and stored, dynamically decrypted for use, ensuring credential
  security.
- **Connection Test**: Built-in API connectivity test to ensure correct configuration.

### ⚙️ System Integration

- **System Tray**: Runs in the system tray with extremely low resource usage.
- **Custom Hotkeys**: Supports custom shortcut combinations.
- **Theme Switching**: Supports Dark/Light themes to adapt to different environments.
- **Auto Update**: Built-in auto-update mechanism supporting breakpoint resumption and real-time progress display.
- **Log Management**: Configurable log levels (supports recording errors only) to effectively manage disk space.

## 📥 Download & Install

> ⚠️ **Note**: The Gitee version may not be the latest. It is recommended to download from GitHub.

| Platform | Download Link                                                        | Status             |
|----------|----------------------------------------------------------------------|--------------------|
| GitHub   | [📥 Latest Release](https://github.com/zRq1351/fuyun_tools/releases) | ✅ Recommended      |
| Gitee    | [📥 Gitee Mirror](https://gitee.com/zrq1351/fuyun_tools/releases)    | ⚠️ May be outdated |

### 🖥️ System Requirements

- **Windows**: Windows 10 or later
- **Linux**: Supports most distributions (dependencies required)
- **macOS**: macOS 10.15 or later

### 📦 Installation Steps

1. Download the installation package for your platform from the links above.
2. Windows users run the `.exe` installer.
3. Linux users download the `.AppImage` or `.deb` package.
4. macOS users download the `.dmg` image file.
5. The configuration file will be automatically created upon the first run.

## 🚀 Latest Updates

**Interface & Interaction Optimization**

- ✨ **Immersive Result Window**: Removed system title bar, adopted custom UI, supports dragging, providing a better
  visual experience.
- 🖱️ **Smart Selection Optimization**: Added support for double-click and triple-click selection to accurately capture
  user intent.
- 🛠️ **Toolbar Interaction**: Optimized toolbar auto-close logic to reduce accidental interference.

**System Capability Enhancement**

- 📥 **Download Progress Feedback**: Real-time display of download progress and file size during software updates.
- 📝 **Log Level Management**: Supports configuring log levels, defaulting to recording only errors to prevent log file
  bloat.
- ⚡ **Performance Optimization**: Optimized lock contention and resource usage to improve response speed.

## 📋 Clipboard Usage

1. **Open History Panel**: Press default shortcut `Ctrl+Shift+K` (macOS: `Cmd+Shift+K`).
2. **Browse History**:
    - 🔼🔽 Use `↑` `↓` arrow keys to navigate up and down.
    - 🖱️ Use mouse wheel or touch drag to browse.
    - 📱 Supports touch screen gestures.
3. **Select Content**:
    - ⌨️ Press `Enter` key to confirm selection.
    - 🖱️ Double-click to select an item.
    - 📱 Tap to confirm.
4. **Close Panel**:
    - Press `Esc` key.
    - Click outside the interface.
    - Automatically closes after selection confirmation.

## 🔤 Text Selection Usage Tips

> ⚠️ **Important**: The text selection feature is currently only available on Windows.

1. **Activate Selection**: Select text in any application (supports mouse drag, double-click, triple-click).
2. **Use Toolbar**: A floating toolbar appears after selecting text.
3. **Function Options**:
    - 🟢 **Translate** - AI intelligent translation of selected text.
    - 🔵 **Explain** - Detailed explanation of text meaning.
    - 🟠 **Copy** - Copy selected text to clipboard.
4. **View Results**: Translation or explanation results will be displayed in a new window with streaming output.

## ⚙️ AI Configuration

1. **Open Settings**: Right-click system tray icon → Settings.
2. **Select Provider**:
    - Built-in options: DeepSeek, Qwen, Xiaomi Mimo.
    - Custom: Add any OpenAI-compatible service.
3. **Fill Configuration**:
    - API Endpoint
    - Model Name
    - API Key (Automatically encrypted)
4. **Test Connection**: Click the test button to verify configuration correctness.
5. **Save Settings**: Save configuration after confirmation.

📺 [Watch Demo Video](https://www.bilibili.com/video/BV1bwBSBUE8k)

## ⚙️ System Tray Menu

Right-click the system tray icon to access the full feature menu:

### 🧹 Cleanup

- **Clear History** - Completely clear all clipboard history.
- **Clear Logs** - Delete application log files to free up disk space.

### 🔧 Management

- **Open Log Directory** - Locate log files in the file manager.
- **Reload Config** - Reload application configuration file.
- **Dev Tools** - Open debug console (Dev mode).

### ⚙️ Settings

- **Settings** - Open application settings interface.
- **Check for Updates** - Manually check for software updates.
- **About** - View software version and copyright information.

### 🚪 Exit

- **Exit** - Completely exit the application and stop all background services.

## 🔧 Advanced Configuration

### Custom Shortcuts

Customize various operation shortcut combinations in the settings interface, supporting modifier keys like Ctrl, Alt,
Shift.

### Theme Customization

Supports both Dark and Light themes, switchable according to personal preference and usage environment.

### Log Level

Adjust log detail level; recommended to set to "Error Only" to save space.

### Startup Options

- **Auto Start**: Set application to run automatically on system startup.
- **Start Minimized**: Start directly minimized to the system tray.

## 🔒 Security Features

- **Key Encryption**: All API keys are encrypted using a self-developed algorithm.
