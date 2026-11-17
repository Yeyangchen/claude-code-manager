use crate::domain::{Release, ReleaseCatalog};
use crate::process;
use futures_util::StreamExt;
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::crypto::{hash::HashAlgorithm, public_key::PublicKeyAlgorithm};
use pgp::packet::SignatureType;
use pgp::types::KeyDetails;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub const RELEASE_ROOT: &str = "https://downloads.claude.ai/claude-code-releases";
pub const CHANGELOG: &str =
    "https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md";
const KEY_FINGERPRINT: &str = "31DDDE24DDFAB679F42D7BD2BAA929FF1A7ECACE";
const MAX_METADATA: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformArtifact {
    pub binary: String,
    pub checksum: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: String,
    pub build_date: Option<String>,
    pub platforms: BTreeMap<String, PlatformArtifact>,
}

#[derive(Debug, Clone)]
pub struct VerifiedManifest {
    pub manifest: Manifest,
    pub signed: bool,
}

pub fn validate_version(version: &str) -> Result<String, String> {
    let parsed = semver::Version::parse(version)
        .map_err(|_| "版本格式无效，请填写例如 2.1.326 的完整版本号")?;
    if parsed.to_string() != version || !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err("Claude Code 目标版本必须为三段数字版本号".into());
    }
    Ok(parsed.to_string())
}

