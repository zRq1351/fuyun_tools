# Security Review Report

**Project**: fuyun_tools (v0.8.1)  
**Platform**: Tauri 2 (Rust backend + Vue 3 frontend)  
**Date**: 2026-07-23  
**Scope**: API key storage, data privacy, dependency security, input validation, IPC security

---

## 1. API Key Storage

### Current Implementation

API keys are stored using the **OS-native Credential Manager** via the `keyring` crate (v3.6.3, Windows Credential Manager backend). This is a strong, industry-standard approach.

**Key files**: `src-tauri/src/utils/settings_model.rs:443-504`

- Keys are stored per-provider using `keyring::Entry::new("fuyun_tools", "api_key_{provider}")`.
- The `encrypted_api_key` field in config is cleared on migration, meaning plaintext keys are no longer persisted in config files.
- A legacy XOR migration path exists (`migrate_legacy_api_keys`) that decrypts old config-based keys and moves them to OS keyring.

### Positive Findings

- **OS keyring usage**: Keys are stored in Windows Credential Manager, which encrypts at rest using DPAPI. This is the correct approach for desktop apps.
- **Retry logic**: Both `set_provider_api_key` and `get_provider_api_key` have 3-attempt retry loops with 100ms delays, handling transient keyring errors gracefully.
- **Key cleanup**: When `api_key` is empty, the credential entry is deleted via `entry.delete_credential()`.
- **No plaintext config**: The `encrypted_api_key` field is cleared after migration; keys are not serialized to disk in plaintext.

### Concerns

| Severity | Issue | Detail |
|----------|-------|--------|
| **Low** | Legacy XOR key in source | `LEGACY_ENCRYPTION_KEY` (`b"fuyun_tools_encryption_key_2025!"`) is hardcoded at `settings_model.rs:13`. While only used for migration of old configs, the key is trivially extractable from the binary. Since it only decrypts legacy data being migrated to keyring, the practical risk is low. |
| **Low** | Error message leakage | `get_provider_api_key` returns error messages from `keyring` to the frontend. While these are generic keyring errors (not key values), they could reveal implementation details. |
| **Info** | API key passed in memory | The `AIConfig` struct holds `api_key: String` in `ai_client.rs:63`. This is unavoidable (must be in memory to make API calls), but the key lives on the heap until dropped. No explicit zeroing is performed. |

### Recommendation

- Consider removing the legacy XOR key constant after the migration window (e.g., in a future major version).
- Error messages from keyring operations should be logged but not propagated verbatim to the frontend.

---

## 2. Data Privacy

### What Data Is Collected and Stored

| Data Type | Storage Location | Encrypted? | Retention |
|-----------|-----------------|------------|-----------|
| API keys | OS Credential Manager | Yes (DPAPI) | Until user deletes |
| Clipboard text history | SQLite (`history.db`) | No | Configurable max items |
| Image clipboard history | SQLite (`image_history.db`) + blob files | No | Configurable |
| App settings | Local config file | No | Until user deletes |
| Custom prompts | Local config file | No | Until user deletes |
| Document manager paths | SQLite (`document_manager.db`) | No | Until user deletes |
| AI chat messages | Memory only (not persisted) | N/A | Session only |

### Positive Findings

- **No telemetry**: No analytics, tracking, or remote logging is implemented.
- **Local-only data**: All data stays on the user's machine.
- **Configurable retention**: Users can set `max_items` for clipboard history.
- **Clear history**: `clear_history()` and `clear_history_by_mode()` functions allow users to delete their data.
- **DOMPurify**: The frontend uses `DOMPurify.sanitize()` for rendering markdown/HTML content, preventing XSS from AI-generated content.

### Concerns

| Severity | Issue | Detail |
|----------|-------|--------|
| **Medium** | Clipboard history unencrypted | Text and image clipboard history is stored in plaintext SQLite databases. If the machine is compromised or the file is accessed, all clipboard contents are exposed. |
| **Low** | No data-at-rest encryption | The SQLite databases (`history.db`, `image_history.db`) have no encryption. This is a common tradeoff in desktop apps but should be documented. |
| **Info** | Local path exposure | Document manager stores absolute filesystem paths. This reveals directory structure but is necessary for the feature. |

### Recommendation

- Consider offering optional encryption for clipboard history databases (e.g., using SQLCipher).
- Document that clipboard history is stored in plaintext SQLite databases on disk.

