use crate::domain::*;
use crate::{platform, process};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone)]
pub struct Discovery {
    pub environment: Environment,
    pub installations: Vec<Installation>,
    pub shells: Vec<ShellResolution>,
    pub checks: Vec<Check>,
}

pub fn scan() -> Result<Discovery, String> {
    let home = platform::user_home()?;
    let config = platform::claude_config_dir()?;
    let mut checks = Vec::new();
    let system = process::ps_capture("$v=Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion'; @{build=$v.CurrentBuildNumber;name=$v.ProductName}|ConvertTo-Json -Compress")
        .ok().and_then(|o| serde_json::from_str::<Value>(&o.stdout).ok());
    let build = system
        .as_ref()
        .and_then(|v| v["build"].as_str())
        .and_then(|v| v.parse::<u32>().ok());
    let supported = cfg!(windows) && build.is_some_and(|v| v >= 17763);
    let os = system
        .as_ref()
        .and_then(|v| v["name"].as_str())
        .unwrap_or(std::env::consts::OS)
        .to_string();
    checks.push(check(
        "os",
        "系统版本",
        if supported { "ok" } else { "error" },
        &format!(
            "{os} · build {}，要求 Windows 10 1809 或更新版本",
            build
                .map(|v| v.to_string())
                .unwrap_or_else(|| "未知".into())
        ),
    ));
    let elevated = platform::is_elevated()?;
    checks.push(check(
        "permissions",
        "运行权限",
        if elevated { "warning" } else { "ok" },
        if elevated {
            "当前进程已提升权限，检测对应当前管理员用户。建议以标准用户启动。"
        } else {
            "标准用户，用户范围安装可直接管理。"
        },
    ));
    let git = process::resolve("git.exe");
    checks.push(check(
        "git",
        "Git for Windows",
        if git.is_some() { "ok" } else { "info" },
        if git.is_some() {
            "已发现 Git。"
        } else {
            "可选依赖。缺少 Git Bash 时 Claude Code 使用 PowerShell。"
        },
    ));

    let shells = resolve_shells();
    let active = shells
        .iter()
        .find(|s| s.name == "PowerShell")
        .and_then(|s| s.path.as_ref());
    let mut installations = Vec::new();
    let native = home.join(".local/bin/claude.exe");
    if native.is_file() {
        let path = native.to_string_lossy().to_string();
        installations.push(installation(
            Source::Native,
            &native,
            process::version(&native),
            "user",
            active,
            "官方原生安装目录",
            None,
            None,
        ));
        if active.is_some_and(|p| !process::equivalent(p, &path)) {
            checks.push(check(
                "path-conflict",
                "终端命中",
                "warning",
                "已发现原生安装，但 PowerShell 优先使用其他路径。可在环境页查看或修复。",
            ));
        }
    }

    let npm = process::resolve("npm.cmd");
    if let Some(npm_path) = npm.as_ref() {
        let node = process::resolve("node.exe").and_then(|p| process::version(&p));
        let node_ok = node
            .as_deref()
            .and_then(|v| semver::Version::parse(v).ok())
            .is_some_and(|v| v.major >= 22);
        checks.push(check(
            "node",
            "npm 运行环境",
            if node_ok { "ok" } else { "warning" },
            &format!(
                "Node.js {}，npm 安装要求 Node.js 22 或更新版本。",
                node.unwrap_or_else(|| "未知".into())
            ),
        ));
        let mut prefixes = HashSet::new();
        if let Ok(output) = process::capture(
            process::executable_command(npm_path, &["prefix".into(), "-g".into()]),
            Duration::from_secs(15),
        ) {
            if output.code == 0 && Path::new(output.stdout.trim()).is_absolute() {
                prefixes.insert(PathBuf::from(output.stdout.trim()));
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            prefixes.insert(PathBuf::from(appdata).join("npm"));
        }
        if let Some(paths) = std::env::var_os("PATH") {
            for path in std::env::split_paths(&paths) {
                if path
                    .join("node_modules/@anthropic-ai/claude-code/package.json")
                    .is_file()
                {
                    prefixes.insert(path);
                }
            }
        }
        for prefix in prefixes {
            let package = prefix.join("node_modules/@anthropic-ai/claude-code/package.json");
            let Some(package_json) = std::fs::read(&package)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            if package_json["name"] != "@anthropic-ai/claude-code" {
                continue;
            }
            let executable = ["claude.cmd", "claude.exe", "claude.ps1"]
                .iter()
                .map(|f| prefix.join(f))
                .find(|p| p.is_file());
            let Some(executable) = executable else {
                continue;
            };
            let scope = if prefix.starts_with(&home) {
                "user"
            } else {
                "machine"
            };
            let version = package_json["version"].as_str().map(String::from);
            installations.push(installation(
                Source::Npm,
                &executable,
                version,
                scope,
                active,
                "npm 全局包，操作绑定当前 npm 与该 prefix",
                Some(npm_path.to_string_lossy().into()),
                Some(prefix.to_string_lossy().into()),
            ));
        }
    }

    let winget = process::resolve("winget.exe");
    checks.push(check(
        "winget",
        "WinGet",
        if winget.is_some() { "ok" } else { "info" },
        if winget.is_some() {
            "WinGet 可用。"
        } else {
            "未发现 WinGet，相关来源操作暂不可用。"
        },
    ));
    let roots = [
        (
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|p| p.join("Microsoft/WinGet/Packages")),
            "user",
        ),
        (
            std::env::var_os("ProgramFiles")
                .map(PathBuf::from)
                .map(|p| p.join("WinGet/Packages")),
            "machine",
        ),
    ];
    for (root, scope) in roots {
        let Some(root) = root else { continue };
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with("Anthropic.ClaudeCode_")
            {
                continue;
            }
            let executable = entry.path().join("claude.exe");
            if executable.is_file() {
                let mut install = installation(
                    Source::Winget,
                    &executable,
                    process::version(&executable),
                    scope,
                    active,
                    "WinGet portable 包",
                    None,
                    None,
                );
                if winget.is_none() {
                    install.capabilities.upgrade = false;
                    install.capabilities.uninstall = false;
                }
                installations.push(install);
            }
        }
    }

    for shell in &shells {
        for path in &shell.candidates {
            if installations
                .iter()
                .any(|i| process::equivalent(&i.path, path))
            {
                continue;
            }
            // Recognized npm and native launch aliases may resolve differently.
            let is_npm_alias = installations.iter().any(|i| {
                i.source == Source::Npm && Path::new(&i.path).parent() == Path::new(path).parent()
            });
            if is_npm_alias {
                continue;
            }
            if !Path::new(path).is_file() {
                continue;
            }
            installations.push(installation(
                Source::External,
                Path::new(path),
                None,
                "unknown",
                active,
                "其他程序、IDE 私有 CLI 或应用执行别名，仅显示归属",
                None,
                None,
            ));
        }
    }
    if let Ok(output) = process::ps_capture("Get-ExecutionPolicy") {
        checks.push(check(
            "execution-policy",
            "PowerShell 执行策略",
            if output.stdout.contains("Restricted") {
                "warning"
            } else {
                "info"
            },
            &format!("{}。此设置可能影响 npm 的 .ps1 启动脚本。", output.stdout),
        ));
    }
    Ok(Discovery {
        environment: Environment {
            os,
            arch: platform::native_architecture(),
            user_name: std::env::var("USERNAME").unwrap_or_else(|_| "unknown".into()),
            elevated,
            config_dir: config.to_string_lossy().into(),
            supported,
        },
        installations,
        shells,
        checks,
    })
}