pub async fn metadata(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().await.map_err(network_error)?;
    if !response.status().is_success() {
        return Err(format!("来源返回 HTTP {}", response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_METADATA as u64)
    {
        return Err("版本元数据过大".into());
    }
    let mut stream = response.bytes_stream();
    let mut data = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(network_error)?;
        if data.len() + chunk.len() > MAX_METADATA {
            return Err("版本元数据过大".into());
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

pub async fn channel(client: &Client, name: &str) -> Result<String, String> {
    if name != "latest" && name != "stable" {
        return Err("未知发布通道".into());
    }
    let data = metadata(client, &format!("{RELEASE_ROOT}/{name}")).await?;
    validate_version(String::from_utf8_lossy(&data).trim())
}

pub fn verify_signature(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    let (key, _) = SignedPublicKey::from_string(include_str!("../resources/anthropic-release.asc"))
        .map_err(|e| format!("内置发布密钥解析失败: {e}"))?;
    if format!("{:X}", key.fingerprint()) != KEY_FINGERPRINT {
        return Err("发布密钥指纹不匹配".into());
    }
    if key.algorithm() != PublicKeyAlgorithm::RSA || !key.public_subkeys.is_empty() {
        return Err("发布密钥算法或结构超出固定信任范围".into());
    }
    key.verify_bindings()
        .map_err(|e| format!("发布密钥绑定验证失败: {e}"))?;
    let signature = if signature.starts_with(b"-----BEGIN") {
        DetachedSignature::from_string(std::str::from_utf8(signature).map_err(|_| "签名文本无效")?)
            .map(|v| v.0)
    } else {
        DetachedSignature::from_bytes(BufReader::new(signature))
    }
    .map_err(|e| format!("发布签名解析失败: {e}"))?;
    if !signature
        .signature
        .config()
        .is_some_and(|c| c.pub_alg == PublicKeyAlgorithm::RSA)
        || signature.signature.typ() != Some(SignatureType::Binary)
        || !matches!(
            signature.signature.hash_alg(),
            Some(HashAlgorithm::Sha256 | HashAlgorithm::Sha384 | HashAlgorithm::Sha512)
        )
    {
        return Err("发布签名算法不符合固定 RSA/SHA2 策略".into());
    }
    if signature.verify(&key, manifest).is_ok() {
        return Ok(());
    }
    Err("官方 manifest 签名验证失败".into())
}

pub async fn manifest(client: &Client, version: &str) -> Result<VerifiedManifest, String> {
    let version = validate_version(version)?;
    let bytes = metadata(client, &format!("{RELEASE_ROOT}/{version}/manifest.json")).await?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| "来源返回的 manifest 格式无效")?;
    if manifest.version != version {
        return Err("manifest 版本与请求版本不一致".into());
    }
    let signed = semver::Version::parse(&version).map_err(|e| e.to_string())?
        >= semver::Version::new(2, 1, 89);
    if signed {
        let signature = metadata(
            client,
            &format!("{RELEASE_ROOT}/{version}/manifest.json.sig"),
        )
        .await?;
        verify_signature(&bytes, &signature)?;
    }
    Ok(VerifiedManifest { manifest, signed })
}

pub fn windows_artifact<'a>(
    manifest: &'a Manifest,
    architecture: &str,
) -> Result<(&'static str, &'a PlatformArtifact), String> {
    let platform = if architecture == "arm64" || architecture == "aarch64" {
        "win32-arm64"
    } else {
        "win32-x64"
    };
    let artifact = manifest
        .platforms
        .get(platform)
        .ok_or("该版本没有对应 Windows 架构的安装文件")?;
    if artifact.binary != "claude.exe"
        || artifact.checksum.len() != 64
        || !artifact.checksum.chars().all(|c| c.is_ascii_hexdigit())
        || artifact.size == 0
        || artifact.size > 1024 * 1024 * 1024
    {
        return Err("官方文件元数据不符合要求".into());
    }
    Ok((platform, artifact))
}

pub async fn catalog(
    client: &Client,
    data_dir: &Path,
    force: bool,
) -> Result<ReleaseCatalog, String> {
    let cache_path = data_dir.join("release-cache.json");
    let cached: Option<ReleaseCatalog> = crate::storage::read_json::<ReleaseCatalog>(&cache_path)
        .ok()
        .map(|s| s.value)
        .filter(|v| !v.latest.is_empty());
    if !force {
        if let Some(mut cached) = cached.clone() {
            let recent = chrono::DateTime::parse_from_rfc3339(&cached.checked_at)
                .ok()
                .is_some_and(|t| chrono::Utc::now().signed_duration_since(t).num_hours() < 6);
            if recent {
                cached.cached = true;
                return Ok(cached);
            }
        }
    }
    let result = fetch_catalog(client).await;
    match result {
        Ok(result) => {
            let _ = crate::storage::write_json(&cache_path, &result);
            Ok(result)
        }
        Err(error) => cached
            .map(|mut value| {
                value.cached = true;
                value.warning = Some(error.clone());
                value
            })
            .ok_or(error),
    }
}

async fn fetch_catalog(client: &Client) -> Result<ReleaseCatalog, String> {
    let (latest, stable) = tokio::try_join!(channel(client, "latest"), channel(client, "stable"))?;
    let changelog = metadata(client, CHANGELOG)
        .await
        .ok()
        .map(|data| String::from_utf8_lossy(&data).into_owned())
        .unwrap_or_default();
    let mut candidates = vec![latest.clone()];
    if latest != stable {
        candidates.push(stable.clone());
    }
    for line in changelog.lines().filter_map(|l| l.strip_prefix("## ")) {
        if validate_version(line.trim()).is_ok() && !candidates.contains(&line.trim().to_string()) {
            candidates.push(line.trim().to_string());
        }
        if candidates.len() >= 16 {
            break;
        }
    }
    let mut releases = Vec::new();
    for version in candidates {
        // The two channels are verified eagerly. Historical candidates are
        // verified when a concrete plan is requested, keeping browsing fast.
        let checked = if version == latest || version == stable {
            Some(manifest(client, &version).await?)
        } else {
            None
        };
        let available = checked.as_ref().is_some_and(|m| {
            windows_artifact(&m.manifest, &crate::platform::native_architecture()).is_ok()
        });
        let release_channel = if version == latest {
            Some("latest".into())
        } else if version == stable {
            Some("stable".into())
        } else {
            None
        };
        releases.push(Release {
            version: version.clone(),
            channel: release_channel,
            published_at: checked.as_ref().and_then(|m| m.manifest.build_date.clone()),
            notes: notes_for(&changelog, &version),
            verified: checked.as_ref().is_some_and(|m| m.signed),
            available,
            source: "native".into(),
        });
    }
    Ok(ReleaseCatalog {
        latest,
        stable,
        releases,
        checked_at: chrono::Utc::now().to_rfc3339(),
        cached: false,
        warning: None,
    })
}

pub fn notes_for(changelog: &str, version: &str) -> String {
    let marker = format!("## {version}");
    let mut found = false;
    let mut result = Vec::new();
    for line in changelog.lines() {
        if line.trim() == marker {
            found = true;
            continue;
        }
        if found && line.starts_with("## ") {
            break;
        }
        if found {
            result.push(line);
        }
    }
    result.join("\n").trim().chars().take(12_000).collect()
}

pub async fn download_binary<F>(
    client: &Client,
    checked: &VerifiedManifest,
    architecture: &str,
    folder: &Path,
    cancelled: &std::sync::atomic::AtomicBool,
    progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(u64, u64) + Send,
{
    cancellable_transfer(
        cancelled,
        download_binary_inner(client, checked, architecture, folder, cancelled, progress),
    )
    .await
}

async fn cancellable_transfer<T>(
    cancelled: &std::sync::atomic::AtomicBool,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let cancellation = async {
        loop {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    };
    tokio::select! {
        biased;
        _ = cancellation => Err("cancelled".into()),
        result = work => result,
    }
}

async fn download_binary_inner<F>(
    client: &Client,
    checked: &VerifiedManifest,
    architecture: &str,
    folder: &Path,
    cancelled: &std::sync::atomic::AtomicBool,
    mut progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(u64, u64) + Send,
{
    let (platform, artifact) = windows_artifact(&checked.manifest, architecture)?;
    tokio::fs::create_dir_all(folder)
        .await
        .map_err(|e| e.to_string())?;
    let destination = folder.join(format!(
        "claude-{}-{platform}.exe",
        checked.manifest.version
    ));
    if destination.is_file() && verify_digest(&destination, artifact).is_ok() {
        verify_authenticode(&destination)?;
        return Ok(destination);
    }
    let temporary = destination.with_extension("part.exe");
    let url = format!(
        "{RELEASE_ROOT}/{version}/{platform}/claude.exe",
        version = checked.manifest.version
    );
    let response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(1800))
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err(format!(
            "下载安装文件失败 HTTP {}",
            response.status().as_u16()
        ));
    }
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|e| e.to_string())?;
    let mut downloaded = 0;
    let mut digest = Sha256::new();
    while let Some(chunk) = stream.next().await {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let chunk = chunk.map_err(network_error)?;
        downloaded += chunk.len() as u64;
        if downloaded > artifact.size {
            return Err("下载文件大小超出官方清单".into());
        }
        digest.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        progress(downloaded, artifact.size);
    }
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    if downloaded != artifact.size
        || hex::encode(digest.finalize()) != artifact.checksum.to_lowercase()
    {
        return Err("下载文件的大小或 SHA256 校验失败".into());
    }
    verify_authenticode(&temporary)?;
    tokio::fs::rename(&temporary, &destination)
        .await
        .map_err(|e| e.to_string())?;
    Ok(destination)
}

pub fn verify_digest(path: &Path, artifact: &PlatformArtifact) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != artifact.size {
        return Err("文件大小与清单不一致".into());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if hex::encode(digest.finalize()) != artifact.checksum.to_lowercase() {
        return Err("文件 SHA256 与清单不一致".into());
    }
    Ok(())
}

pub fn verify_authenticode(path: &Path) -> Result<(), String> {
    let mut command = process::ps("$s=Get-AuthenticodeSignature -LiteralPath $env:CCM_VERIFY_PATH; @{status=$s.Status.ToString(); signer=if($s.SignerCertificate){$s.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName,$false)}else{''}}|ConvertTo-Json -Compress");
    command.env("CCM_VERIFY_PATH", path);
    let result = process::capture(command, std::time::Duration::from_secs(30))?;
    validate_authenticode_output(&result)
}

fn authenticode_error(reason: &str, output: &process::Output) -> String {
    let diagnostic = format!(
        "{reason}; exit={}; stdout={}; stderr={}",
        output.code, output.stdout, output.stderr
    );
    crate::storage::redact(&diagnostic)
        .chars()
        .take(2000)
        .collect()
}

fn validate_authenticode_output(result: &process::Output) -> Result<(), String> {
    #[derive(Deserialize)]
    struct AuthenticodeResult {
        status: String,
        signer: String,
    }
    if result.code != 0 {
        return Err(authenticode_error("Windows 文件签名检测程序失败", result));
    }
    let value: AuthenticodeResult = serde_json::from_str(&result.stdout)
        .map_err(|_| authenticode_error("无法解析 Windows 文件签名检测结果", result))?;
    let valid_signer = matches!(value.signer.as_str(), "Anthropic, PBC" | "Anthropic PBC");
    if value.status != "Valid" || !valid_signer {
        return Err(authenticode_error(
            "Windows 代码签名无效或签名者不是 Anthropic",
            result,
        ));
    }
    Ok(())
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "网络请求超时，请检查代理和连接".into()
    } else if error.is_connect() {
        "无法连接版本来源，请检查网络和代理设置".into()
    } else {
        "版本来源请求失败，请稍后重试".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_commands_and_noncanonical_versions() {
        for invalid in ["latest", "2.1", "2.1.326;calc", "../../x", "2.1.326+build"] {
            assert!(validate_version(invalid).is_err());
        }
        assert_eq!(validate_version("2.1.326").unwrap(), "2.1.326");
    }

    #[test]
    fn authenticode_failures_are_explanatory_and_never_accepted() {
        let valid = process::Output {
            code: 0,
            stdout: r#"{"status":"Valid","signer":"Anthropic, PBC"}"#.into(),
            stderr: String::new(),
        };
        validate_authenticode_output(&valid).unwrap();
        for output in [
            process::Output {
                code: 1,
                stdout: valid.stdout.clone(),
                stderr: "module could not be loaded".into(),
            },
            process::Output {
                code: 1,
                stdout: String::new(),
                stderr: "CouldNotAutoloadMatchingModule".into(),
            },
            process::Output {
                code: 0,
                stdout: "not json".into(),
                stderr: String::new(),
            },
            process::Output {
                code: 0,
                stdout: r#"{"status":"NotSigned","signer":""}"#.into(),
                stderr: String::new(),
            },
            process::Output {
                code: 0,
                stdout: r#"{"status":"Valid","signer":"Microsoft Windows"}"#.into(),
                stderr: String::new(),
            },
        ] {
            let error = validate_authenticode_output(&output).unwrap_err();
            assert!(error.contains(&format!("exit={}", output.code)));
            assert!(error.contains("stdout=") && error.contains("stderr="));
        }
    }
    #[test]
    fn isolates_exact_changelog_section() {
        let log = "## 2.1.32\nA\n## 2.1.3\nB\n## 2.1.2\nC";
        assert_eq!(notes_for(log, "2.1.3"), "B");
    }
    #[test]
    fn pinned_key_and_corrupt_signature_are_rejected() {
        let (key, _) =
            SignedPublicKey::from_string(include_str!("../resources/anthropic-release.asc"))
                .unwrap();
        assert_eq!(format!("{:X}", key.fingerprint()), KEY_FINGERPRINT);
        assert_eq!(key.algorithm(), PublicKeyAlgorithm::RSA);
        assert!(key.public_subkeys.is_empty());
        assert!(verify_signature(b"{}", b"bad signature").is_err());
    }

    #[test]
    fn official_signed_fixture_verifies_and_tampering_is_rejected() {
        let manifest = include_bytes!("../../tests/fixtures/official-manifest.json");
        let signature = include_bytes!("../../tests/fixtures/official-manifest.json.sig");
        verify_signature(manifest, signature)
            .expect("official manifest must validate against pinned key");
        let mut changed = manifest.to_vec();
        let offset = changed.iter().position(|byte| *byte == b'2').unwrap();
        changed[offset] = b'3';
        assert!(verify_signature(&changed, signature).is_err());
        let mut changed_signature = signature.to_vec();
        let offset = changed_signature
            .iter()
            .position(|byte| *byte == b'i')
            .unwrap();
        changed_signature[offset] = b'j';
        assert!(verify_signature(manifest, &changed_signature).is_err());
        let parsed: Manifest = serde_json::from_slice(manifest).unwrap();
        assert_eq!(parsed.version, "2.1.326");
        assert_eq!(windows_artifact(&parsed, "x86_64").unwrap().0, "win32-x64");
    }

    #[test]
    fn binary_checksum_rejects_same_size_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("binary.fixture");
        std::fs::write(&path, b"original").unwrap();
        let artifact = PlatformArtifact {
            binary: "claude.exe".into(),
            checksum: hex::encode(Sha256::digest(b"original")),
            size: 8,
        };
        verify_digest(&path, &artifact).unwrap();
        std::fs::write(&path, b"modified").unwrap();
        assert!(verify_digest(&path, &artifact).is_err());
    }

    #[tokio::test]
    async fn cancellation_interrupts_stalled_transfer_before_installer_stage() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_request = cancelled.clone();
        let installer_reached = AtomicBool::new(false);
        let signal = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_request.store(true, Ordering::Relaxed);
        });
        // A connection/stream that never completes, without making a request.
        let stalled = async {
            std::future::pending::<()>().await;
            installer_reached.store(true, Ordering::Relaxed);
            Ok(())
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            cancellable_transfer(&cancelled, stalled),
        )
        .await
        .expect("cancel must be responsive");
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(!installer_reached.load(Ordering::Relaxed));
        signal.await.unwrap();
    }

    /// Never run on a developer machine. The explicit opt-in and fresh-profile
    /// checks are deliberate. Child processes receive a separate temporary home,
    /// settings directory and PATH so preinstalled runner CLIs are never migrated.
    #[tokio::test]
    #[ignore = "installs Claude only on an explicitly opted-in disposable GitHub runner"]
    async fn native_install_smoke() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        assert_eq!(std::env::var("CCM_RUN_INSTALL_SMOKE").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("RUNNER_ENVIRONMENT").as_deref(),
            Ok("github-hosted")
        );
        assert_eq!(std::env::var("RUNNER_OS").as_deref(), Ok("Windows"));
        let original_home = crate::platform::user_home().unwrap();
        let original_launcher = original_home.join(".local/bin/claude.exe");
        let original_fingerprint = process::fingerprint(&original_launcher);
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().join("profile");
        for directory in [
            home.clone(),
            home.join("AppData/Local"),
            home.join("AppData/Roaming"),
            home.join("claude-settings"),
        ] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let launcher = home.join(".local/bin/claude.exe");
        assert!(
            !launcher.exists(),
            "smoke requires a fresh disposable runner profile"
        );
        let system = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows SystemRoot"));
        let mut search = vec![
            system.join("System32"),
            system.clone(),
            system.join("System32/WindowsPowerShell/v1.0"),
        ];
        let git = process::resolve("git.exe");
        if let Some(parent) = git.as_ref().and_then(|path| path.parent()) {
            search.push(parent.to_path_buf());
        }
        let isolated_path = std::env::join_paths(search).unwrap();
        let isolate = |command: &mut std::process::Command| {
            command
                .current_dir(&home)
                .env("USERPROFILE", &home)
                .env("HOME", &home)
                .env("APPDATA", home.join("AppData/Roaming"))
                .env("LOCALAPPDATA", home.join("AppData/Local"))
                .env("CLAUDE_CONFIG_DIR", home.join("claude-settings"))
                .env("XDG_CONFIG_HOME", home.join(".config"))
                .env("XDG_DATA_HOME", home.join(".local/share"))
                .env("XDG_CACHE_HOME", home.join(".cache"))
                .env("PATH", &isolated_path)
                .env("npm_config_prefix", home.join("npm"))
                .env("DISABLE_AUTOUPDATER", "1")
                .env("CLAUDE_CODE_PACKAGE_MANAGER_AUTO_UPDATE", "0")
                .env_remove("DISABLE_UPDATES")
                .env_remove("NODE_OPTIONS");
            if let Some(bash) = git
                .as_ref()
                .and_then(|path| path.parent())
                .and_then(Path::parent)
                .map(|path| path.join("bin/bash.exe"))
                .filter(|path| path.is_file())
            {
                command.env("CLAUDE_CODE_GIT_BASH_PATH", bash);
            }
        };
        let client = reqwest::Client::builder()
            .user_agent("claude-code-manager-isolated-smoke/0.1")
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap();
        let target = manifest(&client, "2.1.326").await.unwrap();
        let older = manifest(&client, "2.1.311").await.unwrap();
        let bootstrap_version = channel(&client, "latest").await.unwrap();
        let bootstrap = manifest(&client, &bootstrap_version).await.unwrap();
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let binary = download_binary(
            &client,
            &bootstrap,
            &crate::platform::native_architecture(),
            temporary.path(),
            &cancelled,
            |_, _| {},
        )
        .await
        .unwrap();
        let mut help_command =
            process::executable_command(&binary, &["install".into(), "--help".into()]);
        isolate(&mut help_command);
        let help = process::capture(help_command, std::time::Duration::from_secs(30)).unwrap();
        let force_supported = help.stdout.split_whitespace().any(|word| word == "--force");
        println!("bootstrap={bootstrap_version}; --force={force_supported}");
        println!("install help: {}", crate::storage::redact(&help.stdout));
        for (target, reinstall) in [
            (&target, false),
            (&target, true),
            (&older, false),
            (&target, false),
        ] {
            let mut args = vec!["install".into(), target.manifest.version.clone()];
            if reinstall && force_supported {
                args.push("--force".into());
            }
            let mut command = process::executable_command(&binary, &args);
            isolate(&mut command);
            let output = process::capture(command, std::time::Duration::from_secs(600)).unwrap();
            println!(
                "reinstall={reinstall}; exit={}; {}",
                output.code,
                crate::storage::redact(&format!("{}\n{}", output.stdout, output.stderr))
            );
            let mut version_command = process::executable_command(&launcher, &["--version".into()]);
            isolate(&mut version_command);
            let version_output =
                process::capture(version_command, std::time::Duration::from_secs(30)).unwrap();
            assert_eq!(
                process::parse_version(&version_output.stdout).as_deref(),
                Some(target.manifest.version.as_str())
            );
            let (_, artifact) =
                windows_artifact(&target.manifest, &crate::platform::native_architecture())
                    .unwrap();
            verify_digest(&launcher, artifact).unwrap();
            verify_authenticode(&launcher).unwrap();
            assert_eq!(
                process::fingerprint(&original_launcher),
                original_fingerprint,
                "isolated installation touched the runner's original launcher"
            );
        }
    }
}
