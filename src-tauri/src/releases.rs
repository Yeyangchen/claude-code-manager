use crate::domain::{Release, ReleaseCatalog};
use crate::process;
use futures_util::StreamExt;
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
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
    key.verify_bindings()
        .map_err(|e| format!("发布密钥绑定验证失败: {e}"))?;
    let signature = if signature.starts_with(b"-----BEGIN") {
        DetachedSignature::from_string(std::str::from_utf8(signature).map_err(|_| "签名文本无效")?)
            .map(|v| v.0)
    } else {
        DetachedSignature::from_bytes(BufReader::new(signature))
    }
    .map_err(|e| format!("发布签名解析失败: {e}"))?;
    if signature.verify(&key, manifest).is_ok() {
        return Ok(());
    }
    for subkey in &key.public_subkeys {
        if signature.verify(subkey, manifest).is_ok() {
            return Ok(());
        }
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
    let temporary = destination.with_extension("part");
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
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).map_err(|_| "无法读取 Windows 文件签名")?;
    let valid_signer = matches!(
        value["signer"].as_str(),
        Some("Anthropic, PBC") | Some("Anthropic PBC")
    );
    if result.code != 0 || value["status"] != "Valid" || !valid_signer {
        return Err("Windows 代码签名无效或签名者不是 Anthropic".into());
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
        assert!(verify_signature(b"{}", b"bad signature").is_err());
    }
}
