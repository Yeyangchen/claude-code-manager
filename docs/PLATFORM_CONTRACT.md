# Platform module integration

All functions return `Result<T, String>` unless stated. Structs are serializable with camelCase fields. Mutating functions must run under the application's operation lock and only after the preview plan is confirmed. Each module performs a final stale-state check. Module tests use pure data and temporary directories.

## storage

- `now_ms() -> u64`, `fingerprint(bytes: &[u8]) -> String`.
- `read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String>` and `file_fingerprint(path) -> Result<String, String>` (`missing` for absent files).
- `read_json<T: DeserializeOwned + Default>(path) -> Result<LoadResult<T>, String>` with `value: T`, `read_only: bool`.
- `write_json<T: Serialize>(path, &value)` stores `{schemaVersion: 1, data: ...}` and rejects future schemas or malformed current files.
- `atomic_write(path, bytes)`, `safe_record_path(directory, uuid)`, `redact(text) -> String`.
- Schema-zero plain JSON is migrated after preserving a byte-exact backup. Future schemas return read-only state. Callers must honor the flag.

## platform

- `user_home()`, `claude_config_dir() -> Result<PathBuf, String>`.
- `native_architecture() -> String`, `is_elevated() -> Result<bool, String>`.
- `hide_console(command: &mut std::process::Command)`.
- `expand_environment(value: &str)`, `effective_path() -> Result<String, String>`.
- `apply_fresh_user_path(command: &mut std::process::Command)` updates the child PATH from current HKLM + HKCU values.
- `broadcast_environment_changed()`.

## path_edit

- `RawPathValue { exists: bool, value: String, value_type: u32 }` preserves REG_SZ=1 or REG_EXPAND_SZ=2.
- `PathChange { id, created_at: u64, before: RawPathValue, after: RawPathValue, reason, state }`.
- `read_user_path() -> Result<RawPathValue, String>`.
- `preview_append_native() -> Result<PathChange, String>`.
- `preview_prefer(known_directories: &[PathBuf], preferred: &Path)` reorders only recognized Claude entries in their existing slots.
- `preview_remove_missing(known_directories: &[PathBuf])` removes only nonexistent entries from the caller's previously identified Claude directories.
- `apply_change(data_dir: &Path, change: &PathChange) -> Result<PathChange, String>`.
- `list_changes(data_dir) -> Result<Vec<PathChange>, String>`.
- `preview_restore(data_dir, id: &str)` and `restore_change(data_dir, id) -> Result<PathChange, String>`.
- Records are stored under `changes/path`, independent before-value backups under `backups/path`.
- States are planned/prepared/applied/restored. Prepared records remain recoverable after interruption. Restore conflicts preserve original, applied and current values.

## policy

- `PolicyRequest { mode: String, channel: Option<String>, minimum_version: Option<String>, source: String }`.
- `mode` is official/manual/hold; source winget sets the package-manager auto-update field. Empty minimum_version clears the user-level pin; None preserves it.
- `PolicySnapshot { settings_path, mode, channel, minimum_version: Option<String>, disabled_updates: bool, sources: Vec<PolicySource>, detail }`.
- `PolicySource { name, value }` only includes update-related fields.
- `PolicyChange { id, created_at: u64, settings_path, expected_fingerprint, fields: Vec<FieldChange>, mode, state, held_versions: BTreeMap<String, String> }`. `held_versions` defaults to empty for older records; the manager fills installation IDs and versions when creating a hold plan.
- `FieldChange { pointer, before: Option<Value>, after: Option<Value> }`.
- `read_policy(data_dir: &Path) -> Result<PolicySnapshot, String>`.
- `preview_policy(request: &PolicyRequest) -> Result<PolicyChange, String>`.
- `apply_policy(data_dir, change: &PolicyChange)` / `restore_policy(data_dir, id)` return PolicyChange.
- `preview_restore(data_dir, id)` and `list_changes(data_dir)` mirror path functions.
- Records under `changes/policy` hold changed fields only. Full user settings and authentication data are never copied into manager backups.
- Effective policy is inferred from inherited environment, user settings, known current-directory settings, readable managed-settings files/drop-ins, and Windows HKLM/HKCU policy registry values. The snapshot clearly identifies other-project, helper-produced and server-managed scopes as requiring the actual installation's doctor output.

## proxy

- `ProxySettings { mode: String, url: String, has_credentials: bool }`, default system.
- `load_proxy(data_dir)` / `save_proxy(data_dir, settings, username: Option<String>, password: Option<String>)` return ProxySettings.
- Username/password must both be present when changing credentials. Both empty removes credentials. Both None preserves them.
- `validate_proxy_url(value: &str) -> Result<url::Url, String>` rejects userinfo, non-proxy schemes, URL paths and query strings.
- `reqwest_client(data_dir) -> Result<reqwest::Client, String>` uses reqwest 0.12.
- `effective_proxy_url(data_dir) -> Result<Option<url::Url>, String>` selects the HTTPS updater proxy. The returned URL can contain credentials: keep it in memory only; do not log or serialize. Direct-mode callers must separately disable automatic proxy discovery. PAC and per-host updater bypass rules are not evaluated by this helper.
- `apply_command_proxy(data_dir, command: &mut std::process::Command)` configures task-specific proxy environment.
- Proxy URLs are sanitized in JSON, credentials reside in Windows Credential Manager under URL-derived targets.
- System mode inherits proxy environment variables, falling back to explicit Windows Internet Settings proxies. PAC evaluation is not implemented and should be described in compatibility notes.

## Dependencies

serde, serde_json, sha2, uuid(v4), semver, url, reqwest 0.12(json/rustls-tls/socks), winreg 0.55 (Windows), windows-sys 0.61 Windows features Foundation, Storage_FileSystem, Security, Security_Credentials, System_Threading, System_Environment, UI_WindowsAndMessaging. Test dependency tempfile 3.
