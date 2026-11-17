use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LoadResult<T> {
    pub value: T,
    pub read_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<T> {
    schema_version: u32,
    data: T,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.len() > MAX_JSON_BYTES {
                return Err("配置文件超过允许的大小，请先检查文件内容".into());
            }
            fs::read(path)
                .map(Some)
                .map_err(|e| format!("读取配置失败：{e}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取配置失败：{error}")),
    }
}

pub fn file_fingerprint(path: &Path) -> Result<String, String> {
    Ok(match read_bytes(path)? {
        Some(bytes) => fingerprint(&bytes),
        None => "missing".to_string(),
    })
}

/// A newer schema is readable only when its data still matches the known shape.
/// Callers must carry `read_only` into all write operations.
pub fn read_json<T: DeserializeOwned + Serialize + Default>(
    path: &Path,
) -> Result<LoadResult<T>, String> {
    let Some(bytes) = read_bytes(path)? else {
        return Ok(LoadResult {
            value: T::default(),
            read_only: false,
        });
    };
    let root: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("配置 JSON 无法解析，原文件已保留：{e}"))?;
    let schema = root
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if schema > SCHEMA_VERSION as u64 {
        let data = root.get("data").cloned().unwrap_or_default();
        return Ok(LoadResult {
            value: serde_json::from_value(data).unwrap_or_default(),
            read_only: true,
        });
    }
    if schema == 0 {
        // Version-zero files were plain JSON. Preserve the exact source before migration.
        let value: T = serde_json::from_value(root)
            .map_err(|e| format!("旧配置格式无法迁移，原文件已保留：{e}"))?;
        let backup = path.with_extension(format!("schema-0-{}.backup", now_ms()));
        atomic_write(&backup, &bytes)?;
        write_json(path, &value)?;
        return Ok(LoadResult {
            value,
            read_only: false,
        });
    }
    let envelope: Envelope<T> =
        serde_json::from_slice(&bytes).map_err(|e| format!("配置数据无效，原文件已保留：{e}"))?;
    Ok(LoadResult {
        value: envelope.data,
        read_only: false,
    })
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(bytes) = read_bytes(path)? {
        let root: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("现有配置无法解析，已停止覆盖：{e}"))?;
        if root
            .get("schemaVersion")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > SCHEMA_VERSION as u64
        {
            return Err("数据来自更新版本的管理器，当前处于只读模式".into());
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&Envelope {
        schema_version: SCHEMA_VERSION,
        data: value,
    })
    .map_err(|e| format!("序列化配置失败：{e}"))?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("文件缺少父目录")?;
    fs::create_dir_all(parent).map_err(|e| format!("创建数据目录失败：{e}"))?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("创建临时文件失败：{e}"))?;
        file.write_all(bytes)
            .map_err(|e| format!("写入文件失败：{e}"))?;
        file.sync_all().map_err(|e| format!("同步文件失败：{e}"))?;
        drop(file);
        replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both paths refer to files in the same managed directory.
    let success = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if success == 0 {
        return Err(format!(
            "原子替换文件失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|e| format!("原子替换文件失败：{e}"))
}

pub fn safe_record_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "记录 ID 无效")?;
    Ok(directory.join(format!("{id}.json")))
}

pub fn redact(text: &str) -> String {
    let mut redacted = text.to_owned();
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(key) {
            if home.len() > 3 {
                redacted = redacted.replace(&home, "<用户目录>");
                redacted = redacted.replace(&home.replace('\\', "/"), "<用户目录>");
                redacted = redacted.replace(&home.replace('\\', "\\\\"), "<用户目录>");
            }
        }
    }
    if let Ok(username) = std::env::var("USERNAME") {
        if username.len() > 2 {
            redacted = redacted.replace(&username, "<用户名>");
        }
    }
    redacted
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if [
                "authorization",
                "api_key",
                "api-key",
                "access_token",
                "auth_token",
                "oauth",
                "password",
                "secret=",
                "secret:",
                "\"secret\"",
                "proxy-authorization",
            ]
            .iter()
            .any(|term| lower.contains(term))
            {
                return "<敏感输出已隐藏>".to_string();
            }
            let mut result = line.to_string();
            while let Some(start) = result.find("sk-") {
                let end = result[start..]
                    .find(char::is_whitespace)
                    .map(|i| start + i)
                    .unwrap_or(result.len());
                result.replace_range(start..end, "<密钥已隐藏>");
            }
            // Remove URL userinfo even when the installer prints an entire proxy URL.
            for prefix in ["https://", "http://", "socks5://", "socks5h://"] {
                let mut offset = 0;
                while let Some(relative) = result[offset..].find(prefix) {
                    let start = offset + relative + prefix.len();
                    let authority_end = result[start..]
                        .find(['/', ' ', '\t', '\r', '\n'])
                        .map(|i| start + i)
                        .unwrap_or(result.len());
                    if let Some(relative_at) = result[start..authority_end].rfind('@') {
                        result.replace_range(start..start + relative_at, "<凭据>");
                        offset = start + "<凭据>".len() + 1;
                    } else {
                        offset = authority_end;
                    }
                    if offset >= result.len() {
                        break;
                    }
                }
            }
            result
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_schema_cannot_be_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, br#"{"schemaVersion":99,"data":{"theme":"future"}}"#).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(read_json::<serde_json::Value>(&path).unwrap().read_only);
        assert!(write_json(&path, &serde_json::json!({})).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn migration_keeps_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, br#"{"theme":"dark"}"#).unwrap();
        let state = read_json::<serde_json::Value>(&path).unwrap();
        assert_eq!(state.value["theme"], "dark");
        assert!(fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "backup")));
    }

    #[test]
    fn malformed_existing_data_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let original = b"{broken-json";
        fs::write(&path, original).unwrap();
        assert!(write_json(&path, &serde_json::json!({"theme":"light"})).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn atomic_replacement_and_record_path_are_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("record.json");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(safe_record_path(directory.path(), "../../outside").is_err());
        let valid = safe_record_path(directory.path(), &uuid::Uuid::new_v4().to_string()).unwrap();
        assert_eq!(valid.parent(), Some(directory.path()));
    }

    #[test]
    fn redaction_handles_credentials_and_tokens() {
        let clean = redact("Downloading via https://alice:p%40ss@proxy.example:8080/file\nAuthorization: Bearer secret\nkey sk-123456 sk-second finished");
        assert!(!clean.contains("alice"));
        assert!(!clean.contains("secret"));
        assert!(!clean.contains("123456"));
        assert!(!clean.contains("sk-second"));
        assert!(!redact(r#"{"password": "unsafe-value"}"#).contains("unsafe-value"));
        if let Ok(home) = std::env::var("USERPROFILE") {
            let serialized = serde_json::to_string(&format!("{home}\\private.json")).unwrap();
            assert!(redact(&serialized).contains("<用户目录>"));
        }
    }
}
