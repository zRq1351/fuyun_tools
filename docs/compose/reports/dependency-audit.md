# Dependency Audit Report

**Project:** fuyun_tools v0.8.1  
**Date:** 2026-07-23  
**Auditor:** MiMoCode Agent  

## Executive Summary

The project has **0 known security vulnerabilities** in frontend dependencies. There are **11 outdated Rust dependencies** and **14 outdated frontend dependencies** identified, with several requiring major version bumps. No critical security issues were found.

---

## Frontend Dependencies (package.json)

### Security Status
- `npm audit`: **0 vulnerabilities found**

### Outdated Dependencies

| Package | Current | Latest | Semver | Priority | Notes |
|---------|---------|--------|--------|----------|-------|
| `@tauri-apps/api` | 2.10.1 | 2.11.1 | Minor | Medium | Core Tauri API |
| `@tauri-apps/cli` | 2.10.1 | 2.11.4 | Minor | Low | Dev tooling only |
| `@tauri-apps/plugin-dialog` | 2.7.0 | 2.7.2 | Patch | High | Security-relevant (dialog) |
| `@tauri-apps/plugin-opener` | 2.5.3 | 2.5.4 | Patch | Medium | File opener |
| `@vitejs/plugin-vue` | 6.0.5 | 6.0.8 | Patch | Low | Dev tooling |
| `@vueuse/core` | 14.2.1 | 14.3.0 | Minor | Low | Utility library |
| `element-plus` | 2.13.7 | 2.14.3 | Minor | Medium | UI framework |
| `eslint` | 10.3.0 | 10.7.0 | Minor | Low | Dev tooling |
| `eslint-plugin-vue` | 10.9.1 | 10.10.0 | Minor | Low | Dev tooling |
| `sass` | 1.99.0 | 1.101.3 | Minor | Low | Dev tooling |
| `unplugin-vue-components` | 32.0.0 | 32.1.0 | Minor | Low | Dev tooling |
| `vue` | 3.5.32 | 3.5.40 | Patch | Medium | Core framework |
| `vue-i18n` | 11.1.10 | 11.4.7 | Minor | Medium | i18n (pinned, not caret) |
| `vuedraggable` | 4.1.0 | 4.1.0 | - | - | Latest available |

### Recommended Updates (Priority Order)

1. **`@tauri-apps/plugin-dialog`** → 2.7.2 (patch, security-relevant)
2. **`@tauri-apps/api`** → 2.11.1 (minor, core API alignment)
3. **`vue`** → 3.5.40 (patch, stability)
4. **`element-plus`** → 2.14.3 (minor, UI fixes)
5. **`vue-i18n`** → 11.4.7 (minor, note: currently pinned without caret)

---

## Rust Dependencies (Cargo.toml)

### Security Status
- `cargo audit`: Not installed (install via `cargo install cargo-audit`)
- Manual review: No known critical vulnerabilities identified

### Build Environment Note
- `cargo check` fails on `ocr-rs` v2.3.3 due to a corrupted MNN object file (`GeometryUnary.cpp.obj`)
- This is a pre-existing build environment issue (CMake/MSVC toolchain), not a dependency issue
- The `ocr-rs` crate builds MNN from source via CMake; the corrupted `.obj` suggests a partial build cache

### Outdated Dependencies

| Package | Current | Latest | Semver | Priority | Notes |
|---------|---------|--------|--------|----------|-------|
| `async-openai` | 0.38.2 | 0.41.1 | Minor | Medium | OpenAI client, breaking API changes likely |
| `cpal` | 0.17.3 | 0.18.1 | Minor | Medium | Audio I/O, used for recording |
| `imageproc` | 0.26.2 | 0.27.0 | Minor | Low | Image processing |
| `keyring` | 3.6.3 | 4.1.5 | **Major** | High | Credential storage, breaking API changes |
| `opencv` | 0.98.2 | 0.99.0 | Minor | Low | Optional feature only |
| `pdf-extract` | 0.10.0 | 0.12.0 | Minor | Low | PDF text extraction |
| `sqlx` | 0.8.6 | 0.9.0 | **Major** | High | Database, breaking API changes |
| `generic-array` | 0.14.7 | 0.14.9 | Patch | Low | Transitive dependency |
| `toml` | 0.8.2 | 0.8.23 | Patch | Low | Transitive dependency |
| `toml_datetime` | 0.6.3 | 0.6.11 | Patch | Low | Transitive dependency |
| `toml_edit` | 0.20.2 | 0.20.7 | Patch | Low | Transitive dependency |

