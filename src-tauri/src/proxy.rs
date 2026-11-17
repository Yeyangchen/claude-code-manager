use crate::storage;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettings {
    pub mode: String,
    pub url: String,
    pub has_credentials: bool,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            mode: "system".into(),
            url: String::new(),
            has_credentials: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ProxyCredentials {
    username: String,
    password: String,
}

pub fn validate_proxy_url(value: &str) -> Result<Url, String> {
    if value.len() > 4096 {
        return Err("代理地址过长".into());
    }
    let url = Url::parse(value).map_err(|_| "代理地址格式无效")?;
    if !["http", "https", "socks5", "socks5h"].contains(&url.scheme()) || url.host_str().is_none() {
        return Err("代理需要 http、https、socks5 或 socks5h 地址".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("请把代理用户名和密码填入独立凭据字段".into());
    }
    if !["", "/"].contains(&url.path()) || url.query().is_some() || url.fragment().is_some() {
        return Err("代理地址只能包含协议、主机和端口".into());
    }
    Ok(url)
}

fn credential_target(url: &str) -> String {
    format!(
        "Yeyangchen.ClaudeCodeManager.Proxy.{}",
        storage::fingerprint(url.as_bytes())
    )
}

pub fn load_proxy(data_dir: &Path) -> Result<ProxySettings, String> {
    let mut settings = storage::read_json::<ProxySettings>(&data_dir.join("proxy.json"))?.value;
    settings.has_credentials =
        !settings.url.is_empty() && read_credentials(&credential_target(&settings.url))?.is_some();
    Ok(settings)
}

pub fn save_proxy(
    data_dir: &Path,
    mut settings: ProxySettings,
    username: Option<String>,
    password: Option<String>,
) -> Result<ProxySettings, String> {
    if !["system", "direct", "custom"].contains(&settings.mode.as_str()) {
        return Err("代理模式无效".into());
    }
    if settings.mode == "custom" || !settings.url.is_empty() {
        settings.url = validate_proxy_url(settings.url.trim())?.to_string();
    }
    let state_path = data_dir.join("proxy.json");
    if storage::read_json::<ProxySettings>(&state_path)?.read_only {
        return Err("代理设置来自更新版本，当前只读".into());
    }
    if username.is_some() != password.is_some() {
        return Err("更新代理凭据需要同时提供用户名和密码".into());
    }
    let target = credential_target(&settings.url);
    let previous = if settings.url.is_empty() {
        None
    } else {
        read_credentials(&target)?
    };
    let changed_credentials = username.is_some();
    if let (Some(username), Some(password)) = (username, password) {
        if settings.url.is_empty() {
            return Err("保存代理凭据前需要代理地址".into());
        }
        if username.is_empty() && password.is_empty() {
            delete_credentials(&target)?;
        } else {
            write_credentials(&target, &ProxyCredentials { username, password })?;
        }
    }
    settings.has_credentials = !settings.url.is_empty() && read_credentials(&target)?.is_some();
    if let Err(error) = storage::write_json(&state_path, &settings) {
        if changed_credentials {
            match previous {
                Some(credentials) => {
                    let _ = write_credentials(&target, &credentials);
                }
                None => {
                    let _ = delete_credentials(&target);
                }
            }
        }
        return Err(error);
    }
    Ok(settings)
}

fn url_with_credentials(settings: &ProxySettings) -> Result<Url, String> {
    let mut url = validate_proxy_url(&settings.url)?;
    if let Some(credentials) = read_credentials(&credential_target(&settings.url))? {
        url.set_username(&credentials.username)
            .map_err(|_| "无法设置代理用户名")?;
        url.set_password(Some(&credentials.password))
            .map_err(|_| "无法设置代理密码")?;
    }
    Ok(url)
}

/// Proxy for an HTTPS-only updater. The returned URL may contain credentials;
/// keep it in memory and never serialize, log, or include it in an error.
/// `direct` callers must also disable the updater's automatic proxy detection.
pub fn effective_proxy_url(data_dir: &Path) -> Result<Option<Url>, String> {
    let settings = load_proxy(data_dir)?;
    match settings.mode.as_str() {
        "direct" => Ok(None),
        "custom" => url_with_credentials(&settings).map(Some),
        "system" => {
            for name in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
                if let Ok(value) = std::env::var(name) {
                    if !value.is_empty() {
                        let url = Url::parse(&value).map_err(|_| "系统代理环境变量格式无效")?;
                        if !["http", "https", "socks5", "socks5h"].contains(&url.scheme())
                            || url.host_str().is_none()
                        {
                            return Err("系统代理环境变量格式无效".into());
                        }
                        return Ok(Some(url));
                    }
                }
            }
            // Match reqwest_client: an explicit environment proxy configuration
            // takes precedence over Windows Internet Settings, including an
            // HTTP-only configuration which does not cover this HTTPS updater.
            if has_proxy_environment() {
                return Ok(None);
            }
            system_proxy()?
                .and_then(|(_, https, _)| https)
                .map(|value| Url::parse(&value).map_err(|_| "系统 HTTPS 代理无效".into()))
                .transpose()
        }
        _ => Err("未知代理模式".into()),
    }
}

pub fn reqwest_client(data_dir: &Path) -> Result<reqwest::Client, String> {
    let settings = load_proxy(data_dir)?;
    let mut builder = reqwest::Client::builder()
        .user_agent("ClaudeCodeManager/0.1")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(5));
    match settings.mode.as_str() {
        "direct" => {
            builder = builder.no_proxy();
        }
        "custom" => {
            let proxy = reqwest::Proxy::all(url_with_credentials(&settings)?.as_str())
                .map_err(|_| "代理配置无法用于网络请求")?;
            builder = builder.no_proxy().proxy(proxy);
        }
        "system" => {
            if !has_proxy_environment() {
                if let Some((http, https, bypass)) = system_proxy()? {
                    if let Some(http) = http {
                        let proxy = reqwest::Proxy::http(http)
                            .map_err(|_| "系统 HTTP 代理无效")?
                            .no_proxy(reqwest::NoProxy::from_string(&bypass));
                        builder = builder.proxy(proxy);
                    }
                    if let Some(https) = https {
                        let proxy = reqwest::Proxy::https(https)
                            .map_err(|_| "系统 HTTPS 代理无效")?
                            .no_proxy(reqwest::NoProxy::from_string(&bypass));
                        builder = builder.proxy(proxy);
                    }
                }
            }
        }
        _ => return Err("未知代理模式".into()),
    }
    builder
        .build()
        .map_err(|_| "无法创建网络客户端，请检查代理和系统证书")
}

