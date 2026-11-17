use crate::{platform, storage};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPathValue {
    pub exists: bool,
    pub value: String,
    pub value_type: u32,
}

impl Default for RawPathValue {
    fn default() -> Self {
        Self {
            exists: false,
            value: String::new(),
            value_type: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PathChange {
    pub id: String,
    pub created_at: u64,
    pub before: RawPathValue,
    pub after: RawPathValue,
    pub reason: String,
    pub state: String,
}

pub fn decode_registry_string(bytes: &[u8]) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("PATH 注册表值含无效 UTF-16 数据".into());
    }
    let mut words: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    if words.last() == Some(&0) {
        words.pop();
    }
    if words.contains(&0) {
        return Err("PATH 注册表值含内部空字符，需手动检查".into());
    }
    String::from_utf16(&words).map_err(|_| "PATH 注册表值含无效 Unicode".into())
}

#[cfg(windows)]
pub fn read_user_path() -> Result<RawPathValue, String> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ},
        RegKey,
    };
    let environment = match RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment") {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RawPathValue::default()),
        Err(e) => return Err(format!("无法读取用户环境注册表：{e}")),
    };
    match environment.get_raw_value("Path") {
        Ok(raw) => {
            if raw.vtype != REG_SZ && raw.vtype != REG_EXPAND_SZ {
                return Err("用户 PATH 类型必须为 REG_SZ 或 REG_EXPAND_SZ".into());
            }
            Ok(RawPathValue {
                exists: true,
                value: decode_registry_string(&raw.bytes)?,
                value_type: raw.vtype as u32,
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RawPathValue::default()),
        Err(e) => Err(format!("无法读取用户 PATH：{e}")),
    }
}

#[cfg(not(windows))]
pub fn read_user_path() -> Result<RawPathValue, String> {
    Err("用户级 PATH 管理适用于 Windows".into())
}

fn normalized_entry(entry: &str) -> String {
    entry
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

fn matching_entry(entry: &str, directory: &Path, expand: bool) -> bool {
    let expanded = if expand {
        platform::expand_environment(entry).unwrap_or_else(|_| entry.into())
    } else {
        entry.into()
    };
    normalized_entry(&expanded) == normalized_entry(&directory.to_string_lossy())
}

pub fn preview_append_native() -> Result<PathChange, String> {
    let native = platform::user_home()?.join(".local").join("bin");
    append_native_to(&read_user_path()?, &native)
}

pub fn append_native_to(before: &RawPathValue, native: &Path) -> Result<PathChange, String> {
    let already_present = before.value.split(';').any(|entry| {
        (before.value_type == 2 && normalized_entry(entry) == "%userprofile%\\.local\\bin")
            || matching_entry(entry, native, before.value_type == 2)
    });
    let mut after = before.clone();
    if !already_present {
        let entry = if before.value_type == 2 {
            "%USERPROFILE%\\.local\\bin".to_string()
        } else {
            native.to_string_lossy().into_owned()
        };
        if !after.value.is_empty() && !after.value.ends_with(';') {
            after.value.push(';');
        }
        after.value.push_str(&entry);
        after.exists = true;
    }
    validate_length(&after.value)?;
    Ok(PathChange {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: storage::now_ms(),
        before: before.clone(),
        after,
        reason: "将原生 Claude Code 启动目录加入用户 PATH".into(),
        state: "planned".into(),
    })
}

/// Reorder only recognized Claude-related entries, keeping other entries at their indexes.
pub fn preview_prefer(
    known_directories: &[PathBuf],
    preferred: &Path,
) -> Result<PathChange, String> {
    prefer_in(&read_user_path()?, known_directories, preferred)
}

pub fn prefer_in(
    before: &RawPathValue,
    known_directories: &[PathBuf],
    preferred: &Path,
) -> Result<PathChange, String> {
    if !known_directories
        .iter()
        .any(|directory| directory == preferred)
    {
        return Err("目标目录未被检测为 Claude Code 安装目录".into());
    }
    let mut entries: Vec<String> = before.value.split(';').map(str::to_owned).collect();
    let slots: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            known_directories
                .iter()
                .any(|directory| matching_entry(entry, directory, before.value_type == 2))
                .then_some(index)
        })
        .collect();
    if let Some(preferred_index) = slots
        .iter()
        .position(|index| matching_entry(&entries[*index], preferred, before.value_type == 2))
    {
        let mut related: Vec<String> = slots.iter().map(|index| entries[*index].clone()).collect();
        let preferred_entry = related.remove(preferred_index);
        related.insert(0, preferred_entry);
        for (slot, entry) in slots.into_iter().zip(related) {
            entries[slot] = entry;
        }
    } else {
        return Err("该目录尚未存在于用户 PATH，请先处理安装目录配置".into());
    }
    let after = RawPathValue {
        value: entries.join(";"),
        ..before.clone()
    };
    Ok(PathChange {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: storage::now_ms(),
        before: before.clone(),
        after,
        reason: "调整已识别的 Claude Code PATH 条目顺序".into(),
        state: "planned".into(),
    })
}