fn check(id: &str, title: &str, level: &str, message: &str) -> Check {
    Check {
        id: id.into(),
        title: title.into(),
        level: level.into(),
        message: message.into(),
    }
}

fn installation(
    source: Source,
    path: &Path,
    version: Option<String>,
    scope: &str,
    active: Option<&String>,
    detail: &str,
    npm_path: Option<String>,
    prefix: Option<String>,
) -> Installation {
    let path_text = path.to_string_lossy().to_string();
    let manageable = source != Source::External && scope == "user";
    let native = source == Source::Native;
    let id = hex::encode(Sha256::digest(
        format!("{source:?}|{}", path_text.to_lowercase()).as_bytes(),
    ))[..20]
        .to_string();
    let manual_command = if scope == "machine" {
        Some(match source {
            Source::Npm => format!(
                "npm install -g @anthropic-ai/claude-code@latest --prefix \"{}\"",
                prefix.as_deref().unwrap_or_default()
            ),
            Source::Winget => "winget upgrade --id Anthropic.ClaudeCode --exact".into(),
            _ => "请在原安装来源中管理此实例".into(),
        })
    } else {
        None
    };
    Installation {
        id,
        source,
        path: path_text.clone(),
        version,
        scope: scope.into(),
        active: active.is_some_and(|p| process::equivalent(p, &path_text)),
        capabilities: Capabilities {
            upgrade: manageable,
            install_version: manageable && native,
            uninstall: manageable,
        },
        detail: detail.into(),
        manual_command,
        npm_path,
        prefix,
        fingerprint: process::fingerprint(path),
    }
}