---

## 3. Dependency Security

### Rust Dependencies (Cargo.toml)

| Crate | Version | Notes |
|-------|---------|-------|
| `tauri` | 2.x | Core framework, actively maintained |
| `keyring` | 3.6.3 | OS credential store, well-maintained |
| `sqlx` | 0.8.6 | SQLite, uses parameterized queries by default |
| `reqwest` | 0.13.3 | HTTP client with rustls (no OpenSSL) |
| `async-openai` | 0.38.2 | OpenAI API client |
| `image` | 0.25.10 | Image processing |
| `windows` | 0.62.2 | Windows API bindings |
| `opencv` | 0.98.2 | Optional, behind feature flag |
| `pdf-extract` | 0.10.0 | PDF text extraction |

### Frontend Dependencies (package.json)

| Package | Version | Notes |
|---------|---------|-------|
| `vue` | 3.5.32 | Core framework |
| `element-plus` | 2.13.7 | UI component library |
| `dompurify` | 3.4.1 | HTML sanitization |
| `marked` | 18.0.0 | Markdown parser |
| `highlight.js` | 11.11.1 | Syntax highlighting |
| `@tauri-apps/api` | 2.10.1 | Tauri frontend API |

### Positive Findings

- **rustls over OpenSSL**: `reqwest` and `async-openai` use `rustls` feature, avoiding native OpenSSL dependency and its potential vulnerabilities.
- **DOMPurify**: Used in `FormattedContent.vue` and `result_display/App.vue` to sanitize AI-generated HTML/markdown before rendering.
- **Parameterized SQL**: The codebase uses `sqlx` parameterized queries (`?1`, `?2`) for user-facing data, preventing SQL injection.
- **No known critical CVEs**: Dependencies are at recent versions. The `tauri` 2.x branch is actively maintained with security patches.

### Concerns

| Severity | Issue | Detail |
|----------|-------|--------|
| **Low** | Dynamic table names in SQL | `database.rs:71-80` uses `format!()` for table names in `CREATE TEMP TABLE` and `DELETE FROM`. However, these are internal temp table names (not user input), so injection risk is minimal. |
| **Low** | `opentrusted` CSP comment | The CSP policy includes `style-src-attr 'unsafe-inline'` which allows inline styles. This is a minor relaxation but necessary for some CSS frameworks. |
| **Info** | Optional opencv dependency | `opencv` is behind a feature flag (`longshot-opencv`). If enabled, it adds a large native dependency surface. |

### Recommendation

- Run `cargo audit` and `npm audit` periodically to catch newly disclosed vulnerabilities.
- Monitor the `tauri` security advisories channel.

---

## 4. Input Validation

### Rust Backend

**Positive findings**:

- **Path validation**: `commands_document.rs:12-24` validates doc root paths using `canonicalize()` and checks `is_dir()`.
- **Path traversal prevention**: `backup_restore.rs:448-453` rejects blob paths containing `..`, starting with `/`, or containing `:`.
- **Category name validation**: `commands_document.rs:37-49` rejects names with filesystem-unsafe characters (`< > : " / \ | ? *`).
- **Clipboard content size**: `ClipboardManager` has a configurable `max_items` limit and uses bloom filters for deduplication, preventing unbounded growth.
- **Input trimming**: Category names are trimmed before use (`category.trim().to_string()`).

### Frontend

- **API key input**: The `AISettings.vue` component uses `type="password" show-password` for the API key field, preventing shoulder-surfing.
- **DOMPurify sanitization**: AI-generated content is sanitized before rendering in `FormattedContent.vue:101,114,125,143`.

### Concerns

| Severity | Issue | Detail |
|----------|-------|--------|
| **Medium** | `open_file` lacks path validation | `app_scanner.rs:368-373` checks `PathBuf::from(path).exists()` but does not validate the path is within expected directories. A malicious path (e.g., `C:\Windows\System32\cmd.exe`) could be opened if passed through the IPC. |
| **Low** | No URL validation for AI API URL | The `apiUrl` field in `AISettings.vue` accepts any URL. A malicious URL could be set, though this is user-configured and local. |
| **Info** | Custom provider name unvalidated | The `customProviderName` field has no length or character validation beyond what the UI enforces. |

### Recommendation

