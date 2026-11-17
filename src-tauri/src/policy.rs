use crate::{platform, storage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const FIELDS: [&str; 5] = [
    "/env/DISABLE_AUTOUPDATER",
    "/env/DISABLE_UPDATES",
    "/env/CLAUDE_CODE_PACKAGE_MANAGER_AUTO_UPDATE",
    "/autoUpdatesChannel",
    "/minimumVersion",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRequest {
    pub mode: String,
    pub channel: Option<String>,
    pub minimum_version: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySource {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySnapshot {
    pub settings_path: String,
    pub mode: String,
    pub channel: String,
    pub minimum_version: Option<String>,
    pub disabled_updates: bool,
    pub sources: Vec<PolicySource>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub pointer: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PolicyChange {
    pub id: String,
    pub created_at: u64,
    pub settings_path: String,
    pub expected_fingerprint: String,
    pub fields: Vec<FieldChange>,
    pub mode: String,
    pub state: String,
}

fn read_settings(path: &Path) -> Result<Value, String> {
    let Some(bytes) = storage::read_bytes(path)? else {
        return Ok(json!({}));
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Claude 配置 JSON 无法解析，原文件已保留：{e}"))?;
    if !value.is_object() {
        return Err("Claude 配置根节点必须为 JSON 对象".into());
    }
    if value.get("env").is_some_and(|env| !env.is_object()) {
        return Err("Claude 配置 env 必须为 JSON 对象".into());
    }
    Ok(value)
}

fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        matches!(value.as_str(), Some("1" | "true")) || value.as_bool() == Some(true)
    })
}

fn settings_path() -> Result<PathBuf, String> {
    let path = platform::claude_config_dir()?.join("settings.json");
    if path.exists() {
        return fs::canonicalize(&path).map_err(|error| format!("无法解析配置文件路径：{error}"));
    }
    Ok(path)
}

fn managed_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(windows)]
    for variable in ["ProgramFiles", "ProgramW6432"] {
        if let Some(base) = std::env::var_os(variable) {
            let path = PathBuf::from(base)
                .join("ClaudeCode")
                .join("managed-settings.json");
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    #[cfg(not(windows))]
    paths.push(PathBuf::from("/etc/claude-code/managed-settings.json"));
    paths
}

fn policy_sources() -> Result<Vec<(String, Value)>, String> {
    let mut result = Vec::new();
    let mut inherited = json!({ "env": {} });
    for variable in [
        "DISABLE_AUTOUPDATER",
        "DISABLE_UPDATES",
        "CLAUDE_CODE_PACKAGE_MANAGER_AUTO_UPDATE",
    ] {
        if let Ok(value) = std::env::var(variable) {
            inherited["env"][variable] = Value::String(value);
        }
    }
    result.push(("当前进程环境".into(), inherited));
    let user = settings_path()?;
    result.push((
        format!("用户配置 · {}", user.display()),
        read_settings(&user)?,
    ));
    // Inspect only the manager's explicitly known working directory. Other project
    // scopes and server-managed policy require the installation's doctor output.
    if let Ok(directory) = std::env::current_dir() {
        for name in ["settings.json", "settings.local.json"] {
            let path = directory.join(".claude").join(name);
            if path.is_file() && path != user {
                result.push((
                    format!("当前目录配置 · {}", path.display()),
                    read_settings(&path)?,
                ));
            }
        }
    }
    for path in managed_paths() {
        if path.is_file() {
            result.push((
                format!("组织配置 · {}", path.display()),
                read_settings(&path)?,
            ));
        }
    }
    Ok(result)
}

pub fn read_policy(data_dir: &Path) -> Result<PolicySnapshot, String> {
    let mut effective = json!({});
    let mut sources = Vec::new();
    for (name, value) in policy_sources()? {
        for pointer in FIELDS {
            if let Some(value) = value.pointer(pointer) {
                set_field(&mut effective, pointer, Some(value.clone()))?;
                sources.push(PolicySource {
                    name: format!("{name} · {pointer}"),
                    value: value.to_string(),
                });
            }
        }
    }
    let disabled_auto = truthy(effective.pointer("/env/DISABLE_AUTOUPDATER"));
    let disabled_updates = truthy(effective.pointer("/env/DISABLE_UPDATES"));
    let mut mode = if disabled_auto { "manual" } else { "official" }.to_string();
    if disabled_auto {
        if let Some(record) = list_changes(data_dir)?
            .into_iter()
            .find(|record| record.state == "applied")
        {
            if record.mode == "hold" {
                mode = "hold".into();
            }
        }
    }
    Ok(PolicySnapshot {
        settings_path: settings_path()?.to_string_lossy().into_owned(), mode,
        channel: effective.get("autoUpdatesChannel").and_then(Value::as_str).unwrap_or("latest").into(),
        minimum_version: effective.get("minimumVersion").and_then(Value::as_str).map(str::to_owned),
        disabled_updates, sources,
        detail: "依据当前用户、当前目录及可读取的组织配置推导。其他项目和服务器下发策略的实际效果以目标终端 claude doctor 结果为准。".into(),
    })
}

pub fn preview_policy(request: &PolicyRequest) -> Result<PolicyChange, String> {
    if !["official", "manual", "hold"].contains(&request.mode.as_str()) {
        return Err("更新策略无效".into());
    }
    if request
        .channel
        .as_ref()
        .is_some_and(|channel| !["latest", "stable"].contains(&channel.as_str()))
    {
        return Err("更新通道无效".into());
    }
    if let Some(version) = request
        .minimum_version
        .as_ref()
        .filter(|version| !version.is_empty())
    {
        semver::Version::parse(version).map_err(|_| "最低版本号无效")?;
    }
    let path = settings_path()?;
    let fingerprint = storage::file_fingerprint(&path)?;
    let before = read_settings(&path)?;
    if fingerprint != storage::file_fingerprint(&path)? {
        return Err("配置在读取过程中被修改，请重新预览".into());
    }
    let mut changes = Vec::new();
    let disabled = (request.mode != "official").then(|| Value::String("1".into()));
    changes.push(FieldChange {
        pointer: "/env/DISABLE_AUTOUPDATER".into(),
        before: before.pointer("/env/DISABLE_AUTOUPDATER").cloned(),
        after: disabled,
    });
    if request.source == "winget" {
        changes.push(FieldChange {
            pointer: "/env/CLAUDE_CODE_PACKAGE_MANAGER_AUTO_UPDATE".into(),
            before: before
                .pointer("/env/CLAUDE_CODE_PACKAGE_MANAGER_AUTO_UPDATE")
                .cloned(),
            after: Some(Value::String(
                if request.mode == "official" { "1" } else { "0" }.into(),
            )),
        });
    }
    if let Some(channel) = &request.channel {
        changes.push(FieldChange {
            pointer: "/autoUpdatesChannel".into(),
            before: before.get("autoUpdatesChannel").cloned(),
            after: Some(Value::String(channel.clone())),
        });
    }
    if let Some(version) = &request.minimum_version {
        changes.push(FieldChange {
            pointer: "/minimumVersion".into(),
            before: before.get("minimumVersion").cloned(),
            after: (!version.is_empty()).then(|| Value::String(version.clone())),
        });
    }
    for managed in managed_paths().into_iter().filter(|path| path.is_file()) {
        let value = read_settings(&managed)?;
        if changes
            .iter()
            .any(|change| value.pointer(&change.pointer).is_some())
        {
            return Err("组织配置已设置相关更新策略，请依据组织策略执行".into());
        }
    }
    changes.retain(|change| change.before != change.after);
    Ok(PolicyChange {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: storage::now_ms(),
        settings_path: path.to_string_lossy().into_owned(),
        expected_fingerprint: fingerprint,
        fields: changes,
        mode: request.mode.clone(),
        state: "planned".into(),
    })
}

pub fn set_field(root: &mut Value, pointer: &str, value: Option<Value>) -> Result<(), String> {
    if !FIELDS.contains(&pointer) {
        return Err("该配置字段不属于更新管理范围".into());
    }
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let mut object = root.as_object_mut().ok_or("配置根节点不是对象")?;
    for part in &parts[..parts.len() - 1] {
        if !object.contains_key(*part) {
            if value.is_none() {
                return Ok(());
            }
            object.insert((*part).into(), json!({}));
        }
        object = object
            .get_mut(*part)
            .and_then(Value::as_object_mut)
            .ok_or("配置字段的父节点不是对象")?;
    }
    let key = parts.last().ok_or("配置字段无效")?;
    match value {
        Some(value) => {
            object.insert((*key).into(), value);
        }
        None => {
            object.remove(*key);
        }
    }
    Ok(())
}

fn records_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("changes").join("policy")
}

pub fn apply_policy(data_dir: &Path, change: &PolicyChange) -> Result<PolicyChange, String> {
    let path = settings_path()?;
    if path != PathBuf::from(&change.settings_path) {
        return Err("配置目录发生变化，请重新检测".into());
    }
    if storage::file_fingerprint(&path)? != change.expected_fingerprint {
        return Err("Claude 配置已被其他操作修改，请重新预览".into());
    }
    for managed in managed_paths().into_iter().filter(|path| path.is_file()) {
        let value = read_settings(&managed)?;
        if change
            .fields
            .iter()
            .any(|field| value.pointer(&field.pointer).is_some())
        {
            return Err("组织策略在执行前已设置相关字段，请重新检测".into());
        }
    }
    let mut value = read_settings(&path)?;
    for field in &change.fields {
        set_field(&mut value, &field.pointer, field.after.clone())?;
    }
    let mut record = change.clone();
    record.state = "prepared".into();
    let record_path = storage::safe_record_path(&records_dir(data_dir), &record.id)?;
    storage::write_json(&record_path, &record)?;
    if storage::file_fingerprint(&path)? != change.expected_fingerprint {
        return Err("Claude 配置在备份期间被修改，请重新预览".into());
    }
    write_settings(&path, &value)?;
    let written = read_settings(&path)?;
    if change
        .fields
        .iter()
        .any(|field| written.pointer(&field.pointer).cloned() != field.after)
    {
        return Err("配置写入后发生变化，变更台账已保留，请重新核查".into());
    }
    record.state = "applied".into();
    storage::write_json(&record_path, &record)?;
    Ok(record)
}

pub fn preview_restore(data_dir: &Path, id: &str) -> Result<PolicyChange, String> {
    let record_path = storage::safe_record_path(&records_dir(data_dir), id)?;
    if !record_path.is_file() {
        return Err("策略变更记录不存在".into());
    }
    let loaded = storage::read_json::<PolicyChange>(&record_path)?;
    if loaded.read_only {
        return Err("策略记录来自更新版本，当前只读".into());
    }
    let record = loaded.value;
    if !["prepared", "applied"].contains(&record.state.as_str()) {
        return Err("该策略变更已经撤销".into());
    }
    if settings_path()? != PathBuf::from(&record.settings_path) {
        return Err("该变更属于不同的 Claude 配置目录".into());
    }
    let current = read_settings(Path::new(&record.settings_path))?;
    verify_restorable(&current, &record.fields)?;
    Ok(record)
}

fn verify_restorable(current: &Value, fields: &[FieldChange]) -> Result<(), String> {
    for field in fields {
        let value = current.pointer(&field.pointer).cloned();
        if value != field.after && value != field.before {
            return Err(format!(
                "字段 {} 已被用户或其他程序修改，已保留该修改，请审阅差异",
                field.pointer
            ));
        }
    }
    Ok(())
}

pub fn restore_policy(data_dir: &Path, id: &str) -> Result<PolicyChange, String> {
    let mut record = preview_restore(data_dir, id)?;
    let path = PathBuf::from(&record.settings_path);
    let fingerprint = storage::file_fingerprint(&path)?;
    let mut value = read_settings(&path)?;
    verify_restorable(&value, &record.fields)?;
    for field in &record.fields {
        set_field(&mut value, &field.pointer, field.before.clone())?;
    }
    if storage::file_fingerprint(&path)? != fingerprint {
        return Err("配置在还原期间被修改，请重新检查".into());
    }
    write_settings(&path, &value)?;
    record.state = "restored".into();
    storage::write_json(
        &storage::safe_record_path(&records_dir(data_dir), id)?,
        &record,
    )?;
    Ok(record)
}

fn write_settings(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    storage::atomic_write(path, &bytes)
}

pub fn list_changes(data_dir: &Path) -> Result<Vec<PolicyChange>, String> {
    let directory = records_dir(data_dir);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut changes = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            changes.push(storage::read_json::<PolicyChange>(&entry.path())?.value);
        }
    }
    changes.sort_by_key(|change| std::cmp::Reverse(change.created_at));
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_preserve_unrelated_keys_and_nested_values() {
        let mut value = json!({"model":"custom", "env":{"CUSTOM_VAR":"keep"}, "permissions":{"allow":["Read"]}});
        set_field(&mut value, "/env/DISABLE_AUTOUPDATER", Some(json!("1"))).unwrap();
        assert_eq!(value["env"]["CUSTOM_VAR"], "keep");
        assert_eq!(value["permissions"]["allow"], json!(["Read"]));
        set_field(&mut value, "/env/DISABLE_AUTOUPDATER", None).unwrap();
        assert!(value["env"].get("DISABLE_AUTOUPDATER").is_none());
    }

    #[test]
    fn restore_detects_changes_to_owned_fields() {
        let fields = vec![FieldChange {
            pointer: "/minimumVersion".into(),
            before: None,
            after: Some(json!("2.1.1")),
        }];
        assert!(verify_restorable(&json!({"minimumVersion":"2.1.2"}), &fields).is_err());
        assert!(
            verify_restorable(&json!({"minimumVersion":"2.1.1", "model":"new"}), &fields).is_ok()
        );
    }

    #[test]
    fn edits_reject_fields_outside_update_management() {
        assert!(set_field(&mut json!({}), "/permissions/allow", Some(json!(["*"]))).is_err());
        assert!(set_field(
            &mut json!({"env":42}),
            "/env/DISABLE_AUTOUPDATER",
            Some(json!("1"))
        )
        .is_err());
    }
}
