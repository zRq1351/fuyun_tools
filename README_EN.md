<div align="center">

<img src="src-tauri/icons/icon.png" alt="fuyun_tools" width="112" />

# fuyun_tools

**One Shortcut Away from Everything**

A keyboard-first Windows productivity hub: clipboard, AI selection, screenshot OCR, screen recording, phone mirroring, app launcher, and document management — always one shortcut deep.

![Version](https://img.shields.io/badge/version-0.8.53-blue?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows_10%20%2F%2011-0078D6?style=flat-square&logo=windows)
![License](https://img.shields.io/badge/license-GPL--2.0-green?style=flat-square)
![Tauri](https://img.shields.io/badge/Tauri-2.x-FFC131?style=flat-square&logo=tauri)
![Vue](https://img.shields.io/badge/Vue-3.x-4FC08D?style=flat-square&logo=vue.js)
![Rust](https://img.shields.io/badge/Rust-native-CE422B?style=flat-square&logo=rust)

[中文](README.md) · [Releases](https://github.com/zRq1351/fuyun_tools/releases) · [Gitee mirror](https://gitee.com/zrq1351/fuyun_tools)

</div>

---

## What it is

**fuyun_tools** is not a random toolbox — it is a **keyboard-driven efficiency core** for Windows:

| Capability | Entry | In one line |
|------------|-------|-------------|
| Text Clipboard | <kbd>Ctrl+Shift+Z</kbd> | Multi-layer dedup · categories & pin · AI on items |
| Image Clipboard | <kbd>Ctrl+Shift+X</kbd> | Async thumbs · disk quota · batch import |
| Screenshot OCR | <kbd>Ctrl+Shift+S</kbd> | Region / long-shot · pro annotations · dual offline OCR |
| Screen Recording | <kbd>Alt+R</kbd> | WGC / WASAPI · multi-monitor · floating capsule |
| Phone Mirroring | Enable in Settings | USB / Wi-Fi · reverse control · per-app mirror · screen-off |
| App Launcher | <kbd>Alt+Q</kbd> | Full scan · fuzzy search · custom commands |
| Document Hub | <kbd>Ctrl+Shift+D</kbd> | Index / repo modes · FTS5 · tag governance |

> **Principles**: features are **opt-in (off by default)** · data stays **local** · shortcuts are customizable · disabled entry points stay silent.

---

## Core experience

### 1 · Intelligent clipboard

- **Three-layer dedup**: Bloom Filter → XXH3 index → fuzzy version detection; fragments auto-upgrade to full text
- **Organization**: categories, pins, capacity policies — important items survive eviction
- **Keyboard flow**: arrows browse, Enter paste, `Ctrl+1~9` jump; `T` / `E` for AI on selection
- **Image library**: async thumbnails, quota, fullscreen preview, drag-drop import; paged IPC skips redundant previews when a local path exists

### 2 · AI text selection

```
Select text → Floating toolbar → Translate / Explain / Custom prompt → Stream → Write back
```

- Global hooks + linearity / horizontal-bias / speed heuristics to tell selection from casual drags
- DeepSeek, Qwen, Xiaomi Mimo, or any OpenAI-compatible endpoint
- API keys **encrypted in local SQLite**, masked in UI · SSE streaming · Markdown

### 3 · Screenshot & annotation

Pipeline: **capture → annotate → long-shot → OCR → pin**

| Tools | Details |
|-------|---------|
| Geometry | Rectangle · Circle · Arrow · Number callout · Line |
| Emphasis | Highlighter · Text · Freehand |
| Redact & sample | Mosaic · Blackout bar · Color picker (HEX/RGB) |
| Constraints | <kbd>Shift</kbd> squares, circumscribed circles, 0°/45°/90° snaps |
| Crop | Bake selection + marks into a new base and keep editing |
| History | <kbd>Ctrl+Z</kbd> / <kbd>Ctrl+Y</kbd> — up to 50 steps |

**Dual OCR engines (offline)**

| | Windows Native | PaddleOCR (MNN) |
|--|----------------|-----------------|
| Speed | ~500ms | ~1000ms |
| Accuracy | 80–85% | **95–98%** |
| Handwriting | — | ✓ |

### 4 · Professional recording

- **Capture**: WGC window/display hardware encode · FFmpeg fullscreen/region · WASAPI system / mic / per-process audio
- **Control**: collapsible capsule · Push-to-Talk (<kbd>Ctrl+Space</kbd>)
- **Reliability**: black-frame watchdog · Job Object cleanup · audio pre-validation · AAC fallback

### 5 · Phone mirroring

Android device mirroring built on the scrcpy protocol (see [Third-party](#third-party)). Frames travel over local USB or LAN only — **never through any server**.

**Connections**

| Mode | Description |
|------|-------------|
| USB | Plug and play, lowest latency (~40–80 ms) |
| One-tap USB → Wi-Fi | Automatically runs `adb tcpip`, reads the phone's IP and establishes the wireless link; unplug and keep mirroring |
| Manual IP / wireless debugging | Enter an IP manually; Android 11+ supports pairing-code wireless debugging |
| Device history | One-tap reconnect for multiple phones |

**Quality profiles (switchable in Settings)**

| Profile | Resolution | FPS | Bitrate | Best for |
|---------|-----------|-----|---------|----------|
| Smooth | long edge 1280 | 30 | 6 Mbps | Weak networks / low traffic |
| Balanced (default) | long edge 1600 | 30 | 8 Mbps | Latency/quality balance |
| HD | long edge 1920 | 60 | 16 Mbps | High refresh, USB preferred |
| Native | device native | 60 | 24 Mbps | Near-native experience |

**Reverse control & mirroring enhancements**

- **Reverse control**: mouse click / drag / wheel, keyboard with CJK IME support, two-way clipboard sync
- **One-tap per-app mirroring**: move the phone's **current foreground app** into an isolated virtual display — watch it in the mirror window while the physical phone screen stays free (Android 10+)
- **Screen-off mirroring**: turn off the phone's panel while rendering and mirroring continue; the screen wakes automatically when mirroring stops
- **Wheel paging**: synthesize touch flings from wheel input so short-video apps (Douyin, Hongguo, …) snap to the next video; switch back to smooth scrolling anytime
- **File transfer**: drop files onto the mirror window to push them to the phone (with media scan); one-tap export of recent phone files/images
- **Session resilience**: automatic rebuild on stream/control loss; seamless recovery across USB→Wi-Fi switches and device changes

### 6 · Launcher & document hub

- **Launcher**: Start Menu sweep, PE verification, built-in commands into every module, category grid + drag sort
- **Documents**: index (zero-move) vs repository (managed) · FTS5 · **multi-select batch tags**, global rename/merge/delete, tag filters, keyword snippets, rebuild index · desktop widget

---

## System integration

<div align="center">

| Tray | Auto-start | Shortcuts | Themes | i18n | Updates |
|:----:|:----------:|:---------:|:------:|:----:|:-------:|
| Context menu | Optional | Fully customizable | Light / Dark / Eye-care | zh / en | Silent check + settings badge |

</div>

- **Backup & restore**: `.fytbk.zip` + SHA-256 · manual or scheduled · merge/overwrite with rollback
- **Diagnostics**: health checks, one-click repair, performance monitor (CPU/memory, startup & IPC tops, idle window GC)
- **Settings**: 10 tabs, right pane auto-saves with 450ms debounce

---

## Quick start

1. Download the `.exe` from [Releases](https://github.com/zRq1351/fuyun_tools/releases)
2. Tray icon → **Settings**, enable the modules you need (all off by default)
3. For AI selection / clipboard AI, configure an OpenAI-compatible provider under **AI Settings** and run **Test Connection**
4. Recording asks you to download FFmpeg on first enable (not bundled)
5. Phone mirroring lives under **Settings → Phone Sync**: USB works out of the box; wireless pairing prefers Android 11+; per-app mirroring needs Android 10+

**Requirements**: Windows 10/11 (64-bit) · 4GB+ RAM · 500MB+ disk

```bash
cd src && npm install
npm run tauri:dev      # hot reload
npm run tauri:build    # production
cd src-tauri && cargo check
```

---

## Architecture

| Layer | Stack |
|-------|-------|
| Shell | **Tauri 2.x** |
| Frontend | Vue 3 · Element Plus · Vite · 16 WebViews |
| Backend | **Rust** · threads & async tasks |
| Data | SQLite · WAL · FTS5 · fully local |
| AI | async-openai · encrypted local keys |
| Imaging | image · imageproc · OpenCV (optional long-shot) |
| AV | WASAPI · WGC / DXGI · FFmpeg (on demand) |
| OCR | Windows Media OCR · PaddleOCR / MNN (offline) |
| Phone mirroring | scrcpy-server v5.0.1 (embedded, unmodified) · Android platform-tools adb |

```text
fuyun_tools/
├── src/           # Vue 3 frontend (clipboard / screenshot / recording / mirroring / launcher / docs …)
├── src-tauri/     # Rust backend (features · services · ui · utils)
└── docs/          # Licenses & notices
```

---

## Security & privacy

| | |
|--|--|
| API keys | Encrypted local SQLite, masked UI |
| OCR / recording / mirroring / data | **Local only** — no upload, no telemetry; mirrored frames travel over USB or LAN point-to-point |
| Source | **GPL-2.0**, auditable |
| Defaults | Features **off** until you enable them |

---

## FAQ

<details>
<summary><b>Shortcuts do nothing?</b></summary>
Enable the feature in Settings first, then check for conflicts. Failed registration opens Settings automatically.
</details>

<details>
<summary><b>Why no AI selection on Linux/macOS?</b></summary>
The pipeline depends on Windows global hooks (<code>WH_MOUSE_LL</code> / <code>WH_KEYBOARD_LL</code>).
</details>

<details>
<summary><b>Why download FFmpeg for recording?</b></summary>
Keeps the installer small; fetched on demand (GitHub / Gitee, URL configurable).
</details>

<details>
<summary><b>Are API keys safe?</b></summary>
Yes — encrypted in local SQLite provider config, never plaintext in settings files.
</details>

<details>
<summary><b>Android requirements for mirroring?</b></summary>
Basic USB / Wi-Fi mirroring: Android 7+. Per-app mirroring (virtual display): Android 10+. Wireless-debugging pairing (first connection without USB): Android 11+.
</details>

<details>
<summary><b>Do mirrored frames go through a third-party server?</b></summary>
No. Frames travel over the USB cable or your local network point-to-point. The app never collects or uploads mirroring content.
</details>

<details>
<summary><b>Migrate to another PC?</b></summary>
Export `.fytbk.zip` via Backup on the old machine, Restore on the new one.
</details>

---

## Third-party

Built on great open-source projects — thanks to their authors and communities:

- **scrcpy** (Genymobile) — the core of phone mirroring: embeds **scrcpy-server v5.0.1** (unmodified, distributed as-is), which provides the mirroring protocol and on-device rendering · Apache License 2.0 · [Project](https://github.com/Genymobile/scrcpy)
- **Android platform-tools (adb)** — device connection and debugging channel, bundled with the installer · Apache License 2.0 · [AOSP](https://developer.android.com/tools/adb)
- **FFmpeg** — external process (GPL/LGPL), on-demand for recording
- **PaddleOCR / MNN** — Apache 2.0
- **OpenCV** — optional compile feature (Apache 2.0)

Full license texts and distribution-compliance notes: [`docs/THIRD_PARTY_NOTICES.md`](docs/THIRD_PARTY_NOTICES.md) · [`docs/scrcpy_LICENSE`](docs/scrcpy_LICENSE)

---

<div align="center">

## Get the app

[![GitHub](https://img.shields.io/badge/GitHub-Release-white?style=for-the-badge&logo=github)](https://github.com/zRq1351/fuyun_tools/releases)
[![Gitee](https://img.shields.io/badge/Gitee-Mirror-white?style=for-the-badge)](https://gitee.com/zrq1351/fuyun_tools)
[![Bilibili](https://img.shields.io/badge/Bilibili-Demo-white?style=for-the-badge&logo=bilibili)](https://www.bilibili.com/video/BV1bwBSBUE8k)

**GPL-2.0** · Windows 10 / 11 · Local-first · Keyboard-driven

</div>