fn has_proxy_environment() -> bool {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

pub fn apply_command_proxy(data_dir: &Path, command: &mut Command) -> Result<(), String> {
    let settings = load_proxy(data_dir)?;
    match settings.mode.as_str() {
        "direct" => {
            for variable in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
                "npm_config_proxy",
                "npm_config_https_proxy",
            ] {
                command.env_remove(variable);
            }
            command.env("NO_PROXY", "*").env("no_proxy", "*");
        }
        "custom" => {
            let url = url_with_credentials(&settings)?.to_string();
            for variable in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
                "npm_config_proxy",
                "npm_config_https_proxy",
            ] {
                command.env(variable, &url);
            }
            command.env_remove("NO_PROXY").env_remove("no_proxy");
        }
        "system" => {
            if !has_proxy_environment() {
                if let Some((http, https, bypass)) = system_proxy()? {
                    if let Some(http) = http {
                        command.env("HTTP_PROXY", http);
                    }
                    if let Some(https) = https {
                        command.env("HTTPS_PROXY", https);
                    }
                    if !bypass.is_empty() {
                        command.env("NO_PROXY", bypass);
                    }
                }
            }
        }
        _ => return Err("未知代理模式".into()),
    }
    Ok(())
}

type SystemProxy = Option<(Option<String>, Option<String>, String)>;