fn resolve_shells() -> Vec<ShellResolution> {
    let mut shells = Vec::new();
    let script = "@(Get-Command claude -All -ErrorAction SilentlyContinue | ForEach-Object { $_.Source }) | ConvertTo-Json -Compress";
    let mut command = process::ps(script);
    let _ = platform::apply_fresh_user_path(&mut command);
    let candidates = process::capture(command, Duration::from_secs(15))
        .ok()
        .map(|o| parse_json_strings(&o.stdout))
        .unwrap_or_default();
    shells.push(ShellResolution {
        name: "PowerShell".into(),
        path: candidates.first().cloned(),
        candidates,
        detail: "新进程，无个人 profile。个人别名和函数可在终端运行 Get-Command claude -All 核查。"
            .into(),
    });
    let mut where_command = std::process::Command::new(
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32/where.exe"),
    );
    where_command.arg("claude");
    let _ = platform::apply_fresh_user_path(&mut where_command);
    let candidates = process::capture(where_command, Duration::from_secs(10))
        .ok()
        .filter(|o| o.code == 0)
        .map(|o| {
            o.stdout
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    shells.push(ShellResolution {
        name: "CMD".into(),
        path: candidates.first().cloned(),
        candidates,
        detail: "按新的用户和系统 PATH 解析，已打开终端需重新启动。".into(),
    });
    if let Some(git) = process::resolve("git.exe") {
        let bash = git
            .parent()
            .and_then(Path::parent)
            .map(|root| root.join("bin/bash.exe"));
        if let Some(bash) = bash.filter(|p| p.is_file()) {
            let mut command = std::process::Command::new(bash);
            command.args([
                "--noprofile",
                "--norc",
                "-c",
                "command -v claude | xargs -r cygpath -w",
            ]);
            let _ = platform::apply_fresh_user_path(&mut command);
            let candidates = process::capture(command, Duration::from_secs(10))
                .ok()
                .filter(|o| o.code == 0)
                .map(|o| {
                    o.stdout
                        .lines()
                        .filter(|v| !v.is_empty())
                        .map(String::from)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            shells.push(ShellResolution {
                name: "Git Bash".into(),
                path: candidates.first().cloned(),
                candidates,
                detail: "使用新的 PATH，未加载个人启动脚本。".into(),
            });
        }
    }
    shells
}

fn parse_json_strings(text: &str) -> Vec<String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::String(v)) => vec![v],
        Ok(Value::Array(v)) => v
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .filter(|v| !v.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn running_for(installation: Option<&Installation>) -> Result<Vec<u32>, String> {
    let mut command = process::ps("@(Get-CimInstance Win32_Process -Filter \"Name='claude.exe'\" | Select-Object ProcessId,ExecutablePath) | ConvertTo-Json -Compress");
    command.env("CCM_READ_ONLY", "1");
    let output = process::capture(command, Duration::from_secs(15))?;
    if output.code != 0 {
        return Err("无法核实运行中的 Claude 进程，请稍后重试".into());
    }
    let value: Value = serde_json::from_str(&output.stdout).unwrap_or(Value::Array(Vec::new()));
    let values = match value {
        Value::Array(a) => a,
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    Ok(values
        .iter()
        .filter(|v| {
            installation.is_none_or(|i| {
                v["ExecutablePath"]
                    .as_str()
                    .is_none_or(|p| process::equivalent(p, &i.path) || i.source == Source::Npm)
            })
        })
        .filter_map(|v| v["ProcessId"].as_u64().map(|n| n as u32))
        .collect())
}
