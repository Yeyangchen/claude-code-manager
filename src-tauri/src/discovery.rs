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
    let product_name = system
        .as_ref()
        .and_then(|v| v["name"].as_str())
        .unwrap_or(std::env::consts::OS);
    let os = platform::windows_display_name(product_name, build);
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
        let owned = native_install_owned(&home, &native);
        let path = native.to_string_lossy().to_string();
        installations.push(installation(
            if owned {
                Source::Native
            } else {
                Source::External
            },
            &native,
            if owned {
                process::version(&native)
            } else {
                None
            },
            if owned { "user" } else { "unknown" },
            active,
            if owned {
                "官方原生安装目录"
            } else {
                "原生目录经重解析指向其他位置，无法确认归属，仅显示"
            },
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
            let scope = if prefix
                .canonicalize()
                .ok()
                .zip(home.canonicalize().ok())
                .is_some_and(|(prefix, home)| prefix.starts_with(home))
            {
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
                Some((
                    npm_path.to_string_lossy().into(),
                    prefix.to_string_lossy().into(),
                )),
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
    let winget_records = match winget_records() {
        Ok(records) => records,
        Err(message) => {
            checks.push(check(
                "winget-ownership",
                "WinGet 归属核查",
                "warning",
                &message,
            ));
            Vec::new()
        }
    };
    for record in &winget_records {
        let mut install = installation(
            Source::Winget,
            &record.executable,
            process::version(&record.executable),
            &record.scope,
            active,
            "WinGet portable 包，已核对卸载注册表、官方来源与安装路径",
            None,
        );
        install.fingerprint = hex::encode(Sha256::digest(
            format!("{}|{}", install.fingerprint, record.identity).as_bytes(),
        ));
        let ambiguous = winget_records
            .iter()
            .filter(|other| other.scope == record.scope)
            .count()
            > 1;
        if winget.is_none() || ambiguous {
            install.capabilities.upgrade = false;
            install.capabilities.uninstall = false;
            if ambiguous {
                install
                    .detail
                    .push_str("；发现同范围多条注册记录，请先通过 WinGet 解决歧义");
            }
        }
        installations.push(install);
    }
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
    for (root, _scope) in roots {
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
            if executable.is_file()
                && !winget_records.iter().any(|record| {
                    process::equivalent(
                        &record.executable.to_string_lossy(),
                        &executable.to_string_lossy(),
                    )
                })
            {
                let install = installation(
                    Source::External,
                    &executable,
                    None,
                    "unknown",
                    active,
                    "位于 WinGet 目录，但未能核实官方来源注册记录与安装路径，仅显示",
                    None,
                );
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
            ));
        }
    }
    if let Ok(output) = process::ps_capture("Get-ExecutionPolicy") {
        let execution_policy = match output.stdout.trim() {
            "" => "无法读取执行策略",
            value => value,
        };
        checks.push(check(
            "execution-policy",
            "PowerShell 执行策略",
            if output.stdout.contains("Restricted") {
                "warning"
            } else {
                "info"
            },
            &format!("{execution_policy}。此设置可能影响 npm 的 .ps1 启动脚本。"),
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

struct WingetRecord {
    executable: PathBuf,
    scope: String,
    identity: String,
}

fn valid_winget_record(
    package: &str,
    source: &str,
    installer_type: &str,
    location: &Path,
    executable: &Path,
) -> bool {
    if package != "Anthropic.ClaudeCode"
        || source != "Microsoft.Winget.Source_8wekyb3d8bbwe"
        || installer_type != "portable"
        || !location.is_absolute()
        || !executable.is_file()
        || !executable
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("claude.exe"))
    {
        return false;
    }
    location
        .canonicalize()
        .ok()
        .zip(executable.canonicalize().ok())
        .is_some_and(|(location, executable)| {
            executable.parent().is_some_and(|parent| {
                lexical_path(&parent.to_string_lossy()) == lexical_path(&location.to_string_lossy())
            })
        })
}

#[cfg(windows)]
fn winget_records() -> Result<Vec<WingetRecord>, String> {
    use winreg::{
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
        RegKey,
    };
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    for (hive, scope) in [(HKEY_CURRENT_USER, "user"), (HKEY_LOCAL_MACHINE, "machine")] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let root = match RegKey::predef(hive).open_subkey_with_flags(
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                KEY_READ | view,
            ) {
                Ok(root) => root,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    return Err("无法读取 WinGet 卸载记录，未核实的目录仅以只读方式显示".into())
                }
            };
            for name in root.enum_keys() {
                let name = name.map_err(|_| "无法完整枚举 WinGet 卸载记录")?;
                let Ok(key) = root.open_subkey(&name) else {
                    continue;
                };
                let package = key
                    .get_value::<String, _>("WinGetPackageIdentifier")
                    .unwrap_or_default();
                if package != "Anthropic.ClaudeCode" {
                    continue;
                }
                let source = key
                    .get_value::<String, _>("WinGetSourceIdentifier")
                    .unwrap_or_default();
                let installer_type = key
                    .get_value::<String, _>("WinGetInstallerType")
                    .unwrap_or_default();
                let location_text = key
                    .get_value::<String, _>("InstallLocation")
                    .unwrap_or_default();
                let location = PathBuf::from(&location_text);
                let target_text = key
                    .get_value::<String, _>("PortableTargetFullPath")
                    .unwrap_or_default();
                let executable = if target_text.is_empty() {
                    location.join("claude.exe")
                } else {
                    PathBuf::from(&target_text)
                };
                if !valid_winget_record(&package, &source, &installer_type, &location, &executable)
                {
                    continue;
                }
                let canonical = executable
                    .canonicalize()
                    .map_err(|_| "WinGet 安装路径在读取期间改变")?;
                if !seen.insert(format!(
                    "{scope}|{}",
                    lexical_path(&canonical.to_string_lossy())
                )) {
                    continue;
                }
                records.push(WingetRecord {
                    executable,
                    scope: scope.into(),
                    identity: format!("{scope}|{name}|{package}|{source}|{installer_type}|{location_text}|{target_text}"),
                });
            }
        }
    }
    Ok(records)
}