/// The caller supplies paths established by discovery or its own change ledger.
pub fn preview_remove_missing(known_directories: &[PathBuf]) -> Result<PathChange, String> {
    let before = read_user_path()?;
    let entries: Vec<&str> = before
        .value
        .split(';')
        .filter(|entry| {
            !known_directories.iter().any(|directory| {
                matching_entry(entry, directory, before.value_type == 2) && !directory.exists()
            })
        })
        .collect();
    let after = RawPathValue {
        value: entries.join(";"),
        ..before.clone()
    };
    Ok(PathChange {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: storage::now_ms(),
        before,
        after,
        reason: "移除已经不存在的 Claude Code 安装目录条目".into(),
        state: "planned".into(),
    })
}

fn validate_length(value: &str) -> Result<(), String> {
    if value.encode_utf16().count() >= 32767 {
        return Err("修改后的用户 PATH 超过 Windows 环境变量长度限制".into());
    }
    if value.contains('\0') {
        return Err("PATH 含空字符".into());
    }
    Ok(())
}

fn records_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("changes").join("path")
}

pub fn apply_change(data_dir: &Path, change: &PathChange) -> Result<PathChange, String> {
    if read_user_path()? != change.before {
        return Err("用户 PATH 已被其他操作修改，请重新预览差异".into());
    }
    validate_length(&change.after.value)?;
    let path = storage::safe_record_path(&records_dir(data_dir), &change.id)?;
    let mut record = change.clone();
    record.state = "prepared".into();
    storage::write_json(&path, &record)?;
    let backup = storage::safe_record_path(&data_dir.join("backups").join("path"), &record.id)?;
    storage::write_json(&backup, &record.before)?;
    if read_user_path()? != record.before {
        return Err("PATH 在备份期间发生改变，请重新预览".into());
    }
    write_user_path(&record.after)?;
    record.state = "applied".into();
    storage::write_json(&path, &record)?;
    platform::broadcast_environment_changed();
    if read_user_path()? != record.after {
        return Err("PATH 写入后发生外部变化，已保留备份和变更记录".into());
    }
    Ok(record)
}

pub fn preview_restore(data_dir: &Path, id: &str) -> Result<PathChange, String> {
    let path = storage::safe_record_path(&records_dir(data_dir), id)?;
    if !path.is_file() {
        return Err("PATH 变更记录不存在".into());
    }
    let loaded = storage::read_json::<PathChange>(&path)?;
    if loaded.read_only {
        return Err("变更记录来自更新版本，当前只读".into());
    }
    let record = loaded.value;
    if !["prepared", "applied"].contains(&record.state.as_str()) {
        return Err("此 PATH 变更已经撤销或无法还原".into());
    }
    let current = read_user_path()?;
    if current != record.after && current != record.before {
        return Err("PATH 已被外部修改，原值和管理器写入值已保留，请审阅三方差异".into());
    }
    Ok(record)
}