#[cfg(windows)]
fn system_proxy() -> Result<SystemProxy, String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let key = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    {
        Ok(key) => key,
        Err(_) => return Ok(None),
    };
    if key.get_value::<u32, _>("ProxyEnable").unwrap_or(0) == 0 {
        return Ok(None);
    }
    let server = key
        .get_value::<String, _>("ProxyServer")
        .map_err(|_| "系统代理已开启但地址不可读")?;
    let bypass = key
        .get_value::<String, _>("ProxyOverride")
        .unwrap_or_default()
        .split(';')
        .filter(|item| !item.is_empty() && *item != "<local>")
        .collect::<Vec<_>>()
        .join(",");
    let mut http = None;
    let mut https = None;
    if server.contains('=') {
        for assignment in server.split(';') {
            if let Some((scheme, address)) = assignment.split_once('=') {
                match scheme.trim() {
                    "http" => http = Some(normalize_system_proxy(address)?),
                    "https" => https = Some(normalize_system_proxy(address)?),
                    _ => {}
                }
            }
        }
    } else {
        let url = normalize_system_proxy(&server)?;
        http = Some(url.clone());
        https = Some(url);
    }
    Ok(Some((http, https, bypass)))
}

#[cfg(not(windows))]
fn system_proxy() -> Result<SystemProxy, String> {
    Ok(None)
}

#[cfg(windows)]
fn normalize_system_proxy(value: &str) -> Result<String, String> {
    let url = if value.contains("://") {
        value.trim().to_string()
    } else {
        format!("http://{}", value.trim())
    };
    Ok(validate_proxy_url(&url)?.to_string())
}

#[cfg(windows)]
fn read_credentials(target: &str) -> Result<Option<ProxyCredentials>, String> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };
    let target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
            return Ok(None);
        }
        return Err("无法读取系统凭据管理器中的代理凭据".into());
    }
    let result = unsafe {
        let credential_ref = &*credential;
        if credential_ref.CredentialBlobSize == 0 || credential_ref.CredentialBlob.is_null() {
            Err("代理凭据记录为空".to_string())
        } else {
            let bytes = std::slice::from_raw_parts(
                credential_ref.CredentialBlob,
                credential_ref.CredentialBlobSize as usize,
            );
            serde_json::from_slice(bytes)
                .map(Some)
                .map_err(|_| "代理凭据格式无效".into())
        }
    };
    unsafe {
        CredFree(credential.cast());
    }
    result
}

#[cfg(windows)]
fn write_credentials(target: &str, credentials: &ProxyCredentials) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };
    let mut target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let mut username: Vec<u16> = "ClaudeCodeManagerProxy"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut bytes = serde_json::to_vec(credentials).map_err(|_| "代理凭据序列化失败")?;
    if bytes.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
        return Err("代理凭据超过系统允许的长度".into());
    }
    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        Comment: std::ptr::null_mut(),
        LastWritten: windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: bytes.len() as u32,
        CredentialBlob: bytes.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: username.as_mut_ptr(),
    };
    let result = unsafe { CredWriteW(&credential, 0) };
    bytes.fill(0);
    if result == 0 {
        return Err("无法保存代理凭据到系统凭据管理器".into());
    }
    Ok(())
}

#[cfg(windows)]
fn delete_credentials(target: &str) -> Result<(), String> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};
    let target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        if std::io::Error::last_os_error().raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
            return Err("删除代理凭据失败".into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn read_credentials(_target: &str) -> Result<Option<ProxyCredentials>, String> {
    Ok(None)
}

#[cfg(not(windows))]
fn write_credentials(_target: &str, _credentials: &ProxyCredentials) -> Result<(), String> {
    Err("此版本仅支持 Windows 系统凭据管理器".into())
}

#[cfg(not(windows))]
fn delete_credentials(_target: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_rejects_userinfo_paths_and_unsupported_schemes() {
        for invalid in [
            "https://user:password@proxy.test",
            "file:///tmp/x",
            "http://proxy.test/path",
            "http://proxy.test?token=secret",
        ] {
            assert!(validate_proxy_url(invalid).is_err(), "{invalid}");
        }
        assert!(validate_proxy_url("socks5h://127.0.0.1:7890").is_ok());
    }
    #[test]
    fn settings_serialization_has_no_credentials() {
        let settings = ProxySettings {
            mode: "custom".into(),
            url: "http://proxy.test:8080/".into(),
            has_credentials: true,
        };
        let encoded = serde_json::to_string(&settings).unwrap();
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("username"));
    }
}
