# Launcher Module Review & Optimization Report

**Task:** Task 8 - 应用启动器模块审查与优化  
**Scope:** [S4.5, S5.3, S5.6]  
**Date:** 2026-07-23  

## 1. Architecture Overview

### Frontend (Vue 3)
```
src/pages/launcher/
├── App.vue                          # Main window, orchestrates all sub-components
├── main.js                          # Entry point
├── components/
│   ├── SearchBox.vue                # Search input with clear button
│   ├── AppList.vue                  # List view with collapsible sections
│   ├── AppGrid.vue                  # Grid/category view with drag-and-drop
│   ├── CategoryManager.vue          # Category CRUD dialog
│   ├── CommandManager.vue           # Custom command management
│   └── ResultList.vue / ResultItem.vue  # Search results display
└── composables/
    └── useLauncherSearch.js         # Search logic, command matching, app execution
```

### Backend (Rust / Tauri)
```
src-tauri/src/
├── ui/commands_launcher.rs          # Tauri command handlers (IPC bridge)
├── services/
│   ├── app_scanner.rs               # File system scanning, shortcut parsing, app launch
│   ├── app_store.rs                 # App persistence, system app detection
│   ├── launcher_config.rs           # Configuration management, categories, commands
│   └── launcher_db.rs               # SQLite database operations
```

### Data Flow
```
User types query
  → SearchBox.vue emits 'input' event
  → App.vue handleSearch() calls useLauncherSearch.search()
  → Backend app_scanner::search_apps() (scans filesystem EVERY TIME)
  → Returns filtered results
  → App.vue displays results in commandResults/displayApps

User launches app
  → handleSelect() → useLauncherSearch.executeAction()
  → invoke('launch_app') → commands_launcher.rs
  → app_scanner::launch_app() → std::process::Command or ShellExecuteEx
```

## 2. Performance Findings

### 2.1 Critical Performance Issues

| # | Severity | File | Issue | Impact |
|---|----------|------|-------|--------|
| C1 | **Critical** | `app_scanner.rs:152-172` | `search_apps()` calls `scan_apps_by_category()` on EVERY search query, re-scanning the entire Start Menu filesystem | 100-500ms per keystroke, blocks UI |
| C2 | **Critical** | `commands_launcher.rs:29-47` | `search_launcher_items()` also calls `search_apps()` which re-scans filesystem | Double scanning on search |
| C3 | **High** | `App.vue:186-201` | `loadAllApps()` is called on every launcher show (`show-launcher` event) | Unnecessary reloads if data unchanged |
| C4 | **High** | `App.vue:222-238` | `handleRefresh()` does full scan + reload config + reload icons sequentially | Slow refresh (2-5 seconds) |
| C5 | **High** | `useLauncherSearch.js:14-33` | `loadCustomCommands()` calls `invoke('get_launcher_config')` every time | Redundant IPC calls |

### 2.2 Code Structure Issues

| # | Severity | File | Issue | Impact |
|---|----------|------|-------|--------|
| S1 | Medium | `app_scanner.rs:225-283` | `launch_app()` and `launch_app_with_args()` have ~80% duplicated code | Maintenance burden |
| S2 | Medium | `AppList.vue:321-329` | `removeApp()` function is identical to `AppGrid.vue:416-426` | Code duplication |
| S3 | Medium | `AppList.vue:404-413` | `removeFromCategory()` is identical to `AppGrid.vue:404-413` | Code duplication |
| S4 | Medium | `AppList.vue:332-421` | `confirmAddCommand()` is identical to `AppGrid.vue:461-529` | Code duplication |
| S5 | Low | `AppList.vue:260-267` | `loadCategories()` is called on mount AND on every `apps` prop change | Unnecessary reloads |

### 2.3 Search Algorithm Issues

| # | Severity | File | Issue | Impact |
|---|----------|------|-------|--------|
| A1 | Medium | `useLauncherSearch.js:44-60` | `searchApps()` uses simple `includes()` matching, no fuzzy search | Poor search quality |
| A2 | Medium | `useLauncherSearch.js:36-42` | `findCommand()` checks both `startsWith` and `startsWith` (redundant) | Logic bug |
| A3 | Low | `app_scanner.rs:152-172` | `search_apps()` sorts by starts_with but doesn't rank by relevance | Suboptimal results |

## 3. Recommendations

### 3.1 Immediate Fixes (High Priority)

1. **Cache scanned apps in memory** - Store scan results in a `static` or `OnceCell` to avoid re-scanning on every search
2. **Debounce search input** - Add debounce to `handleSearch()` to reduce IPC calls
3. **Extract shared logic** - Create composables for duplicated code (removeApp, removeFromCategory, confirmAddCommand)

### 3.2 Architecture Improvements (Medium Priority)

1. **Implement incremental scanning** - Only re-scan directories that have changed
2. **Add search index** - Build a lightweight search index for faster lookups
3. **Virtualize large lists** - Use virtual scrolling for apps list (100+ items)

### 3.3 Code Quality (Low Priority)

1. **Consolidate duplicate functions** - Merge `launch_app` and `launch_app_with_args` into a single function
2. **Add error boundaries** - Better error handling for scan failures
3. **Add loading states** - Show progress indicators during scan

## 4. Implementation Plan

### Phase 1: Critical Performance Fixes
- [ ] Add in-memory app cache to `app_scanner.rs`
- [ ] Implement search debounce in `App.vue`
- [ ] Cache custom commands in `useLauncherSearch.js`

### Phase 2: Code Deduplication
- [ ] Extract shared logic to `useAppActions.js` composable
- [ ] Merge duplicate launch functions in `app_scanner.rs`

### Phase 3: Search Quality
- [ ] Implement fuzzy search algorithm
- [ ] Add search result ranking

## 5. Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| In-memory cache | Low - Cache invalidation on refresh | Clear cache on `scan_and_save_apps` |
| Debounce search | Low - Standard UX pattern | 150ms debounce delay |
| Code extraction | Medium - May break existing functionality | Thorough testing of all code paths |

## 6. Testing Strategy

1. **Performance testing** - Measure search latency before/after cache implementation
2. **Functional testing** - Verify all launch methods work (direct, with args, shortcuts)
3. **Edge cases** - Test with 0 apps, 1000+ apps, special characters in search
4. **Regression testing** - Ensure all existing features continue to work
