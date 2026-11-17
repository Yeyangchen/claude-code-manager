use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn powershell() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32/WindowsPowerShell/v1.0/powershell.exe")
}

pub fn hidden(command: &mut Command) {
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
}

fn drain(mut reader: impl Read, maximum: u64) -> String {
    let mut data = Vec::new();
    let mut buffer = [0u8; 8192];
    // Continue draining after the display limit so verbose probes cannot block
    // forever on a full OS pipe.
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        let remaining = (maximum as usize).saturating_sub(data.len());
        data.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    String::from_utf8_lossy(&data)
        .trim_start_matches('\u{feff}')
        .trim()
        .to_string()
}

/// Read-only probes are bounded and may be terminated on timeout.
pub fn capture(mut command: Command, timeout: Duration) -> Result<Output, String> {
    hidden(&mut command);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法启动检测程序: {e}"))?;
    let stdout = child.stdout.take().ok_or("检测程序没有输出管道")?;
    let stderr = child.stderr.take().ok_or("检测程序没有错误管道")?;
    let out = thread::spawn(move || drain(stdout, 2 * 1024 * 1024));
    let err = thread::spawn(move || drain(stderr, 512 * 1024));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("检测超时，请检查程序是否被其他操作占用".into());
        }
        thread::sleep(Duration::from_millis(60));
    };
    Ok(Output {
        code: status.code().unwrap_or(-1),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

pub fn ps(script: &str) -> Command {
    let mut command = Command::new(powershell());
    command.args(["-NoProfile", "-NonInteractive", "-Command"]);
    command.arg(format!("[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $ErrorActionPreference='Stop'; {script}"));
    command
}

pub fn ps_capture(script: &str) -> Result<Output, String> {
    capture(ps(script), Duration::from_secs(20))
}

pub fn executable_command(path: &Path, args: &[String]) -> Command {
    // npm's .cmd shim is invoked with PowerShell's call operator. Data is passed
    // as environment JSON, never concatenated into the shell program.
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("ps1"))
    {
        let mut command = ps("$arguments = @(ConvertFrom-Json $env:CCM_ARGUMENTS); & $env:CCM_EXECUTABLE @arguments; exit $LASTEXITCODE");
        command.env("CCM_EXECUTABLE", path);
        command.env(
            "CCM_ARGUMENTS",
            serde_json::to_string(args).unwrap_or_default(),
        );
        return command;
    }
    let mut command = Command::new(path);
    command.args(args);
    command
}

pub fn resolve(name: &str) -> Option<PathBuf> {
    let mut command = ps("$c=Get-Command -Name $env:CCM_COMMAND_NAME -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1; if ($c) { $c.Source }");
    command.env("CCM_COMMAND_NAME", name);
    capture(command, Duration::from_secs(10))
        .ok()
        .filter(|o| o.code == 0)
        .and_then(|o| o.stdout.lines().next().map(PathBuf::from))
        .filter(|p| p.is_file())
}

pub fn version(path: &Path) -> Option<String> {
    capture(
        executable_command(path, &["--version".into()]),
        Duration::from_secs(12),
    )
    .ok()
    .filter(|o| o.code == 0)
    .and_then(|o| parse_version(&o.stdout))
}

pub fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|part| {
        let part = part.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        let part = part.strip_prefix('v').unwrap_or(part);
        semver::Version::parse(part).ok().map(|v| v.to_string())
    })
}

pub fn fingerprint(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let metadata = std::fs::metadata(path).ok();
    let material = format!(
        "{}|{:?}|{:?}",
        canonical.display(),
        metadata.as_ref().map(|m| m.len()),
        metadata.and_then(|m| m.modified().ok())
    );
    hex::encode(Sha256::digest(material.as_bytes()))
}

pub fn equivalent(left: &str, right: &str) -> bool {
    fn normalize(path: &str) -> String {
        let resolved = Path::new(path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(path));
        resolved
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_lowercase()
    }
    normalize(left) == normalize(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_parser_accepts_cli_and_npm_formats() {
        assert_eq!(
            parse_version("2.1.326 (Claude Code)"),
            Some("2.1.326".into())
        );
        assert_eq!(parse_version("v24.14.0"), Some("24.14.0".into()));
        assert_eq!(parse_version("Access denied"), None);
    }
    #[test]
    fn semver_is_not_a_command() {
        assert_eq!(parse_version("2.1.326;Remove-Item"), None);
    }
}