pub fn restore_change(data_dir: &Path, id: &str) -> Result<PathChange, String> {
    let mut record = preview_restore(data_dir, id)?;
    let current = read_user_path()?;
    if current != record.after && current != record.before {
        return Err("PATH 在还原前发生变化，请重新检查".into());
    }
    if current == record.after {
        write_user_path(&record.before)?;
    }
    if read_user_path()? != record.before {
        return Err("PATH 还原后发生外部变化，原变更记录已保留".into());
    }
    record.state = "restored".into();
    storage::write_json(
        &storage::safe_record_path(&records_dir(data_dir), id)?,
        &record,
    )?;
    platform::broadcast_environment_changed();
    Ok(record)
}

pub fn list_changes(data_dir: &Path) -> Result<Vec<PathChange>, String> {
    let directory = records_dir(data_dir);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().is_some_and(|ext| ext == "json") {
            records.push(storage::read_json::<PathChange>(&entry.path())?.value);
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.created_at));
    Ok(records)
}

#[cfg(windows)]
fn write_user_path(value: &RawPathValue) -> Result<(), String> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ},
        RegKey, RegValue,
    };
    let (environment, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags("Environment", KEY_SET_VALUE)
        .map_err(|e| format!("无法打开用户环境注册表：{e}"))?;
    if !value.exists {
        match environment.delete_value("Path") {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("删除用户 PATH 失败：{error}")),
        }
    }
    let value_type = match value.value_type {
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        _ => return Err("PATH 注册表类型不受支持".into()),
    };
    let bytes = value
        .value
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    environment
        .set_raw_value(
            "Path",
            &RegValue {
                bytes,
                vtype: value_type,
            },
        )
        .map_err(|e| format!("写入用户 PATH 失败：{e}"))
}

