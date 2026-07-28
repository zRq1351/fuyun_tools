---
feature: ui-beautification
status: in-progress
updated: 2026-07-28
branch: feat/ui-beautification
commits: # filled at delivery
---

# UI Beautification

## Report

## [S1] Problem

The current UI has a well-structured theme token system (`var(--fy-*)` CSS custom properties with dark/light/eye-care support), but many components use **hardcoded `rgba(255,255,255,...)` values** in scoped styles that only work in dark mode. This makes the light and eye-care themes visually broken for:

- Clipboard toolbar category pills
- Clipboard card items (both text and image)
- Dialog/modal overlays
- Shadows and drop shadows

Additionally, several visual polish areas are inconsistent across pages.

## [S2] Design

### S2.1 Theme compatibility — replace hardcoded colors

Replace all `rgba(255, 255, 255, ...)` in component styles with appropriate theme variables:

| Old pattern | New variable | Purpose |
|---|---|---|
| `rgba(255,255,255,0.04)` | `var(--fy-bg-card)` | Card/item background |
| `rgba(255,255,255,0.07)` | `var(--fy-bg-hover)` | Hover background |
| `rgba(255,255,255,0.05)` | `var(--fy-bg-input)` | Input background |
| `rgba(255,255,255,0.08)` | `var(--fy-bg-overlay)` | Overlay background |
| `rgba(255,255,255,0.12)` | `var(--fy-bg-surface)` | Surface/pill background |
| `rgba(255,255,255,0.07)` border | `var(--fy-border)` | Border color |
| `rgba(255,255,255,0.05)` border | `var(--fy-border-light)` | Light border |
| `rgba(255,255,255,0.12)` border | `var(--fy-border)` | Standard border |
| `rgba(0,0,0,0.3)` shadow | `var(--fy-shadow)` | Drop shadow |
| `rgba(0,0,0,0.15)` | `var(--fy-shadow-xs)` | Small shadow |

### S2.2 Dialog overlay consistency

Dialog overlays (`.dialog-overlay`) across launcher components use hardcoded `rgba(0,0,0,0.5)`. Keep overlay backgrounds as-is (dark modals are universal), but dialog box shadows now use `var(--fy-shadow-lg)`.

### S2.3 Shadow consistency

Replace hardcoded `rgba(0,0,0,X.X)` shadows with `var(--fy-shadow)` / `var(--fy-shadow-lg)` tokens which already have correct values per theme.

### S2.4 Category pill polish

The `.category-pill` components in ClipboardToolbar get:
- `.active` state: `var(--fy-accent)` background with `var(--fy-text-inverse)` text
- Default state: `var(--fy-bg-hover)` background with `var(--fy-border)` border
- Hover state: `var(--fy-accent-bg)` background

### S2.5 Clipboard card refinements

- Items use `var(--fy-bg-card)` + `var(--fy-border)` instead of raw rgba
- Selected state: `var(--fy-accent-bg)` background + `var(--fy-border-active)` border
- Hover state: `var(--fy-bg-hover)` background + `var(--fy-border-hover)` border

### S2.6 Search input glass effect

Search inputs get a subtle focus ring using `var(--fy-accent-bg)` box-shadow.

## [S3] Out of Scope

- Screenshot annotation canvas (uses canvas-specific rendering — not CSS)
- Recording toolbar (already uses CSS vars)
- Document manager file type color map (already uses hardcoded brand colors intentionally)
- Selection toolbar color picker defaults (user-configurable values)
- Major layout changes or component restyling beyond color replacement

## Tasks

- [x] T1: Fix ClipboardToolbar hardcoded colors — acceptance: all `rgba(255,255,255,*)` replaced with `var(--fy-*)`, category pills render correctly in all 3 themes (covers: S2.1, S2.4, S2.6)
- [x] T2: Fix ClipboardList hardcoded colors — acceptance: all `rgba(255,255,255,*)` replaced with `var(--fy-*)`, clipboard cards render correctly in all 3 themes (covers: S2.1, S2.5)
- [x] T3: Fix ImageClipboardList hardcoded colors — acceptance: all `rgba(255,255,255,*)` replaced with `var(--fy-*)` (covers: S2.1)
- [x] T4: Fix Launcher component hardcoded colors (AppGrid, AppList, CategoryManager, CommandManager) — acceptance: dialog overlays and shadows use `var(--fy-*)` tokens (covers: S2.1, S2.2, S2.3)
- [x] T5: Fix remaining hardcoded colors across other pages (pinned_image, settings, selection_toolbar, document_manager) — acceptance: all scoped-style `rgba(255,255,255,*)` and `rgba(0,0,0,*)` replaced (covers: S2.1)