- Add path allowlisting for `open_file` — restrict to user directories or known application paths.
- Validate AI API URLs against a regex or URL parser before use.

---

## 5. IPC Security

### Tauri 2 Capabilities

The app uses Tauri 2's capability-based permission system (`capabilities/default.json`):

- **Scoped to specific windows**: Permissions are granted to named windows (clipboard, settings, launcher, etc.).
- **Minimal permissions**: Only necessary core and plugin permissions are enabled.
- **No `core:shell:default`**: Shell execution is not exposed to the frontend.
- **No `core:fs:default`**: Direct filesystem access is not exposed; file operations go through Rust commands.
- **Clipboard permissions**: `clipboard-manager:default` plus explicit `allow-read-image` and `allow-write-image`.

### Positive Findings

- **`withGlobalTauri: false`**: The `window.__TAURI__` global is not exposed, reducing attack surface.
- **No shell plugin**: The frontend cannot execute arbitrary shell commands.
- **Window isolation**: Each window type has specific capabilities; not all commands are available to all windows.
- **CSP enforcement**: `default-src 'self'; script-src 'self'; connect-src 'self' tauri://ipc` — strict content security policy prevents loading external scripts or making arbitrary network requests.

### Concerns

| Severity | Issue | Detail |
|----------|-------|--------|
| **Low** | Broad window patterns | `pinned_image_*` and `ocr_text_pinned_image_*` use wildcard window labels. This is intentional for dynamic windows but broadens the permission surface. |
| **Info** | Custom commands exposed | The app registers ~100+ `#[tauri::command]` handlers. Each is a potential IPC attack surface. Most are benign (clipboard operations, settings), but some interact with the filesystem. |

### Recommendation

- Audit the `open_file`, `open_app_directory`, and document management commands for path traversal.
- Consider adding rate limiting or input size limits for IPC commands that process user data.

---

## 6. Additional Security Observations

### 6.1 SQL Injection

The codebase uses `sqlx` with parameterized queries for all user-facing data. Internal temp table operations use `format!()` for table names, but these are hardcoded strings (not user input). **No SQL injection vulnerabilities found.**

### 6.2 XSS Prevention

- `DOMPurify.sanitize()` is applied to all AI-generated markdown/HTML before rendering.
- Custom `escapeHtml()` function in `result_display/App.vue` escapes HTML entities.
- `rel="noopener noreferrer nofollow"` is added to external links.
- CSP `script-src 'self'` prevents inline script execution.

### 6.3 Secure Update Mechanism

- Tauri updater uses a **public key** for signature verification (`tauri.conf.json:50`).
- Updates are fetched from GitHub releases over HTTPS.
- `createUpdaterArtifacts: true` ensures signed update bundles.

### 6.4 Database Location

- `history.db` is stored relative to the executable path (`database.rs:50-55`), not in a standard user data directory. This means the database is in the installation directory, which may have different permissions than `%APPDATA%`.

### 6.5 Error Information Disclosure

- Some error messages propagate detailed internal errors to the frontend (e.g., keyring errors, database errors). While not critical, this could aid an attacker in understanding the system.

---

## 7. Summary of Recommendations

| Priority | Recommendation |
|----------|---------------|
| **High** | Add path validation/allowlisting for `open_file` and `open_app_directory` commands |
| **Medium** | Consider encrypting clipboard history databases (SQLCipher) |
| **Medium** | Sanitize error messages before returning to frontend |
| **Low** | Validate AI API URLs before use |
| **Low** | Remove legacy XOR encryption key after migration window |
| **Low** | Document that clipboard history is stored in plaintext SQLite |
| **Info** | Run `cargo audit` and `npm audit` in CI/CD pipeline |
| **Info** | Consider moving database files to `%APPDATA%` for better permission isolation |

---

## 8. Overall Assessment

The fuyun_tools project demonstrates a **solid security posture** for a desktop application:

- **Strong**: OS keyring for credential storage, CSP enforcement, DOMPurify sanitization, parameterized SQL, no telemetry, local-only data.
- **Acceptable**: Plaintext clipboard history (common tradeoff), broad IPC surface (inherent to Tauri apps).
- **Needs Attention**: `open_file` path validation, error message sanitization.

No critical or high-severity vulnerabilities were identified. The application follows security best practices for its category (local desktop tool with clipboard management and AI integration).
