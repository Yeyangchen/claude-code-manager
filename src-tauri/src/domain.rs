use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::new("operation_failed", message)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Native,
    Npm,
    Winget,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub upgrade: bool,
    pub install_version: bool,
    pub uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: String,
    pub source: Source,
    pub path: String,
    pub version: Option<String>,
    pub scope: String,
    pub active: bool,
    pub capabilities: Capabilities,
    pub detail: String,
    pub manual_command: Option<String>,
    #[serde(skip)]
    pub npm_path: Option<String>,
    #[serde(skip)]
    pub prefix: Option<String>,
    #[serde(skip)]
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: String,
    pub title: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellResolution {
    pub name: String,
    pub path: Option<String>,
    pub candidates: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub os: String,
    pub arch: String,
    pub user_name: String,
    pub elevated: bool,
    pub config_dir: String,
    pub supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyState {
    pub mode: String,
    pub channel: String,
    pub minimum_version: Option<String>,
    pub disabled_updates: bool,
    pub sources: Vec<PolicySource>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySource {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub proxy_mode: String,
    pub proxy_url: String,
    pub has_proxy_credentials: bool,
    pub check_interval_hours: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            has_proxy_credentials: false,
            check_interval_hours: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub phase: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub messages: Vec<String>,
    pub cancellable: bool,
    pub progress: Option<DownloadProgress>,
    pub result: Option<String>,
    pub installation_id: Option<String>,
    pub target_version: Option<String>,
    #[serde(default)]
    pub child_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub app_version: String,
    pub environment: Environment,
    pub installations: Vec<Installation>,
    pub shells: Vec<ShellResolution>,
    pub checks: Vec<Check>,
    pub policy: PolicyState,
    pub settings: Settings,
    pub active_operation: Option<Operation>,
    pub history: Vec<Operation>,
    pub read_only: bool,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub channel: Option<String>,
    pub published_at: Option<String>,
    pub notes: String,
    pub verified: bool,
    pub available: bool,
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCatalog {
    pub latest: String,
    pub stable: String,
    pub releases: Vec<Release>,
    pub checked_at: String,
    pub cached: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRequest {
    pub kind: String,
    pub installation_id: Option<String>,
    pub target_version: Option<String>,
    pub channel: Option<String>,
    pub policy_mode: Option<String>,
    pub minimum_version: Option<String>,
    pub change_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanChange {
    pub name: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub installation_id: Option<String>,
    pub current_version: Option<String>,
    pub target_version: Option<String>,
    pub steps: Vec<String>,
    pub warnings: Vec<String>,
    pub changes: Vec<PlanChange>,
    pub can_execute: bool,
    pub manual_command: Option<String>,
    pub checksum_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub id: String,
    pub kind: String,
    pub description: String,
    pub created_at: String,
    pub restored: bool,
    pub can_restore: bool,
    pub detail: String,
    pub comparison: Vec<ChangeComparison>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeComparison {
    pub name: String,
    pub before: String,
    pub written: String,
    pub current: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerUpdate {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub available: bool,
    pub download_url: String,
    pub signed_updater_ready: bool,
    pub message: String,
}