#[cfg(not(windows))]
fn winget_records() -> Result<Vec<WingetRecord>, String> {
    Ok(Vec::new())
}

fn installation(
    source: Source,
    path: &Path,
    version: Option<String>,
    scope: &str,
    active: Option<&String>,
    detail: &str,
    npm_environment: Option<(String, String)>,
) -> Installation {
    let (npm_path, prefix) = npm_environment
        .map(|(path, prefix)| (Some(path), Some(prefix)))
        .unwrap_or_default();
    let path_text = path.to_string_lossy().to_string();
    let manageable = source != Source::External && scope == "user";
    let native = source == Source::Native;
    let is_active = active.is_some_and(|p| {
        process::equivalent(p, &path_text)
            || (source == Source::Npm
                && Path::new(p)
                    .parent()
                    .zip(path.parent())
                    .is_some_and(|(a, b)| {
                        process::equivalent(&a.to_string_lossy(), &b.to_string_lossy())
                    }))
    });
    // Bind npm operations to both the package launcher and the package manager
    // selected during preview. Replacing npm or redirecting its prefix invalidates
    // an already confirmed plan even if claude.cmd itself remains unchanged.
    let fingerprint = hex::encode(Sha256::digest(
        format!(
            "{}|{}|{}",
            process::fingerprint(path),
            npm_path
                .as_deref()
                .map(|p| process::fingerprint(Path::new(p)))
                .unwrap_or_default(),
            prefix
                .as_deref()
                .map(|p| {
                    Path::new(p)
                        .canonicalize()
                        .unwrap_or_else(|_| PathBuf::from(p))
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_default(),
        )
        .as_bytes(),
    ));
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
        active: is_active,
        capabilities: Capabilities {
            upgrade: manageable,
            install_version: manageable && native,
            uninstall: manageable,
        },
        detail: detail.into(),
        manual_command,
        npm_path,
        prefix,
        fingerprint,
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
                r#"resolved=$(command -v claude); if [ -n "$resolved" ]; then cygpath -w "$resolved"; fi"#,
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
    let mut command = process::ps("ConvertTo-Json -InputObject @(Get-CimInstance Win32_Process -Filter \"Name='claude.exe' OR Name='node.exe'\" | Select-Object ProcessId,Name,ExecutablePath,CommandLine) -Compress");
    command.env("CCM_READ_ONLY", "1");
    let output = process::capture(command, Duration::from_secs(15))?;
    if output.code != 0 {
        return Err("无法核实运行中的 Claude 进程，请稍后重试".into());
    }
    running_from_json(&output.stdout, installation)
}

fn running_from_json(text: &str, installation: Option<&Installation>) -> Result<Vec<u32>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| "运行进程检测结果无效，不能安全开始操作")?;
    let values = match value {
        Value::Array(a) => a,
        _ => return Err("运行进程检测结果格式无效，不能安全开始操作".into()),
    };
    let mut running = Vec::new();
    for value in values {
        let pid = value["ProcessId"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .ok_or("运行进程缺少有效标识，不能安全开始操作")? as u32;
        let name = value["Name"].as_str().ok_or("无法确定运行进程归属")?;
        let matches = if name.eq_ignore_ascii_case("node.exe") {
            let command = value["CommandLine"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("没有权限读取 Node 进程命令行，无法确认 Claude 会话是否结束")?;
            let command = lexical_path(command);
            let is_claude = command.contains("@anthropic-ai\\claude-code\\")
                || command.contains("@anthropic-ai\\claude-code\"")
                || command.contains("@anthropic-ai\\claude-code'");
            // A Claude npm session whose exact prefix is unavailable still blocks
            // npm operations rather than risking removal of a running package.
            is_claude && installation.is_none_or(|i| i.source == Source::Npm)
        } else if name.eq_ignore_ascii_case("claude.exe") {
            let executable = value["ExecutablePath"].as_str().filter(|s| !s.is_empty());
            installation.is_none_or(|i| {
                executable.is_none_or(|path| {
                    process::equivalent(path, &i.path)
                        || i.source == Source::Npm
                        || (i.source == Source::Native
                            && platform::user_home().is_ok_and(|home| {
                                let versions = home.join(".local/share/claude");
                                Path::new(path)
                                    .canonicalize()
                                    .ok()
                                    .zip(versions.canonicalize().ok())
                                    .is_some_and(|(executable, versions)| {
                                        executable.starts_with(versions)
                                    })
                            }))
                })
            })
        } else {
            return Err("运行进程检测返回了未知程序，不能安全开始操作".into());
        };
        if matches {
            running.push(pid);
        }
    }
    Ok(running)
}

fn lexical_path(path: &str) -> String {
    path.trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn native_install_owned(home: &Path, launcher: &Path) -> bool {
    let Ok(home) = home.canonicalize() else {
        return false;
    };
    let expected_parent = home.join(".local/bin");
    let expected_versions = home.join(".local/share/claude");
    let Some(parent) = launcher.parent().and_then(|p| p.canonicalize().ok()) else {
        return false;
    };
    if lexical_path(&parent.to_string_lossy()) != lexical_path(&expected_parent.to_string_lossy()) {
        return false;
    }
    if expected_versions.exists() {
        let Ok(actual) = expected_versions.canonicalize() else {
            return false;
        };
        if lexical_path(&actual.to_string_lossy())
            != lexical_path(&expected_versions.to_string_lossy())
        {
            return false;
        }
    }
    let Ok(executable) = launcher.canonicalize() else {
        return false;
    };
    let actual = lexical_path(&executable.to_string_lossy());
    let launcher = lexical_path(&expected_parent.join("claude.exe").to_string_lossy());
    let versions = lexical_path(&expected_versions.to_string_lossy());
    actual == launcher || actual.starts_with(&(versions + "\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_probe_fails_closed_for_malformed_or_inaccessible_results() {
        assert!(running_from_json("", None).is_err());
        assert!(running_from_json("null", None).is_err());
        assert!(running_from_json(
            r#"[{"ProcessId":42,"Name":"node.exe","CommandLine":null}]"#,
            None
        )
        .is_err());
        assert!(running_from_json("[]", None).unwrap().is_empty());
    }

    #[test]
    fn process_probe_includes_npm_node_sessions_and_unknown_native_paths() {
        let json = r#"[{"ProcessId":42,"Name":"node.exe","CommandLine":"node C:/Users/Alice/AppData/Roaming/npm/node_modules/@anthropic-ai/claude-code/cli.js"},{"ProcessId":43,"Name":"node.exe","CommandLine":"node C:/work/server.js"},{"ProcessId":44,"Name":"claude.exe","ExecutablePath":null}]"#;
        assert_eq!(running_from_json(json, None).unwrap(), vec![42, 44]);
    }

    #[test]
    fn native_location_must_stay_in_exact_managed_tree() {
        let temporary = tempfile::tempdir().unwrap();
        let launcher = temporary.path().join(".local/bin/claude.exe");
        std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        std::fs::write(&launcher, b"fixture").unwrap();
        assert!(native_install_owned(temporary.path(), &launcher));
        let outsider = temporary.path().join("outside/claude.exe");
        std::fs::create_dir_all(outsider.parent().unwrap()).unwrap();
        std::fs::write(&outsider, b"fixture").unwrap();
        assert!(!native_install_owned(temporary.path(), &outsider));
    }

    #[test]
    fn winget_requires_official_registry_identity_and_matching_location() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("claude.exe");
        std::fs::write(&executable, b"fixture").unwrap();
        let valid = |package, source, location| {
            valid_winget_record(package, source, "portable", location, &executable)
        };
        assert!(valid(
            "Anthropic.ClaudeCode",
            "Microsoft.Winget.Source_8wekyb3d8bbwe",
            temporary.path()
        ));
        assert!(!valid(
            "Anthropic.ClaudeCode",
            "untrusted.source",
            temporary.path()
        ));
        assert!(!valid(
            "Other.Package",
            "Microsoft.Winget.Source_8wekyb3d8bbwe",
            temporary.path()
        ));
        assert!(!valid(
            "Anthropic.ClaudeCode",
            "Microsoft.Winget.Source_8wekyb3d8bbwe",
            &temporary.path().join("elsewhere")
        ));
    }
}