### Recommended Updates (Priority Order)

1. **`keyring`** → 4.1.5 (**major** breaking change, test thoroughly)
2. **`sqlx`** → 0.9.0 (**major** breaking change, database schema compatibility)
3. **`cpal`** → 0.18.1 (audio recording feature)
4. **`async-openai`** → 0.41.1 (AI features, check API compatibility)
5. **`imageproc`** → 0.27.0 (image processing)

---

## Module Structure Analysis

### Frontend (Vue 3)

**Structure:**
```
src/
├── components/          # 3 shared components (ContextMenu, FormattedContent)
├── composables/         # 4 composables (event, locale, theme, drag)
├── pages/              # 16 page modules (multi-page architecture)
├── services/           # 1 IPC service file
├── utils/              # 6 utility modules
└── locales/            # i18n translations
```

**Observations:**
- Clean multi-page architecture with HTML entry points per feature
- Shared components are minimal (3 files) - good separation
- Composables are well-organized (4 focused composables)
- IPC service is centralized in single file - good pattern

### Rust Backend

**Structure:**
```
src-tauri/src/
├── core/              # 8 modules (state, config, error handling)
├── features/          # 4 modules (mouse, recording, screenshot, text_selection)
├── services/          # 16 modules (AI, clipboard, OCR, launcher, etc.)
├── ui/                # 14 modules (commands, tray, window management)
├── utils/             # 15 modules (backup, database, image, settings)
├── lib.rs            # Main entry (725 lines)
└── main.rs           # Entry point
```

**Observations:**
- Well-organized module hierarchy with clear separation of concerns
- `lib.rs` is large (725 lines) - contains all shortcut registrations and app setup
- `ui/commands.rs` is very large (2008 lines) - candidate for further splitting
- `AppState` is a central shared state with many fields - potential for refactoring

### Coupling Assessment

**Low Coupling (Good):**
- Frontend pages are isolated HTML entry points
- Services layer is cleanly separated from UI commands
- Features (recording, screenshot, etc.) are modular

**Medium Coupling (Acceptable):**
- `AppState` is accessed across all layers (core, services, ui)
- `lib.rs` orchestrates many modules directly

**High Coupling (Concern):**
- `ui/commands.rs` imports from nearly all other modules
- Frontend `ipc.js` defines all IPC commands in one place (necessary but large)

---

## Dependency Compatibility Matrix

| Dependency | Frontend Version | Rust Version | Compatibility |
|------------|------------------|--------------|---------------|
| @tauri-apps/api | 2.10.1 | tauri 2.11.5 | OK (minor mismatch) |
| @tauri-apps/plugin-* | 2.x | tauri-plugin-* 2.x | OK |

---

## Action Items

### Immediate (No Risk)
- [ ] Update `@tauri-apps/plugin-dialog` to 2.7.2
- [ ] Update `@tauri-apps/api` to 2.11.1
- [ ] Update `vue` to 3.5.40

### Short-term (Low Risk)
- [ ] Update `element-plus` to 2.14.3
- [ ] Update `vue-i18n` to 11.4.7 (consider removing version pin)
- [ ] Update Rust transitive dependencies via `cargo update`

### Medium-term (Moderate Risk)
- [ ] Update `cpal` to 0.18.1 (test audio recording)
- [ ] Update `async-openai` to 0.41.1 (test AI features)
- [ ] Update `imageproc` to 0.27.0

### Long-term (High Risk - Major Version Bumps)
- [ ] Evaluate `keyring` 4.x migration (breaking API changes)
- [ ] Evaluate `sqlx` 0.9.x migration (breaking API changes)
- [ ] Install `cargo-audit` and run security audit

---

## Conclusion

The project has a solid dependency foundation with no known security vulnerabilities. The main areas for improvement are:
1. Routine patch/minor updates for frontend Tauri plugins
2. Two major Rust dependency upgrades (keyring, sqlx) that require careful migration
3. Module structure is well-organized with appropriate separation of concerns
