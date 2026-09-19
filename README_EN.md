<div align="center">

<img src="src-tauri/icons/icon.png" alt="fuyun_tools" width="112" />

# fuyun_tools

**One Shortcut Away from Everything**

A keyboard-first Windows productivity hub: clipboard, AI selection, screenshot OCR, screen recording, app launcher, and document management — always one shortcut deep.

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

| Capability | Shortcut | In one line |
|------------|----------|-------------|
| Text Clipboard | <kbd>Ctrl+Shift+Z</kbd> | Multi-layer dedup · categories & pin · AI on items |
| Image Clipboard | <kbd>Ctrl+Shift+X</kbd> | Async thumbs · disk quota · batch import |
| Screenshot OCR | <kbd>Ctrl+Shift+S</kbd> | Region / long-shot · pro annotations · dual offline OCR |
| Screen Recording | <kbd>Alt+R</kbd> | WGC / WASAPI · multi-monitor · floating capsule |
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

### 5 · Launcher & document hub

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

```text
fuyun_tools/
├── src/           # Vue 3 frontend
├── src-tauri/     # Rust backend
└── docs/          # Licenses & notices
```

---

## Security & privacy

| | |
|--|--|
| API keys | Encrypted local SQLite, masked UI |
| OCR / recording / data | **Local only** — no upload, no telemetry |
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
The pipeline depends on Windows global hooks (`WH_MOUSE_LL` / `WH_KEYBOARD_LL`).
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
<summary><b>Migrate to another PC?</b></summary>
Export `.fytbk.zip` via Backup on the old machine, Restore on the new one.
</details>

---

## Third-party

- **FFmpeg** — external process (GPL/LGPL), on-demand for recording
- **PaddleOCR / MNN** — Apache 2.0
- **OpenCV** — optional compile feature (Apache 2.0)

See [`docs/THIRD_PARTY_NOTICES.md`](docs/THIRD_PARTY_NOTICES.md)

---

<div align="center">

## Get the app

[![GitHub](https://img.shields.io/badge/GitHub-Release-white?style=for-the-badge&logo=github)](https://github.com/zRq1351/fuyun_tools/releases)
[![Gitee](https://img.shields.io/badge/Gitee-Mirror-white?style=for-the-badge)](https://gitee.com/zrq1351/fuyun_tools)
[![Bilibili](https://img.shields.io/badge/Bilibili-Demo-white?style=for-the-badge&logo=bilibili)](https://www.bilibili.com/video/BV1bwBSBUE8k)

**GPL-2.0** · Windows 10 / 11 · Local-first · Keyboard-driven

</div>
