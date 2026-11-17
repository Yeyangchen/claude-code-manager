use std::path::PathBuf;
use std::process::Command;

pub fn user_home() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let value = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let value = std::env::var_os("HOME");
    let path = PathBuf::from(value.ok_or("无法确定当前用户目录")?);
    if !path.is_absolute() {
        return Err("用户目录必须为绝对路径".into());
    }
    Ok(path)
}

pub fn claude_config_dir() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err("CLAUDE_CONFIG_DIR 必须为绝对路径".into());
        }
        return Ok(path);
    }
    Ok(user_home()?.join(".claude"))
}

pub fn native_architecture() -> String {
    let architecture = std::env::var("PROCESSOR_ARCHITEW6432")
        .or_else(|_| std::env::var("PROCESSOR_ARCHITECTURE"))
        .unwrap_or_else(|_| std::env::consts::ARCH.into());
    match architecture.to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" | "x64" => "x64".into(),
        "aarch64" | "arm64" => "arm64".into(),
        other => other.into(),
    }
}

#[cfg(windows)]
pub fn is_elevated() -> Result<bool, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(format!(
            "无法检查进程权限：{}",
            std::io::Error::last_os_error()
        ));
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut length = 0;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
    };
    unsafe {
        CloseHandle(token);
    }
    if result == 0 {
        return Err(format!(
            "无法读取进程权限：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(elevation.TokenIsElevated != 0)
}

#[cfg(not(windows))]
pub fn is_elevated() -> Result<bool, String> {
    Ok(false)
}

#[cfg(windows)]
pub fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
pub fn hide_console(_command: &mut Command) {}

#[cfg(windows)]
pub fn expand_environment(value: &str) -> Result<String, String> {
    use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
    let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let required = unsafe { ExpandEnvironmentStringsW(value.as_ptr(), std::ptr::null_mut(), 0) };
    if required == 0 || required > 32768 {
        return Err("环境变量展开失败或结果过长".into());
    }
    let mut result = vec![0u16; required as usize];
    let written =
        unsafe { ExpandEnvironmentStringsW(value.as_ptr(), result.as_mut_ptr(), required) };
    if written == 0 || written > required {
        return Err("环境变量展开失败，请重新检测".into());
    }
    String::from_utf16(&result[..written.saturating_sub(1) as usize])
        .map_err(|_| "环境变量含无效 Unicode".into())
}

#[cfg(not(windows))]
pub fn expand_environment(value: &str) -> Result<String, String> {
    Ok(value.into())
}

#[cfg(windows)]
pub fn effective_path() -> Result<String, String> {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .map_err(|e| format!("无法读取系统 PATH：{e}"))?;
    let system_raw = machine
        .get_raw_value("Path")
        .map_err(|e| format!("无法读取系统 PATH：{e}"))?;
    let system = crate::path_edit::decode_registry_string(&system_raw.bytes)?;
    let user = crate::path_edit::read_user_path()?;
    let combined = if user.exists && !user.value.is_empty() {
        format!("{system};{}", user.value)
    } else {
        system
    };
    expand_environment(&combined)
}

#[cfg(not(windows))]
pub fn effective_path() -> Result<String, String> {
    Ok(std::env::var("PATH").unwrap_or_default())
}

pub fn apply_fresh_user_path(command: &mut Command) -> Result<(), String> {
    command.env("PATH", effective_path()?);
    Ok(())
}

#[cfg(windows)]
pub fn broadcast_environment_changed() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let value: Vec<u16> = "Environment".encode_utf16().chain(Some(0)).collect();
    let mut result = 0;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            value.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            &mut result,
        );
    }
}

#[cfg(not(windows))]
pub fn broadcast_environment_changed() {}