#[cfg(not(windows))]
fn write_user_path(_value: &RawPathValue) -> Result<(), String> {
    Err("用户级 PATH 管理适用于 Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appending_preserves_raw_variables_and_value_type() {
        let before = RawPathValue {
            exists: true,
            value: "%JAVA_HOME%\\bin;;C:\\Tools;".into(),
            value_type: 2,
        };
        let change = append_native_to(&before, Path::new("C:\\Users\\Test\\.local\\bin")).unwrap();
        assert_eq!(
            change.after.value,
            "%JAVA_HOME%\\bin;;C:\\Tools;%USERPROFILE%\\.local\\bin"
        );
        assert_eq!(change.after.value_type, 2);
        assert_eq!(change.before, before);
    }

    #[test]
    fn reg_sz_keeps_its_type_and_uses_absolute_path() {
        let before = RawPathValue {
            exists: true,
            value: "C:\\Tools".into(),
            value_type: 1,
        };
        let change = append_native_to(&before, Path::new("C:\\Users\\Test\\.local\\bin")).unwrap();
        assert_eq!(change.after.value_type, 1);
        assert!(!change.after.value.contains("%USERPROFILE%"));
        let literal = RawPathValue {
            value: "%USERPROFILE%\\.local\\bin".into(),
            ..before
        };
        let change = append_native_to(&literal, Path::new("C:\\Users\\Test\\.local\\bin")).unwrap();
        assert_eq!(
            change.after.value,
            "%USERPROFILE%\\.local\\bin;C:\\Users\\Test\\.local\\bin"
        );
    }

    #[test]
    fn appending_is_idempotent_and_preserves_spelling() {
        let before = RawPathValue {
            exists: true,
            value: "%UserProfile%\\.local\\bin;C:\\Tools".into(),
            value_type: 2,
        };
        let change = append_native_to(&before, Path::new("C:\\Users\\Test\\.local\\bin")).unwrap();
        assert_eq!(change.before, change.after);
    }

    #[test]
    fn invalid_utf16_and_overlong_values_are_rejected() {
        assert!(decode_registry_string(&[0]).is_err());
        assert!(validate_length(&"x".repeat(32767)).is_err());
    }

    #[test]
    fn reordering_keeps_unrelated_entries_at_their_original_positions() {
        let before = RawPathValue {
            exists: true,
            value: "C:\\Tools;C:\\npm;C:\\Unrelated;C:\\native;".into(),
            value_type: 1,
        };
        let known = vec![PathBuf::from("C:\\npm"), PathBuf::from("C:\\native")];
        let change = prefer_in(&before, &known, Path::new("C:\\native")).unwrap();
        assert_eq!(
            change.after.value,
            "C:\\Tools;C:\\native;C:\\Unrelated;C:\\npm;"
        );
        assert!(prefer_in(&before, &known, Path::new("C:\\Tools")).is_err());
    }

    /// Mutates the disposable hosted runner's HKCU only. Normal unit test runs
    /// skip this test; even an explicit local --ignored invocation fails before
    /// any registry access. The restore guard also runs during panic unwinding.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires explicitly opted-in disposable GitHub-hosted Windows runner"]
    fn isolated_path_smoke() {
        use winreg::{
            enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
            RegKey, RegValue,
        };
        assert!(
            std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
                && std::env::var("CCM_RUN_INSTALL_SMOKE").as_deref() == Ok("1")
                && std::env::var("RUNNER_ENVIRONMENT").as_deref() == Ok("github-hosted")
                && std::env::var("RUNNER_OS").as_deref() == Ok("Windows"),
            "registry smoke test is permitted only on an opted-in disposable GitHub-hosted Windows runner"
        );

        fn exact_value() -> std::io::Result<Option<RegValue>> {
            let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment") {
                Ok(key) => key,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            };
            match key.get_raw_value("Path") {
                Ok(value) => Ok(Some(value)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        }

        struct RestoreOriginal(Option<RegValue>);
        impl RestoreOriginal {
            fn restore(&self) -> std::io::Result<()> {
                let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
                    .create_subkey_with_flags("Environment", KEY_SET_VALUE)?;
                match &self.0 {
                    Some(original) => key.set_raw_value("Path", original)?,
                    None => match key.delete_value("Path") {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    },
                }
                platform::broadcast_environment_changed();
                Ok(())
            }
        }
        impl Drop for RestoreOriginal {
            fn drop(&mut self) {
                let _ = self.restore();
            }
        }

        let restore_original = RestoreOriginal(exact_value().expect("read runner's original PATH"));
        let before = read_user_path().expect("read raw PATH snapshot");
        let directory = tempfile::tempdir().expect("create isolated ledger directory");
        let change = preview_append_native().expect("preview native PATH append");
        assert_eq!(change.before, before);
        apply_change(directory.path(), &change).expect("apply PATH plan");
        assert_eq!(read_user_path().unwrap(), change.after);
        let expected_bytes: Vec<u8> = change
            .after
            .value
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        let applied = exact_value().unwrap().expect("applied PATH exists");
        assert_eq!(applied.vtype as u32, change.after.value_type);
        assert!(
            applied.bytes == expected_bytes,
            "raw PATH bytes differ from preview"
        );

        let mut child = crate::process::ps("[Console]::Write($env:PATH)");
        platform::apply_fresh_user_path(&mut child).expect("build fresh child PATH");
        let output = crate::process::capture(child, std::time::Duration::from_secs(20))
            .expect("probe newly spawned shell");
        assert_eq!(output.code, 0);
        assert!(
            output.stdout == platform::effective_path().unwrap().trim(),
            "new shell PATH did not match refreshed environment"
        );

        restore_change(directory.path(), &change.id).expect("restore first PATH change");
        assert_eq!(read_user_path().unwrap(), before);
        assert!(
            exact_value().unwrap() == restore_original.0,
            "restoration changed original registry bytes or type"
        );

        let conflict = preview_append_native().expect("preview second PATH change");
        apply_change(directory.path(), &conflict).expect("apply second PATH plan");
        let mut external = conflict.after.clone();
        external
            .value
            .push_str(&format!(";C:\\CCM-Smoke-External-{}", uuid::Uuid::new_v4()));
        write_user_path(&external).expect("simulate an external edit on isolated runner");
        assert!(preview_restore(directory.path(), &conflict.id).is_err());
        assert!(restore_change(directory.path(), &conflict.id).is_err());
        assert_eq!(
            read_user_path().unwrap(),
            external,
            "conflict handling overwrote external edit"
        );

        restore_original
            .restore()
            .expect("restore exact original runner PATH");
        assert!(
            exact_value().unwrap() == restore_original.0,
            "final cleanup did not restore original raw PATH"
        );
    }
}
