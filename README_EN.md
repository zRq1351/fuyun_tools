<div align="center">

<img src="src-tauri/icons/icon.png" alt="fuyun_tools" width="96" />

# fuyun_tools

**One Shortcut Away from Everything**

![Version](https://img.shields.io/badge/version-0.8.1-blue?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows_10/11-0078D6?style=flat-square&logo=windows)
![License](https://img.shields.io/badge/license-GPL--2.0-green?style=flat-square)
![Rust](https://img.shields.io/badge/Tauri-2.x-FFC131?style=flat-square&logo=tauri)
![Vue](https://img.shields.io/badge/Vue-3.x-4FC08D?style=flat-square&logo=vue.js)

[中文](README.md)

</div>

---

**fuyun_tools** is a Windows system-tray productivity suite that unifies clipboard management, AI text selection,
screenshot OCR, screen recording, app launching, and document management into a keyboard-driven workflow.

<div align="center">

| <kbd>Ctrl+Shift+Z</kbd><br>📋 Text Clipboard | <kbd>Ctrl+Shift+X</kbd><br>🖼️ Image Clipboard | <kbd>Ctrl+Shift+S</kbd><br>✂️ Screenshot OCR | <kbd>Alt+R</kbd><br>🎥 Recording | <kbd>Alt+Q</kbd><br>🔍 App Launcher | <kbd>Ctrl+Shift+D</kbd><br>📁 Document Manager |
|----------------------------------------------|------------------------------------------------|----------------------------------------------|----------------------------------|-------------------------------------|------------------------------------------------|

</div>

---

## 🚀 Quick Start

### System Requirements

| Item | Requirement            |
|------|------------------------|
| OS   | Windows 10/11 (64-bit) |
| RAM  | 4GB+ recommended       |
| Disk | 500MB+ free space      |

### Installation

1. Download the `.exe` installer from [GitHub Release](https://github.com/zRq1351/fuyun_tools/releases)
2. Run the installer and follow the prompts
3. On first launch, the tray icon appears and config files are created automatically

### Get Started in 3 Steps

**Step 1: Configure AI**

Go to Settings → AI Settings, choose a provider (DeepSeek / Qwen / Xiaomi Mimo / custom OpenAI-compatible), enter the
API URL, model name, and key, then click "Test Connection".

**Step 2: Try AI Selection**

Select text in any application — the floating toolbar appears automatically. Click "Translate" or "Explain" to see
streaming AI output.

**Step 3: Explore More**

| Action               | Shortcut       |
|----------------------|----------------|
| Open Text Clipboard  | `Ctrl+Shift+Z` |
| Open Image Clipboard | `Ctrl+Shift+X` |
| Screenshot & OCR     | `Ctrl+Shift+S` |
| Screen Recording     | `Alt+R`        |
| App Launcher         | `Alt+Q`        |
| Document Manager     | `Ctrl+Shift+D` |

---

## 📋 Clipboard Management

> Automatically monitors clipboard changes, providing independent history and paste-back for text and images.

### Text Clipboard

| Feature             | Implementation                                                                                 |
|---------------------|------------------------------------------------------------------------------------------------|
| Multi-layer Dedup   | Bloom filter → XXH3 hash index → fuzzy version detection                                       |
| Smart Replacement   | Copies fragment then full text → auto-replaces with complete version                           |
| Categories & Pin    | Custom labels, drag-and-drop; pinned items survive capacity eviction                           |
| Keyboard Ops        | `←` `→` navigate, `Enter` paste, `Ctrl+1~9` quick select                                       |
| AI Integration      | Press `T` to translate, `E` to explain selected item                                           |
| Capacity Protection | With "Limit Ungrouped Items Only" enabled, categorized and pinned items are immune to eviction |

### Image Clipboard

| Feature          | Implementation                          |
|------------------|-----------------------------------------|
| Async Thumbnails | Background generation, smooth scrolling |
| Disk Quota       | Default 2GB cap, automatic cleanup      |
| Full Preview     | Zoom and pan                            |
| Batch Import     | Drag-and-drop or file picker            |

---

## 🔤 AI Text Selection

> Select text in any app and a floating toolbar appears — no window switching.

```
Select text → Toolbar pops up → Translate / Explain / Custom Prompt → Streaming result → Write back
```

### Detection

Uses `WH_MOUSE_LL` + `WH_KEYBOARD_LL` global hooks with multi-factor heuristics — **linearity** (R² > 0.9), **horizontal
bias**, **movement speed** — to accurately distinguish text selection from casual clicking/dragging and suppress false
triggers.

### Toolbar Buttons

|              | Translate | Explain | Copy |             Custom             |
|--------------|:---------:|:-------:|:----:|:------------------------------:|
| **Default**  |     ✓     |    ✓    |  ✓   |               —                |
| **Extended** |     —     |    —    |  —   | User-defined AI prompt buttons |

### AI Providers

- **Built-in**: DeepSeek · Qwen (Tongyi) · Xiaomi Mimo
- **Extensible**: Any OpenAI-compatible endpoint
- **Secure**: API key stored in Windows Credential Manager, never in plaintext
- **Streaming**: SSE real-time delivery, Markdown rendering

---

## ✂️ Screenshot & OCR

> A single shortcut powers the full pipeline: region capture → annotation → long screenshot → OCR → pin.

### Screenshot

| Feature         | Details                                                                                                |
|-----------------|--------------------------------------------------------------------------------------------------------|
| Region Select   | Drag to select, or click to auto-detect window bounds                                                  |
| Long Screenshot | Scroll capture + OpenCV template matching / phase correlation (max 20000px)                            |
| Annotations     | Rectangle · Circle · Arrow · Text · Freehand · Mosaic · Color Picker                                   |
| Undo / Redo     | `Ctrl+Z` / `Ctrl+Y`, up to 50 steps                                                                    |
| Pin to Screen   | Float always-on-top, right-click to OCR, double-click to close                                         |
| Color Picker    | 11×11px region at 12× zoom, pixel-level color sampling; `Shift` toggles HEX/RGB, `Ctrl+C` copies value |

### OCR Engines

<table>
<tr>
<th></th>
<th>Windows Native OCR</th>
<th>PaddleOCR (MNN)</th>
</tr>
<tr>
<td><b>Speed</b></td>
<td>~500ms</td>
<td>~1000ms</td>
</tr>
<tr>
<td><b>Accuracy</b></td>
<td>80-85%</td>
<td>95-98% ✨</td>
</tr>
<tr>
<td><b>Network</b></td>
<td>Offline</td>
<td>Offline</td>
</tr>
<tr>
<td><b>Handwriting</b></td>
<td>—</td>
<td>✓</td>
</tr>
<tr>
<td><b>Preprocessing</b></td>
<td>Lanczos3 + adaptive binarization</td>
<td>MNN inference</td>
</tr>
</table>

---

## 🎥 Screen Recording

> WASAPI native audio capture + WGC hardware acceleration + FFmpeg encoding, all controlled from a floating capsule.

### Capture Matrix

| Target               | Technology                                          |
|----------------------|-----------------------------------------------------|
| Window               | **WGC** (Windows Graphics Capture) → hardware H.264 |
| Full screen / Region | **FFmpeg gdigrab** → `libx264 veryfast`             |
| System Audio         | **WASAPI** loopback → AAC 128kbps                   |
| Microphone           | **WASAPI** input → WAV                              |
| Per-process Audio    | Application-level loopback capture                  |

### Floating Capsule

```
┌──────────────────────────────────────┐
│ 🔴 00:12:35  ┃  ⏸  ⏹  ┃  ⚙  🎤  ✕  │
└──────────────────────────────────────┘
```

- **Collapsed** — 38px rounded pill, red pulsing dot during recording
- **Expanded** — Full panel: target selection, audio devices, FPS/bitrate settings
- **Mic Push-to-Talk** — Hold `Ctrl+Space` during recording to unmute, release to mute — ideal for live commentary

### Reliability

| Mechanism            | Purpose                                            |
|----------------------|----------------------------------------------------|
| Black-frame Watchdog | Auto-stop after 4s with no video frames            |
| Job Object           | Guaranteed FFmpeg subprocess cleanup               |
| Audio Pre-validation | FFmpeg decode check before muxing                  |
| AAC Fallback         | Auto retry with re-encoding on stream copy failure |

---

## 🔍 App Launcher

> `Alt+Q` to summon, fuzzy search, Enter to launch.

- Scans **Start Menu** for all `.lnk` shortcuts, auto-categorizes by folder
- **PE signature verification** to distinguish system vs. third-party apps
- Built-in commands: `:settings` `:clipboard` `:screenshot` `:record`
- Custom commands: run programs · open windows · copy text
- Category grid view with **SortableJS drag-and-drop**
- Launch all apps in a category at once

---

## 📁 Document Manager

> Index / Repository dual-mode, FTS5 full-text search, tags and categories. Desktop widget for always-on access.

| Feature             | Description                                                        |
|---------------------|--------------------------------------------------------------------|
| Index Mode          | Reference paths only, files stay in place                          |
| Repository Mode     | Physically move files to managed directory                         |
| Full-text Search    | SQLite FTS5 across titles / content / tags / notes                 |
| File Icons          | Auto-extracts system-associated program icons (up to 256px), lazy-cached, zero wait |
| Desktop Widget      | Always-on-top floating panel, drag-and-drop import, right-click actions, compact edge-snapping |
| Drag-and-drop       | Drop files or folders to import, auto-prompt index/repo mode       |
| Import History      | Rollback support by batch                                          |
| Orphan Detection    | Auto-discover unregistered files in managed directories            |

---

## ⚙️ System Integration

<div align="center">

| 🖥️ System Tray  | 🚀 Auto-start | ⌨️ Global Shortcuts |      🎨 Themes      |   🌐 i18n    |  🔄 Auto-update  |
|:----------------:|:-------------:|:-------------------:|:-------------------:|:------------:|:----------------:|
| Right-click menu |   Optional    |  All customizable   | Light/Dark/Eye-care | zh-CN / EN   | Built-in checker |

</div>

---

## 🧰 Tray Menu

Right-click the system tray icon:

| Menu Item     | Description                     |
|---------------|---------------------------------|
| Auto Start    | Toggle launch on system startup |
| Clear History | Clear clipboard history         |
| Settings      | Open settings window            |
| Exit          | Quit application                |

> Dev builds additionally provide "Clear Logs" and "Open Log Directory".

---

## ⚙️ Settings

The settings window has 10 tabs in the left sidebar, with auto-save on the right (450ms debounce):

| Tab              | Configuration                                       |
|------------------|-----------------------------------------------------|
| Clipboard        | Shortcuts, item limit, capacity protection strategy |
| Screenshot       | Screenshot hotkey, OCR engine selection             |
| Recording        | Recording hotkey, audio devices, FFmpeg management  |
| Selection        | Trigger method, custom prompts, search engine       |
| Launcher         | Launcher hotkey, view mode                          |
| Doc Manager      | Document manager hotkey, feature toggle, desktop widget toggle |
| AI Settings      | AI provider, API key, model, connection test        |
| Backup & Restore | Manual backup, auto-backup schedule, restore        |
| Diagnostics      | System health checks, one-click repair              |
| About            | Version info, update check, project links           |

---

## 💾 Backup & Restore

|            | Backup                                                    | Restore                                |
|------------|-----------------------------------------------------------|----------------------------------------|
| **Method** | Manual export / Scheduled auto (daily/weekly/monthly)     | Choose backup package                  |
| **Format** | `.fytbk.zip` + SHA-256 checksum                           | Merge (add new) or Overwrite           |
| **Safety** | —                                                         | Auto rollback point, revert on failure |
| **Scope**  | Clipboard history · Image history · Categories · Settings | Same                                   |

---

## 🏗️ Tech Stack

| Layer             | Technology                            |
|:------------------|:--------------------------------------|
| Desktop Framework | **Tauri 2.x**                         |
| Frontend          | Vue 3 · Element Plus · Vite           |
| Backend           | Rust                                  |
| Database          | SQLite · WAL · FTS5                   |
| AI                | async-openai (OpenAI-compatible)      |
| Imaging           | image · imageproc · OpenCV (optional) |
| Audio             | WASAPI (cpal / wasapi)                |
| Capture           | WGC · DXGI · FFmpeg                   |
| OCR               | Windows Media OCR · PaddleOCR / MNN   |
| Security          | Windows Credential Manager            |

```bash
fuyun_tools/
├── src/                     # Vue 3 frontend (16 independent webview windows)
│   ├── pages/
│   │   ├── clipboard/       # Text clipboard
│   │   ├── image_clipboard/ # Image clipboard
│   │   ├── selection_toolbar/  # AI selection toolbar
│   │   ├── result_display/  # AI result display
│   │   ├── screenshot/      # Screenshot editor
│   │   ├── longshot_toolbar/   # Long screenshot control
│   │   ├── recording_toolbar/  # Recording capsule
│   │   ├── launcher/        # App launcher
│   │   ├── document_manager/   # Document manager
│   │   ├── document_manager_widget/  # Document manager desktop widget
│   │   ├── settings/        # Settings
│   │   └── ...
│   └── services/            # IPC communication layer
├── src-tauri/               # Rust backend
│   └── src/
│       ├── core/            # App state · Config · Error handling
│       ├── features/        # System-level (hooks · recording · screenshot · selection)
│       ├── services/        # Business logic (clipboard · AI · OCR · launcher)
│       ├── ui/              # Command routing · Window management · Tray menu
│       └── utils/           # Database · Backup · Settings model
└── docs/                    # Licenses & third-party notices
```

---

## ❓ FAQ

<details>
<summary><b>Why isn't AI text selection available on Linux/macOS?</b></summary>

The text selection pipeline currently relies on Windows global hooks (`WH_MOUSE_LL` / `WH_KEYBOARD_LL`) and is not yet
ported to other platforms.

</details>

<details>
<summary><b>How do I delete a custom AI provider?</b></summary>

Click the `✕` next to the custom provider in the dropdown. Built-in providers cannot be deleted.

</details>

<details>
<summary><b>Why am I prompted to download FFmpeg when enabling recording?</b></summary>

FFmpeg is not bundled to keep the installer small. On first enable, the app checks for `ffmpeg.exe` and downloads it on
demand if missing (from GitHub / Gitee; the download URL is configurable).

</details>

<details>
<summary><b>How do I quickly toggle the microphone during recording?</b></summary>

Two ways: ① Click the mic icon on the recording capsule; ② Use `Ctrl+Space` (hold to talk, release to mute). The
shortcut can be customized in settings.

</details>

<details>
<summary><b>Will my clipboard history be lost?</b></summary>

No. Both text and image history are persisted to local SQLite databases and survive restarts. For important items, pin
or categorize them — with capacity protection enabled, they are permanently retained. Scheduled auto-backup is also
available.

</details>

<details>
<summary><b>Is my API key secure?</b></summary>

Yes. API keys are encrypted and stored in Windows Credential Manager (keyring), never written as plaintext in any config
file. The UI only shows masked characters.

</details>

<details>
<summary><b>What if shortcuts conflict with other apps?</b></summary>

All global shortcuts can be customized in Settings. If a shortcut registration fails at startup, the settings window
opens automatically with a conflict warning.

</details>

<details>
<summary><b>How do I migrate data to a new computer?</b></summary>

On the old computer, use Settings → Backup & Restore → Export Now to create a `.fytbk.zip` file. On the new computer,
import it via the Restore function.

</details>

---

## 🛠️ Local Development

<details>
<summary><b>Prerequisites</b></summary>

- **Node.js** 18+
- **Rust** toolchain (`rustup`)
- **Windows 10/11 SDK**
- (Optional) **OpenCV** — for long-screenshot compile feature

</details>

```bash
# Install frontend dependencies
cd src && npm install

# Tauri dev mode (hot reload)
npm run tauri:dev

# Production build
npm run tauri:build

# Rust code check only
cd src-tauri && cargo check
```

---

## 🔒 Security & Privacy

| Aspect     | Policy                                                |
|------------|-------------------------------------------------------|
| 🔑 API Key | Encrypted in Windows Credential Manager, masked in UI |
| 📷 OCR     | Fully offline, no image upload                        |
| 💾 Data    | Entirely local storage, zero data collection          |
| 📖 Source  | GPL open source, auditable                            |

---

## 📄 Third-Party Licenses

- **FFmpeg** — External process (GPL/LGPL), downloaded on demand when recording is first enabled
- **PaddleOCR** — PP-OCRv5 model (Apache 2.0)
- **MNN** — Inference engine (Apache 2.0)
- **OpenCV** — Optional compile feature (Apache 2.0)

See [`docs/THIRD_PARTY_NOTICES.md`](docs/THIRD_PARTY_NOTICES.md)

---

<div align="center">

## 📥 Download

| [![GitHub](https://img.shields.io/badge/GitHub-Release-blue?style=for-the-badge&logo=github)](https://github.com/zRq1351/fuyun_tools/releases) | [![Gitee](https://img.shields.io/badge/Gitee-Mirror-C71D23?style=for-the-badge)](https://gitee.com/zrq1351/fuyun_tools) |
|------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|

**GPL-2.0** · [Demo Video](https://www.bilibili.com/video/BV1bwBSBUE8k)

</div>
