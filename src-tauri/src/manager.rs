use crate::domain::*;
use crate::{discovery, path_edit, platform, policy, process, proxy, releases, storage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone)]
enum Action {
    Install {
        installation: Option<Installation>,
        manifest: releases::VerifiedManifest,
    },
    Uninstall(Installation),
    Path(path_edit::PathChange),
    Policy(policy::PolicyChange),
    RestorePath(String),
    RestorePolicy(String),
}

#[derive(Clone)]
struct StoredPlan {
    preview: Plan,
    action: Action,
    created: Instant,
}

#[derive(Default, Serialize, Deserialize)]
struct TaskHistory {
    operations: Vec<Operation>,
}

pub struct Manager {
    pub data_dir: PathBuf,
    plans: Mutex<HashMap<String, StoredPlan>>,
    history: Mutex<TaskHistory>,
    cancelled: Mutex<HashMap<String, Arc<AtomicBool>>>,
    scan_cache: Mutex<Option<(Instant, discovery::Discovery)>>,
    startup_warning: Mutex<Option<String>>,
    read_only: AtomicBool,
    owns_task: AtomicBool,
}

impl Manager {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        let startup_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data_dir.join("operation.lock"))
            .map_err(|e| e.to_string())?;
        let can_recover = startup_lock.try_lock().is_ok();
        let loaded = storage::read_json::<TaskHistory>(&data_dir.join("tasks.json"))?;
        let settings = storage::read_json::<Settings>(&data_dir.join("settings.json"))?;
        let mut history = loaded.value;
        for operation in &mut history.operations {
            if !can_recover {
                break;
            }
            if operation.status == "running"
                && operation.kind == "manager-update"
                && operation.target_version.as_deref() == Some(env!("CARGO_PKG_VERSION"))
            {
                operation.status = "succeeded".into();
                operation.phase = "completed".into();
                operation.cancellable = false;
                operation.finished_at = Some(chrono::Utc::now().to_rfc3339());
                operation.result = Some("管理器已完成签名更新，当前运行版本符合目标。".into());
            } else if operation.status == "running" {
                operation.status = "attention".into();
                operation.phase = "recovering".into();
                operation.cancellable = false;
                operation.result =
                    Some("上次任务中断。已保留操作记录，请重新检测安装和变更台账后再操作。".into());
            }
        }
        let manager = Self {
            data_dir,
            plans: Mutex::new(HashMap::new()),
            history: Mutex::new(history),
            cancelled: Mutex::new(HashMap::new()),
            scan_cache: Mutex::new(None),
            startup_warning: Mutex::new(None),
            read_only: AtomicBool::new(loaded.read_only || settings.read_only),
            owns_task: AtomicBool::new(false),
        };
        if can_recover && !manager.read_only.load(Ordering::Relaxed) {
            manager.persist_history()?;
        }
        Ok(manager)
    }

    fn persist_history(&self) -> Result<(), String> {
        let history = self.history.lock().map_err(|_| "任务状态锁异常")?;
        storage::write_json(&self.data_dir.join("tasks.json"), &*history)
    }

    fn discovery(&self, force: bool) -> Result<discovery::Discovery, String> {
        let mut cache = self.scan_cache.lock().map_err(|_| "检测状态锁异常")?;
        if !force {
            if let Some((time, discovery)) = cache.as_ref() {
                if time.elapsed() < Duration::from_secs(20) {
                    return Ok(discovery.clone());
                }
            }
        }
        let discovery = discovery::scan()?;
        *cache = Some((Instant::now(), discovery.clone()));
        Ok(discovery)
    }

    pub fn settings(&self) -> Result<Settings, String> {
        let result = storage::read_json::<Settings>(&self.data_dir.join("settings.json"))?;
        if result.read_only {
            self.read_only.store(true, Ordering::Relaxed);
        }
        let proxy_state = proxy::load_proxy(&self.data_dir)?;
        Ok(Settings {
            proxy_mode: proxy_state.mode,
            proxy_url: proxy_state.url,
            has_proxy_credentials: proxy_state.has_credentials,
            ..result.value
        })
    }

    pub fn save_settings(
        &self,
        settings: Settings,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Settings, String> {
        self.ensure_writable()?;
        let _lock = self.operation_lock()?;
        if !["system", "light", "dark"].contains(&settings.theme.as_str())
            || settings.check_interval_hours > 168
        {
            return Err("设置值无效".into());
        }
        let original_proxy = proxy::load_proxy(&self.data_dir)?;
        proxy::save_proxy(
            &self.data_dir,
            proxy::ProxySettings {
                mode: settings.proxy_mode.clone(),
                url: settings.proxy_url.clone(),
                has_credentials: false,
            },
            username,
            password,
        )?;
        if let Err(error) = storage::write_json(&self.data_dir.join("settings.json"), &settings) {
            let _ = proxy::save_proxy(&self.data_dir, original_proxy, None, None);
            return Err(error);
        }
        self.settings()
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.refresh_external_history()?;
        let discovery = self.discovery(false)?;
        self.recover_interrupted(&discovery)?;
        let settings = self.settings()?;
        let policy = policy::read_policy(&self.data_dir)?;
        let history = self
            .history
            .lock()
            .map_err(|_| "任务状态锁异常")?
            .operations
            .clone();
        let mut checks = discovery.checks;
        if policy.mode == "hold" {
            if let Some(record) = policy::list_changes(&self.data_dir)?
                .into_iter()
                .find(|r| r.state == "applied" && r.mode == "hold")
            {
                let drift = hold_drift(&record.held_versions, &discovery.installations);
                if !drift.is_empty() {
                    checks.push(Check {
                        id: "hold-version-drift".into(),
                        title: "保持版本已发生变化".into(),
                        level: "warning".into(),
                        message: drift.join("；"),
                    });
                }
            }
        }
        if let Some(message) = self
            .startup_warning
            .lock()
            .map_err(|_| "状态锁异常")?
            .clone()
        {
            checks.push(Check {
                id: "storage".into(),
                title: "操作记录".into(),
                level: "warning".into(),
                message,
            });
        }
        Ok(Snapshot {
            app_version: env!("CARGO_PKG_VERSION").into(),
            environment: discovery.environment,
            installations: discovery.installations,
            shells: discovery.shells,
            checks,
            policy: PolicyState {
                mode: policy.mode,
                channel: policy.channel,
                minimum_version: policy.minimum_version,
                disabled_updates: policy.disabled_updates,
                sources: policy
                    .sources
                    .into_iter()
                    .map(|s| PolicySource {
                        name: s.name,
                        value: s.value,
                    })
                    .collect(),
                detail: policy.detail,
            },
            settings,
            active_operation: history.iter().find(|o| o.status == "running").cloned(),
            history,
            read_only: self.read_only.load(Ordering::Relaxed),
            data_dir: self.data_dir.to_string_lossy().into(),
        })
    }

    pub async fn create_plan(self: &Arc<Self>, request: PlanRequest) -> Result<Plan, String> {
        self.ensure_writable()?;
        let manager = self.clone();
        let discovery = tokio::task::spawn_blocking(move || manager.discovery(true))
            .await
            .map_err(|e| e.to_string())??;
        if !discovery.environment.supported {
            return Err("当前系统不满足 Windows 10 1809 要求".into());
        }
        let installation = match request.installation_id.as_ref() {
            Some(id) => Some(
                discovery
                    .installations
                    .iter()
                    .find(|i| &i.id == id)
                    .cloned()
                    .ok_or("安装实例已经改变，请重新检测")?,
            ),
            None => None,
        };
        let mut preview = Plan {
            id: uuid::Uuid::new_v4().to_string(),
            kind: request.kind.clone(),
            title: String::new(),
            installation_id: request.installation_id.clone(),
            current_version: installation.as_ref().and_then(|i| i.version.clone()),
            target_version: None,
            steps: vec!["复核安装和配置状态".into()],
            warnings: Vec::new(),
            changes: Vec::new(),
            can_execute: true,
            manual_command: None,
            checksum_only: false,
        };
        let action = match request.kind.as_str() {
            "install" | "update" | "reinstall" => {
                let policy = policy::read_policy(&self.data_dir)?;
                if policy.disabled_updates {
                    return Err(
                        "DISABLE_UPDATES 已禁止安装和更新，请在设置来源中处理后重新检查".into(),
                    );
                }
                if request.kind == "install"
                    && discovery
                        .installations
                        .iter()
                        .any(|i| i.source == Source::Native)
                {
                    return Err("已存在原生安装，请选择该实例进行更新或重装".into());
                }
                if request.kind == "install" {
                    validate_native_destination(&platform::user_home()?, true)?;
                }
                if request.kind != "install" && installation.is_none() {
                    return Err("请选择安装实例".into());
                }
                if let Some(install) = &installation {
                    if !install.capabilities.upgrade {
                        preview.can_execute = false;
                        preview.manual_command = install.manual_command.clone();
                    }
                    if request.kind == "reinstall" && !install.capabilities.install_version {
                        return Err("该安装来源暂未开放指定版本重装".into());
                    }
                }
                let client = proxy::reqwest_client(&self.data_dir)?;
                let target = match request.target_version.as_deref() {
                    Some(version) => releases::validate_version(version)?,
                    None => {
                        releases::channel(
                            &client,
                            request.channel.as_deref().unwrap_or(&policy.channel),
                        )
                        .await?
                    }
                };
                if let Some(install) = installation.as_ref().filter(|i| !i.capabilities.upgrade) {
                    preview.manual_command = Some(manual_command(install, false, Some(&target)));
                }
                if let Some(minimum) = policy
                    .minimum_version
                    .as_deref()
                    .and_then(|v| semver::Version::parse(v).ok())
                {
                    if semver::Version::parse(&target).map_err(|e| e.to_string())? < minimum {
                        return Err(format!("目标版本低于生效的 minimumVersion {minimum}"));
                    }
                }
                let checked = releases::manifest(&client, &target).await?;
                releases::windows_artifact(&checked.manifest, &discovery.environment.arch)?;
                preview.checksum_only = !checked.signed;
                if preview.checksum_only {
                    preview
                        .warnings
                        .push("该历史版本仅提供 SHA256 摘要，安装前需明确确认。".into());
                }
                if let Some(install) = installation.as_ref() {
                    match install.source {
                        Source::Npm => {
                            verify_npm_version(&client, &target).await?;
                        }
                        Source::Winget => {
                            verify_winget_version(&target)?;
                        }
                        Source::External => {
                            return Err("该安装由其他应用管理，请使用原应用升级".into())
                        }
                        Source::Native => {}
                    }
                }
                preview.title = if request.kind == "install" {
                    "安装 Claude Code"
                } else if request.kind == "reinstall" {
                    "重装指定版本"
                } else {
                    "更新 Claude Code"
                }
                .into();
                preview.target_version = Some(target);
                if let (Some(current), Some(target)) = (
                    preview.current_version.as_deref(),
                    preview.target_version.as_deref(),
                ) {
                    if semver::Version::parse(target).ok() < semver::Version::parse(current).ok() {
                        preview.warnings.push(format!(
                            "目标 {target} 低于当前 {current}，此次操作将重装旧版本。"
                        ));
                    }
                }
                preview.changes.push(PlanChange {
                    name: "Claude Code 版本".into(),
                    before: preview
                        .current_version
                        .clone()
                        .unwrap_or_else(|| "未安装".into()),
                    after: preview.target_version.clone().unwrap_or_default(),
                });
                preview.steps.extend([
                    "等待使用此实例的会话结束".into(),
                    "通过对应安装来源执行安装".into(),
                    "核验目标版本、签名与终端路径".into(),
                ]);
                preview.warnings.push(
                    "运行中的终端需要重新启动后使用新版本。原生回退通过联网重装旧版本完成。".into(),
                );
                Action::Install {
                    installation,
                    manifest: checked,
                }
            }
            "uninstall" => {
                let install = installation.ok_or("请选择要卸载的实例")?;
                preview.title = "卸载选中的安装实例".into();
                preview.can_execute = install.capabilities.uninstall;
                preview.manual_command =
                    (!preview.can_execute).then(|| manual_command(&install, true, None));
                preview.steps.extend([
                    format!("移除该来源的安装文件 {}", install.path),
                    "重新核查其他安装和终端命中关系".into(),
                ]);
                preview
                    .warnings
                    .push("用户配置、会话、项目文件和其他安装实例继续保留。".into());
                if discovery
                    .installations
                    .iter()
                    .filter(|i| i.source != Source::External)
                    .count()
                    <= 1
                {
                    preview
                        .warnings
                        .push("这是最后一个可管理的 Claude Code 安装。".into());
                }
                Action::Uninstall(install)
            }
            "path-repair" => {
                preview.title = "修复用户级 PATH".into();
                let change = if let Some(install) = installation {
                    if install.source == Source::External {
                        return Err("无法确认来源的实例不参与自动 PATH 修复".into());
                    }
                    let directories = discovery
                        .installations
                        .iter()
                        .filter(|i| i.source != Source::External)
                        .filter_map(|i| Path::new(&i.path).parent().map(Path::to_path_buf))
                        .collect::<Vec<_>>();
                    path_edit::preview_prefer(
                        &directories,
                        Path::new(&install.path)
                            .parent()
                            .ok_or("安装路径无父目录")?,
                    )?
                } else if discovery
                    .installations
                    .iter()
                    .any(|i| i.source == Source::Native)
                {
                    path_edit::preview_append_native()?
                } else {
                    // A missing default native directory is the only unselected
                    // cleanup target. Existing shared directories are retained.
                    let native_bin = platform::user_home()?.join(".local/bin");
                    if native_bin.exists() {
                        return Err("未发现原生安装，请选择已识别实例修复 PATH".into());
                    }
                    let change = path_edit::preview_remove_missing(&[native_bin])?;
                    if change.before.value == change.after.value {
                        return Err("未发现可安全修复的原生 PATH 条目，请先选择安装实例".into());
                    }
                    change
                };
                preview.changes.push(PlanChange {
                    name: "HKCU Environment Path".into(),
                    before: change.before.value.clone(),
                    after: change.after.value.clone(),
                });
                preview.steps.extend([
                    "保存原始注册表值和独立备份".into(),
                    "仅修改用户级 PATH 并广播环境变更".into(),
                    "通过新进程核查命中结果".into(),
                ]);
                preview.warnings.push(
                    "已打开的终端需要重新启动。系统级 PATH 的优先级可能继续影响命中结果。".into(),
                );
                Action::Path(change)
            }
            "policy" => {
                preview.title = "修改更新策略".into();
                let source = installation
                    .as_ref()
                    .map(|i| match i.source {
                        Source::Winget => "winget",
                        Source::Npm => "npm",
                        _ => "native",
                    })
                    .unwrap_or("native");
                let mut change = policy::preview_policy(&policy::PolicyRequest {
                    mode: request.policy_mode.ok_or("请选择更新策略")?,
                    channel: request.channel,
                    minimum_version: request.minimum_version,
                    source: source.into(),
                })?;
                if change.mode == "hold" {
                    change.held_versions = discovery
                        .installations
                        .iter()
                        .filter_map(|i| i.version.as_ref().map(|v| (i.id.clone(), v.clone())))
                        .collect();
                    if change.held_versions.is_empty() {
                        return Err("尚无可识别的已安装版本，无法记录保持版本基线".into());
                    }
                }
                preview.changes = change
                    .fields
                    .iter()
                    .map(|f| PlanChange {
                        name: f.pointer.clone(),
                        before: f
                            .before
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "未设置".into()),
                        after: f
                            .after
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "移除".into()),
                    })
                    .collect();
                preview.warnings.push(
                    "共享同一配置目录的安装会受此策略影响。项目与组织策略可能覆盖用户设置。".into(),
                );
                preview.steps.extend([
                    "按字段合并配置并登记可撤销变更".into(),
                    "读取策略来源并核查生效结果".into(),
                ]);
                Action::Policy(change)
            }
            "restore-change" => {
                let id = request.change_id.ok_or("缺少变更记录 ID")?;
                preview.title = "还原管理器所做的更改".into();
                if path_edit::list_changes(&self.data_dir)?
                    .iter()
                    .any(|change| change.id == id)
                {
                    let change = path_edit::preview_restore(&self.data_dir, &id)?;
                    preview.changes.push(PlanChange {
                        name: "用户 PATH".into(),
                        before: change.after.value,
                        after: change.before.value,
                    });
                    Action::RestorePath(id)
                } else {
                    let change = policy::preview_restore(&self.data_dir, &id)?;
                    preview.changes = change
                        .fields
                        .iter()
                        .map(|f| PlanChange {
                            name: f.pointer.clone(),
                            before: f
                                .after
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default(),
                            after: f
                                .before
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default(),
                        })
                        .collect();
                    Action::RestorePolicy(id)
                }
            }
            _ => return Err("未知操作类型".into()),
        };
        let mut plans = self.plans.lock().map_err(|_| "计划状态锁异常")?;
        plans.retain(|_, p| p.created.elapsed() < Duration::from_secs(900));
        if plans.len() >= 32 {
            return Err("待确认计划过多，请重新打开应用".into());
        }
        plans.insert(
            preview.id.clone(),
            StoredPlan {
                preview: preview.clone(),
                action,
                created: Instant::now(),
            },
        );
        Ok(preview)
    }

    fn ensure_writable(&self) -> Result<(), String> {
        for name in ["tasks.json", "settings.json", "proxy.json"] {
            if storage::read_json::<serde_json::Value>(&self.data_dir.join(name))?.read_only {
                self.read_only.store(true, Ordering::Relaxed);
            }
        }
        if self.read_only.load(Ordering::Relaxed) {
            return Err("数据来自更新版本的管理器，当前处于只读模式".into());
        }
        Ok(())
    }

    fn operation_lock(&self) -> Result<File, String> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.data_dir.join("operation.lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock()
            .map_err(|_| "另一个管理器操作正在执行，请等待完成")?;
        self.ensure_writable()?;
        self.refresh_external_history()?;
        let interrupted = self
            .history
            .lock()
            .map_err(|_| "任务状态锁异常")?
            .operations
            .iter()
            .filter(|o| o.phase == "recovering")
            .filter_map(|o| o.child_pid)
            .collect::<Vec<_>>();
        for pid in interrupted {
            if process::pid_running(pid)? {
                return Err(format!(
                    "上次任务的子进程 {pid} 仍在运行，请等其结束后重新检测"
                ));
            }
        }
        Ok(file)
    }

    fn recover_interrupted(&self, discovery: &discovery::Discovery) -> Result<(), String> {
        if self.owns_task.load(Ordering::Relaxed) {
            return Ok(());
        }
        let guard = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.data_dir.join("operation.lock"))
            .map_err(|e| e.to_string())?;
        if guard.try_lock().is_err() {
            return Ok(());
        }
        {
            let mut history = self.history.lock().map_err(|_| "任务状态锁异常")?;
            for operation in history
                .operations
                .iter_mut()
                .filter(|o| o.status == "running")
            {
                operation.status = "attention".into();
                operation.phase = "recovering".into();
                operation.cancellable = false;
            }
        }
        let interrupted = self
            .history
            .lock()
            .map_err(|_| "任务状态锁异常")?
            .operations
            .iter()
            .filter(|o| o.phase == "recovering")
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = false;
        for operation in interrupted {
            if let Some(pid) = operation.child_pid {
                if process::pid_running(pid)? {
                    continue;
                }
            }
            let actual = discovery.installations.iter().find(|i| {
                operation
                    .installation_id
                    .as_ref()
                    .map_or(i.source == Source::Native, |id| &i.id == id)
            });
            let detail = if operation.kind == "uninstall" && actual.is_none() {
                "重新检测未发现原实例，上次卸载的退出状态未知。".to_string()
            } else if let Some(actual) = actual {
                format!(
                    "重新检测发现 {}，实际版本 {}；原任务退出状态未知，请按当前状态操作。",
                    actual.path,
                    actual.version.as_deref().unwrap_or("不可用")
                )
            } else {
                "重新检测未发现目标安装，请检查任务日志并重新生成安装计划。".to_string()
            };
            if let Some(record) = self
                .history
                .lock()
                .map_err(|_| "任务状态锁异常")?
                .operations
                .iter_mut()
                .find(|o| o.id == operation.id)
            {
                record.phase = "recovered".into();
                record.finished_at = Some(chrono::Utc::now().to_rfc3339());
                record.child_pid = None;
                record.result = Some(detail);
                changed = true;
            }
        }
        if changed && !self.read_only.load(Ordering::Relaxed) {
            self.persist_history()?;
        }
        Ok(())
    }

    pub fn operation(&self, id: &str) -> Result<Operation, String> {
        self.refresh_external_history()?;
        self.history
            .lock()
            .map_err(|_| "任务状态锁异常")?
            .operations
            .iter()
            .find(|o| o.id == id)
            .cloned()
            .ok_or_else(|| "未找到任务".into())
    }

    fn update_operation(
        &self,
        app: &tauri::AppHandle,
        id: &str,
        change: impl FnOnce(&mut Operation),
    ) {
        let updated = {
            let Ok(mut history) = self.history.lock() else {
                return;
            };
            let Some(operation) = history.operations.iter_mut().find(|o| o.id == id) else {
                return;
            };
            change(operation);
            operation.clone()
        };
        if let Err(message) = self.persist_history() {
            if let Ok(mut warning) = self.startup_warning.lock() {
                *warning = Some(format!("保存操作记录失败: {message}"));
            }
        }
        let _ = app.emit("manager://operation", updated);
    }

    pub fn cancel(&self, id: &str) -> Result<Operation, String> {
        if !self.owns_task.load(Ordering::Relaxed) {
            return Err("任务在另一个管理器窗口执行，请在发起窗口取消".into());
        }
        let operation = self.operation(id)?;
        if !operation.cancellable || operation.status != "running" {
            return Err("当前安装阶段需要完成后再操作".into());
        }
        self.cancelled
            .lock()
            .map_err(|_| "任务状态锁异常")?
            .get(id)
            .ok_or("未找到活动任务")?
            .store(true, Ordering::Relaxed);
        Ok(operation)
    }

    pub fn execute(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        plan_id: &str,
        allow_checksum_only: bool,
    ) -> Result<Operation, String> {
        self.ensure_writable()?;
        let lock = self.operation_lock()?;
        let stored = self
            .plans
            .lock()
            .map_err(|_| "计划状态锁异常")?
            .remove(plan_id)
            .ok_or("计划已失效，请重新生成")?;
        if stored.created.elapsed() > Duration::from_secs(900) {
            return Err("计划已过期，请重新检查".into());
        }
        if !stored.preview.can_execute {
            return Err("此操作需要在对应权限的终端执行".into());
        }
        if stored.preview.checksum_only && !allow_checksum_only {
            return Err("请确认该历史版本仅提供摘要校验".into());
        }
        let operation = Operation {
            id: uuid::Uuid::new_v4().to_string(),
            kind: stored.preview.kind.clone(),
            title: stored.preview.title.clone(),
            phase: "preparing".into(),
            status: "running".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            messages: Vec::new(),
            cancellable: true,
            progress: None,
            result: None,
            installation_id: stored.preview.installation_id.clone(),
            target_version: stored.preview.target_version.clone(),
            child_pid: None,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.owns_task.store(true, Ordering::Relaxed);
        self.cancelled
            .lock()
            .map_err(|_| "任务状态锁异常")?
            .insert(operation.id.clone(), cancelled.clone());
        {
            let mut history = self.history.lock().map_err(|_| "任务状态锁异常")?;
            history.operations.insert(0, operation.clone());
            history.operations.truncate(100);
        }
        if let Err(error) = self.persist_history() {
            self.owns_task.store(false, Ordering::Relaxed);
            if let Ok(mut history) = self.history.lock() {
                history.operations.retain(|o| o.id != operation.id);
            }
            if let Ok(mut cancelled) = self.cancelled.lock() {
                cancelled.remove(&operation.id);
            }
            return Err(error);
        }
        let manager = self.clone();
        let id = operation.id.clone();
        tauri::async_runtime::spawn(async move {
            let _operation_lock = lock;
            let result = manager
                .run_action(&app, &id, stored.action, &cancelled)
                .await;
            if let Ok(mut cache) = manager.scan_cache.lock() {
                *cache = None;
            }
            manager.update_operation(&app, &id, |operation| {
                operation.finished_at = Some(chrono::Utc::now().to_rfc3339());
                operation.cancellable = false;
                operation.child_pid = None;
                match result {
                    Ok(message) => {
                        operation.status = "succeeded".into();
                        operation.phase = "completed".into();
                        operation.result = Some(message);
                    }
                    Err(message) if message == "cancelled" => {
                        operation.status = "cancelled".into();
                        operation.phase = "cancelled".into();
                        operation.result = Some("任务已取消".into());
                    }
                    Err(message) => {
                        operation.status = "attention".into();
                        operation.phase = "recovering".into();
                        operation.result = Some(storage::redact(&message));
                    }
                }
            });
            if let Ok(mut cancelled) = manager.cancelled.lock() {
                cancelled.remove(&id);
            }
            manager.owns_task.store(false, Ordering::Relaxed);
        });
        Ok(operation)
    }

    async fn run_action(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        id: &str,
        action: Action,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<String, String> {
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        self.ensure_writable()?;
        match action {
            Action::Install {
                installation,
                manifest,
            } => {
                self.install(app, id, installation, manifest, cancelled)
                    .await
            }
            Action::Uninstall(installation) => {
                self.wait_for_sessions(app, id, Some(&installation), cancelled)
                    .await?;
                self.recheck_installation(Some(&installation))?;
                self.ensure_writable()?;
                if cancelled.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                self.update_operation(app, id, |o| {
                    o.phase = "installing".into();
                    o.cancellable = false;
                });
                let manager = self.clone();
                let app = app.clone();
                let id = id.to_string();
                tokio::task::spawn_blocking(move || {
                    let exit_code = match installation.source {
                        Source::Native => { uninstall_native(&installation)?; 0 },
                        Source::Npm => {
                            let npm = installation.npm_path.as_ref().ok_or("npm 路径缺失")?;
                            let prefix = installation.prefix.as_ref().ok_or("npm prefix 缺失")?;
                            let command = process::executable_command(
                                Path::new(npm),
                                &[
                                    "uninstall".into(),
                                    "-g".into(),
                                    "@anthropic-ai/claude-code".into(),
                                    "--prefix".into(),
                                    prefix.clone(),
                                ],
                            );
                            manager.run_installer(&app, &id, command)?
                        }
                        Source::Winget => {
                            let winget = process::resolve("winget.exe").ok_or("WinGet 不可用")?;
                            let mut command = Command::new(winget);
                            command.args([
                                "uninstall",
                                "--id",
                                "Anthropic.ClaudeCode",
                                "--exact",
                                "--scope",
                                "user",
                                "--silent",
                                "--disable-interactivity",
                            ]);
                            manager.run_installer(&app, &id, command)?
                        }
                        Source::External => return Err("此实例由其他应用管理".into()),
                    };
                    let discovery = manager.discovery(true)?;
                    if discovery
                        .installations
                        .iter()
                        .any(|i| i.id == installation.id)
                    {
                        return Err(format!("卸载程序退出码 {exit_code}，复核后仍发现原实例，请查看诊断"));
                    }
                    Ok(format!("选定安装已移除，用户配置与其他实例已保留。终端命中关系已重新检测。卸载程序退出码 {exit_code}。"))
                })
                .await
                .map_err(|e| e.to_string())?
            }
            Action::Path(change) => {
                self.update_operation(app, id, |o| {
                    o.phase = "applying".into();
                    o.cancellable = false;
                });
                path_edit::apply_change(&self.data_dir, &change)?;
                let after = self.discovery(true)?;
                let resolved = after
                    .shells
                    .iter()
                    .map(|shell| {
                        format!(
                            "{}: {}",
                            shell.name,
                            shell.path.as_deref().unwrap_or("未命中")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("；");
                Ok(format!("用户级 PATH 已更新并保存可撤销记录。新进程核查：{resolved}。请重新打开已有终端。"))
            }
            Action::Policy(change) => {
                self.update_operation(app, id, |o| {
                    o.phase = "applying".into();
                    o.cancellable = false;
                });
                policy::apply_policy(&self.data_dir, &change)?;
                let policy = policy::read_policy(&self.data_dir)?;
                if let Some(installation) = self
                    .discovery(true)?
                    .installations
                    .iter()
                    .find(|i| i.active && i.source != Source::External)
                {
                    let report = self.doctor_report(installation).unwrap_or_else(|e| e);
                    self.update_operation(app, id, |o| o.messages.push(report));
                }
                Ok(format!("更新策略已保存。当前推断模式为 {}，请在目标项目终端运行 claude doctor 核实组织与项目覆盖。", policy.mode))
            }
            Action::RestorePath(change_id) => {
                self.update_operation(app, id, |o| {
                    o.phase = "applying".into();
                    o.cancellable = false;
                });
                path_edit::restore_change(&self.data_dir, &change_id)?;
                Ok("用户级 PATH 已还原。请重新打开已有终端。".into())
            }
            Action::RestorePolicy(change_id) => {
                self.update_operation(app, id, |o| {
                    o.phase = "applying".into();
                    o.cancellable = false;
                });
                policy::restore_policy(&self.data_dir, &change_id)?;
                Ok("管理器登记的策略字段已还原。".into())
            }
        }
    }

    fn recheck_installation(&self, expected: Option<&Installation>) -> Result<(), String> {
        let discovery = self.discovery(true)?;
        match expected {
            Some(expected) => {
                let actual = discovery
                    .installations
                    .iter()
                    .find(|i| i.id == expected.id)
                    .ok_or("安装实例已被外部更改，请重新生成计划")?;
                if actual.fingerprint != expected.fingerprint
                    || actual.version != expected.version
                    || actual.scope != expected.scope
                    || actual.source != expected.source
                    || actual.npm_path != expected.npm_path
                    || actual.prefix != expected.prefix
                {
                    return Err("安装路径、版本或来源已发生变化，请重新生成计划".into());
                }
            }
            None => {
                validate_native_destination(&platform::user_home()?, true)?;
                if discovery
                    .installations
                    .iter()
                    .any(|i| i.source == Source::Native)
                {
                    return Err("已出现新的原生安装，请重新生成计划".into());
                }
            }
        }
        Ok(())
    }

    async fn wait_for_sessions(
        &self,
        app: &tauri::AppHandle,
        id: &str,
        installation: Option<&Installation>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let install = installation.cloned();
            let running =
                tokio::task::spawn_blocking(move || discovery::running_for(install.as_ref()))
                    .await
                    .map_err(|e| e.to_string())??;
            if running.is_empty() {
                return Ok(());
            }
            self.update_operation(app, id, |o| {
                o.phase = "waiting".into();
                o.cancellable = true;
                o.result = Some(format!(
                    "等待 {} 个 Claude Code 进程结束，完成会话后将继续。",
                    running.len()
                ));
            });
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    async fn install(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        id: &str,
        installation: Option<Installation>,
        manifest: releases::VerifiedManifest,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<String, String> {
        self.wait_for_sessions(app, id, installation.as_ref(), cancelled)
            .await?;
        self.recheck_installation(installation.as_ref())?;
        let version = manifest.manifest.version.clone();
        self.check_current_policy(&version)?;
        let source = installation
            .as_ref()
            .map(|i| i.source.clone())
            .unwrap_or(Source::Native);
        let command = match source {
            Source::Native => {
                let executable = {
                    self.update_operation(app, id, |o| {
                        o.phase = "downloading".into();
                        o.cancellable = true;
                        o.result = None;
                    });
                    let client = proxy::reqwest_client(&self.data_dir)?;
                    // Bootstrap follows the official installer's sequence: run the
                    // latest signed installer binary with the requested target.
                    let bootstrap_version = releases::channel(&client, "latest").await?;
                    let bootstrap = releases::manifest(&client, &bootstrap_version).await?;
                    let mut last_event = Instant::now() - Duration::from_secs(1);
                    releases::download_binary(
                        &client,
                        &bootstrap,
                        &platform::native_architecture(),
                        &self.data_dir.join("cache"),
                        cancelled,
                        |downloaded, total| {
                            if last_event.elapsed() >= Duration::from_millis(500)
                                || downloaded == total
                            {
                                self.update_operation(app, id, |o| {
                                    o.progress = Some(DownloadProgress {
                                        downloaded,
                                        total: Some(total),
                                    });
                                });
                                last_event = Instant::now();
                            }
                        },
                    )
                    .await?
                };
                if cancelled.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                self.recheck_installation(installation.as_ref())?;
                let mut args = vec!["install".into(), version.clone()];
                if self.operation(id)?.kind == "reinstall" {
                    let help = process::capture(
                        process::executable_command(
                            &executable,
                            &["install".into(), "--help".into()],
                        ),
                        Duration::from_secs(15),
                    )?;
                    if help.stdout.split_whitespace().any(|word| word == "--force") {
                        args.push("--force".into());
                    }
                }
                process::executable_command(&executable, &args)
            }
            Source::Npm => {
                let install = installation.as_ref().ok_or("npm 实例缺失")?;
                let npm = install.npm_path.as_ref().ok_or("npm 路径缺失")?;
                let prefix = install.prefix.as_ref().ok_or("npm prefix 缺失")?;
                process::executable_command(
                    Path::new(npm),
                    &[
                        "install".into(),
                        "-g".into(),
                        format!("@anthropic-ai/claude-code@{version}"),
                        "--prefix".into(),
                        prefix.clone(),
                    ],
                )
            }
            Source::Winget => {
                verify_winget_version(&version)?;
                let winget = process::resolve("winget.exe").ok_or("WinGet 不可用")?;
                let mut command = Command::new(winget);
                command.args([
                    "upgrade",
                    "--id",
                    "Anthropic.ClaudeCode",
                    "--exact",
                    "--source",
                    "winget",
                    "--scope",
                    "user",
                    "--version",
                    &version,
                    "--silent",
                    "--disable-interactivity",
                ]);
                command
            }
            Source::External => return Err("此实例由其他应用管理".into()),
        };
        self.wait_for_sessions(app, id, installation.as_ref(), cancelled)
            .await?;
        self.recheck_installation(installation.as_ref())?;
        self.check_current_policy(&version)?;
        self.ensure_writable()?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        self.update_operation(app, id, |o| {
            o.phase = "installing".into();
            o.cancellable = false;
            o.progress = None;
            o.result = None;
        });
        let manager = self.clone();
        let app_handle = app.clone();
        let operation_id = id.to_string();
        let exit_code = tokio::task::spawn_blocking(move || {
            manager.run_installer(&app_handle, &operation_id, command)
        })
        .await
        .map_err(|e| e.to_string())??;
        self.update_operation(app, id, |o| {
            o.phase = "verifying".into();
        });
        let manager = self.clone();
        let expected_path = installation.as_ref().map(|i| i.path.clone());
        let verify_app = app.clone();
        let verify_id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let after = manager.discovery(true)?;
            let actual = after.installations.iter().find(|i| {
                i.source == source && i.scope == "user" && (source == Source::Winget || expected_path.as_ref().is_none_or(|p| process::equivalent(&i.path, p)))
            });
            let Some(actual) = actual else {
                return Err(format!("安装器退出码 {exit_code}，未发现目标安装。请检查日志并使用安装入口修复。"));
            };
            let reported = process::version(Path::new(&actual.path));
            if reported.as_deref() != Some(&version) {
                return Err(format!("安装器退出码 {exit_code}，当前实际版本为 {}，目标为 {version}。请重新检测后重试或重装旧版本。", reported.unwrap_or_else(|| "不可用".into())));
            }
            if source == Source::Native {
                let (_, artifact) = releases::windows_artifact(&manifest.manifest, &platform::native_architecture())?;
                releases::verify_digest(Path::new(&actual.path), artifact)?;
                releases::verify_authenticode(Path::new(&actual.path))?;
            }
            let mut message = format!("已安装并验证 Claude Code {version}。");
            let report = manager.doctor_report(actual).unwrap_or_else(|error| error);
            manager.update_operation(&verify_app, &verify_id, |o| o.messages.push(report));
            if !actual.active { message.push_str(" 当前 PowerShell 命中其他路径，请在环境页处理 PATH。"); }
            if exit_code != 0 { message.push_str(&format!(" 安装器返回 {exit_code}，目标版本已通过独立验证。")); }
            Ok(message)
        }).await.map_err(|e| e.to_string())?
    }

    fn run_installer(
        &self,
        app: &tauri::AppHandle,
        id: &str,
        mut command: Command,
    ) -> Result<i32, String> {
        process::hidden(&mut command);
        platform::apply_fresh_user_path(&mut command)?;
        proxy::apply_command_proxy(&self.data_dir, &mut command)?;
        // Disable the launched helper's background update during this operation.
        command.env("DISABLE_AUTOUPDATER", "1");
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法启动安装器: {e}"))?;
        self.update_operation(app, id, |o| {
            o.child_pid = Some(child.id());
        });
        let (send, receive) = std::sync::mpsc::sync_channel::<String>(200);
        if let Some(stdout) = child.stdout.take() {
            let send = send.clone();
            std::thread::spawn(move || {
                process::stream_lines(stdout, send);
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let send = send.clone();
            std::thread::spawn(move || {
                process::stream_lines(stderr, send);
            });
        }
        drop(send);
        let started = Instant::now();
        let mut warned = false;
        loop {
            let lines = receive
                .try_iter()
                .take(100)
                .map(|line| storage::redact(&line))
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>();
            if !lines.is_empty() {
                self.update_operation(app, id, |o| {
                    o.messages.extend(lines);
                    if o.messages.len() > 200 {
                        o.messages.drain(..o.messages.len() - 200);
                    }
                });
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return Ok(status.code().unwrap_or(-1));
            }
            if !warned && started.elapsed() > Duration::from_secs(600) {
                warned = true;
                self.update_operation(app, id, |o| {
                    o.messages
                        .push("安装器仍在运行，已超过 10 分钟。请检查网络或其他安装进程。".into());
                });
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    pub fn changes(&self) -> Result<Vec<ChangeEntry>, String> {
        let mut entries = Vec::new();
        for change in path_edit::list_changes(&self.data_dir)? {
            let current = path_edit::read_user_path()?;
            let display = |value: &path_edit::RawPathValue| {
                if value.exists {
                    value.value.clone()
                } else {
                    "(不存在)".into()
                }
            };
            let comparison = vec![
                ChangeComparison {
                    name: "用户 PATH".into(),
                    before: display(&change.before),
                    written: display(&change.after),
                    current: display(&current),
                },
                ChangeComparison {
                    name: "注册表值类型".into(),
                    before: change.before.value_type.to_string(),
                    written: change.after.value_type.to_string(),
                    current: current.value_type.to_string(),
                },
            ];
            let can_restore = change.state != "restored"
                && path_edit::preview_restore(&self.data_dir, &change.id).is_ok();
            entries.push(ChangeEntry {
                id: change.id,
                kind: "path".into(),
                description: change.reason,
                created_at: timestamp(change.created_at),
                restored: change.state == "restored",
                can_restore,
                detail: if can_restore {
                    "可按登记原值还原".into()
                } else {
                    "已还原或当前值发生变化，请查看差异".into()
                },
                comparison,
            });
        }
        for change in policy::list_changes(&self.data_dir)? {
            let current = storage::read_bytes(Path::new(&change.settings_path))
                .and_then(|bytes| {
                    bytes
                        .map(|b| {
                            serde_json::from_slice::<serde_json::Value>(&b)
                                .map_err(|_| "当前设置 JSON 无法解析".to_string())
                        })
                        .transpose()
                })
                .map(|value| value.unwrap_or_default());
            let comparison = change
                .fields
                .iter()
                .map(|field| ChangeComparison {
                    name: field.pointer.clone(),
                    before: field
                        .before
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "(未设置)".into()),
                    written: field
                        .after
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "(未设置)".into()),
                    current: match current.as_ref() {
                        Ok(value) => value
                            .pointer(&field.pointer)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "(未设置)".into()),
                        Err(error) => format!("无法读取：{error}"),
                    },
                })
                .collect();
            let can_restore = change.state != "restored"
                && policy::preview_restore(&self.data_dir, &change.id).is_ok();
            entries.push(ChangeEntry {
                id: change.id,
                kind: "policy".into(),
                description: format!("更新策略 {}", change.mode),
                created_at: timestamp(change.created_at),
                restored: change.state == "restored",
                can_restore,
                detail: if can_restore {
                    "只还原管理器改动的配置字段".into()
                } else {
                    "已还原或字段被再次修改".into()
                },
                comparison,
            });
        }
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(entries)
    }

    pub fn diagnostics(&self) -> Result<String, String> {
        let snapshot = self.snapshot()?;
        let mut text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
        for installation in snapshot
            .installations
            .iter()
            .filter(|i| i.active && i.source != Source::External)
        {
            text.push_str("\n\nClaude doctor (current manager environment):\n");
            text.push_str(
                &self
                    .doctor_report(installation)
                    .unwrap_or_else(|error| error),
            );
        }
        Ok(storage::redact(&text))
    }

    fn doctor_report(&self, installation: &Installation) -> Result<String, String> {
        let mut command =
            process::executable_command(Path::new(&installation.path), &["doctor".into()]);
        platform::apply_fresh_user_path(&mut command)?;
        proxy::apply_command_proxy(&self.data_dir, &mut command)?;
        command.env("DISABLE_AUTOUPDATER", "1");
        let output = process::capture(command, Duration::from_secs(30))?;
        let text = format!("exit={}\n{}\n{}", output.code, output.stdout, output.stderr);
        Ok(storage::redact(&text).chars().take(12_000).collect())
    }

    fn check_current_policy(&self, version: &str) -> Result<(), String> {
        let policy = policy::read_policy(&self.data_dir)?;
        if policy.disabled_updates {
            return Err("DISABLE_UPDATES 当前禁止安装和更新".into());
        }
        if let Some(minimum) = policy.minimum_version {
            let minimum = releases::validate_version(&minimum)?;
            if semver::Version::parse(version).map_err(|e| e.to_string())?
                < semver::Version::parse(&minimum).map_err(|e| e.to_string())?
            {
                return Err(format!(
                    "目标版本低于当前生效 minimumVersion {minimum}，请重新生成计划"
                ));
            }
        }
        Ok(())
    }

    fn updater(&self, app: &tauri::AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
        let mut builder = app
            .updater_builder()
            .pubkey(include_str!("../resources/manager-updater.pub").trim())
            .timeout(Duration::from_secs(1800));
        if proxy::load_proxy(&self.data_dir)?.mode == "direct" {
            builder = builder.no_proxy();
        } else if let Some(url) = proxy::effective_proxy_url(&self.data_dir)? {
            builder = builder.proxy(url);
        }
        builder
            .build()
            .map_err(|_| "管理器更新组件初始化失败".into())
    }

    pub async fn manager_update(&self, app: &tauri::AppHandle) -> Result<ManagerUpdate, String> {
        let checked = self.updater(app)?.check().await;
        let (version, available, message) = match checked {
            Ok(Some(update)) => (
                Some(update.version),
                true,
                "发现新版本。安装前将验证内置发布公钥对应的更新签名。".into(),
            ),
            Ok(None) => (None, false, "当前版本已是发布通道的最新版本。".into()),
            Err(_) => (
                None,
                false,
                "尚无可用的签名更新清单，或网络检查失败。可前往 GitHub Releases 查看预览版。"
                    .into(),
            ),
        };
        Ok(ManagerUpdate {
            current_version: env!("CARGO_PKG_VERSION").into(),
            latest_version: version,
            available,
            download_url: "https://github.com/Yeyangchen/claude-code-manager/releases".into(),
            signed_updater_ready: true,
            message,
        })
    }

    /// The UI confirms a concrete version. A changed feed must be reviewed again.
    pub fn install_manager_update(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        expected_version: String,
    ) -> Result<Operation, String> {
        self.ensure_writable()?;
        let version =
            semver::Version::parse(&expected_version).map_err(|_| "管理器目标版本无效")?;
        if version
            <= semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?
        {
            return Err("管理器更新必须高于当前版本".into());
        }
        let lock = self.operation_lock()?;
        self.owns_task.store(true, Ordering::Relaxed);
        let operation = Operation {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "manager-update".into(),
            title: "更新 Claude Code Manager".into(),
            phase: "preparing".into(),
            status: "running".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            messages: Vec::new(),
            cancellable: false,
            progress: None,
            result: None,
            installation_id: None,
            target_version: Some(expected_version.clone()),
            child_pid: None,
        };
        {
            let mut history = self.history.lock().map_err(|_| "任务状态锁异常")?;
            history.operations.insert(0, operation.clone());
            history.operations.truncate(100);
        }
        if let Err(error) = self.persist_history() {
            self.owns_task.store(false, Ordering::Relaxed);
            if let Ok(mut history) = self.history.lock() {
                history.operations.retain(|o| o.id != operation.id);
            }
            return Err(error);
        }
        let manager = self.clone();
        let id = operation.id.clone();
        tauri::async_runtime::spawn(async move {
            let _lock = lock;
            let result: Result<(), String> = async {
                let update = manager
                    .updater(&app)?
                    .check()
                    .await
                    .map_err(|_| "管理器更新检查失败")?
                    .ok_or("签名更新已不可用")?;
                if update.version != expected_version {
                    return Err("发布版本已改变，请重新检查并确认".into());
                }
                if update.download_url.scheme() != "https"
                    || update.download_url.host_str() != Some("github.com")
                    || !update
                        .download_url
                        .path()
                        .starts_with("/Yeyangchen/claude-code-manager/releases/download/")
                {
                    return Err("管理器更新文件不属于可信发布地址".into());
                }
                manager.update_operation(&app, &id, |o| o.phase = "downloading".into());
                let mut downloaded = 0u64;
                let mut last = Instant::now() - Duration::from_secs(1);
                let bytes = update
                    .download(
                        |chunk, total| {
                            downloaded += chunk as u64;
                            if last.elapsed() >= Duration::from_millis(500)
                                || Some(downloaded) == total
                            {
                                manager.update_operation(&app, &id, |o| {
                                    o.progress = Some(DownloadProgress { downloaded, total })
                                });
                                last = Instant::now();
                            }
                        },
                        || {},
                    )
                    .await
                    .map_err(|_| "更新包下载或签名验证失败，未执行安装")?;
                manager.update_operation(&app, &id, |o| {
                    o.phase = "installing".into();
                    o.result =
                        Some("更新签名已通过。应用将关闭并启动安装程序，下次启动复核版本。".into());
                });
                update
                    .install(bytes)
                    .map_err(|_| "无法启动管理器更新安装程序")?;
                Ok(())
            }
            .await;
            if let Err(message) = result {
                manager.update_operation(&app, &id, |o| {
                    o.status = "failed".into();
                    o.phase = "completed".into();
                    o.finished_at = Some(chrono::Utc::now().to_rfc3339());
                    o.result = Some(message);
                });
            }
            manager.owns_task.store(false, Ordering::Relaxed);
        });
        Ok(operation)
    }

    pub fn prevent_close_while_active(&self, app: &tauri::AppHandle) -> bool {
        if !self.owns_task.load(Ordering::Relaxed) {
            return false;
        }
        let active = self.history.lock().ok().and_then(|h| {
            h.operations
                .iter()
                .find(|o| o.status == "running")
                .map(|o| o.id.clone())
        });
        if let Some(id) = active {
            self.update_operation(app, &id, |o| {
                o.messages
                    .push("任务正在后台执行，请等待完成；等待会话阶段可以取消任务。".into());
                o.messages.truncate(200);
            });
            return true;
        }
        false
    }

    fn refresh_external_history(&self) -> Result<(), String> {
        if self.owns_task.load(Ordering::Relaxed) {
            return Ok(());
        }
        let loaded = storage::read_json::<TaskHistory>(&self.data_dir.join("tasks.json"))?;
        if loaded.read_only {
            self.read_only.store(true, Ordering::Relaxed);
        }
        *self.history.lock().map_err(|_| "任务状态锁异常")? = loaded.value;
        Ok(())
    }
}

fn timestamp(milliseconds: u64) -> String {
    chrono::DateTime::from_timestamp_millis(milliseconds.min(i64::MAX as u64) as i64)
        .map(|t| t.to_rfc3339())
        .unwrap_or_default()
}

fn manual_command(installation: &Installation, uninstall: bool, target: Option<&str>) -> String {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    match installation.source {
        Source::Npm => {
            let npm = quote(installation.npm_path.as_deref().unwrap_or("npm.cmd"));
            let prefix = quote(installation.prefix.as_deref().unwrap_or_default());
            if uninstall {
                format!("& {npm} uninstall -g @anthropic-ai/claude-code --prefix {prefix}")
            } else {
                format!(
                    "& {npm} install -g @anthropic-ai/claude-code@{} --prefix {prefix}",
                    target.unwrap_or("latest")
                )
            }
        }
        Source::Winget => {
            let scope = if installation.scope == "machine" {
                "machine"
            } else {
                "user"
            };
            if uninstall {
                format!("winget uninstall --id Anthropic.ClaudeCode --exact --scope {scope}")
            } else {
                format!("winget upgrade --id Anthropic.ClaudeCode --exact --source winget --scope {scope} --version {}", target.unwrap_or("latest"))
            }
        }
        _ => "请通过原安装来源管理此实例".into(),
    }
}

fn hold_drift(
    held: &std::collections::BTreeMap<String, String>,
    installations: &[Installation],
) -> Vec<String> {
    held.iter()
        .filter_map(|(id, expected)| {
            let actual = installations.iter().find(|i| &i.id == id);
            if actual.and_then(|i| i.version.as_ref()) == Some(expected) {
                return None;
            }
            Some(format!(
                "原保持版本 {expected}，当前 {}（{}）",
                actual
                    .and_then(|i| i.version.as_deref())
                    .unwrap_or("实例缺失或版本不可用"),
                actual.map(|i| i.path.as_str()).unwrap_or(id)
            ))
        })
        .collect()
}

fn uninstall_native(installation: &Installation) -> Result<(), String> {
    let home = platform::user_home()?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let launcher = home.join(".local/bin/claude.exe");
    let versions = home.join(".local/share/claude");
    validate_native_destination(&home, false)?;
    let parent = launcher.parent().ok_or("原生安装路径缺少父目录")?;
    if parent.canonicalize().map_err(|e| e.to_string())? != parent {
        return Err("原生启动目录包含重解析路径，停止自动卸载".into());
    }
    if !process::equivalent(&installation.path, &launcher.to_string_lossy()) {
        return Err("原生安装路径归属不匹配".into());
    }
    if versions.exists() {
        let canonical = versions.canonicalize().map_err(|e| e.to_string())?;
        if canonical != versions {
            return Err("原生版本目录指向允许范围之外".into());
        }
    }
    if launcher.exists() {
        std::fs::remove_file(&launcher).map_err(|e| format!("移除启动文件失败: {e}"))?;
    }
    if versions.exists() {
        std::fs::remove_dir_all(&versions)
            .map_err(|e| format!("启动文件已移除，版本目录清理失败: {e}"))?;
    }
    Ok(())
}

fn validate_native_destination(home: &Path, require_empty: bool) -> Result<(), String> {
    let home = home.canonicalize().map_err(|e| e.to_string())?;
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_lowercase()
    };
    for relative in [
        ".local",
        ".local/bin",
        ".local/share",
        ".local/share/claude",
    ] {
        let path = home.join(relative);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                let actual = path
                    .canonicalize()
                    .map_err(|_| "原生安装目录包含失效的重解析路径")?;
                if normalize(&actual) != normalize(&path) {
                    return Err("原生安装目录重定向到预期用户目录之外，请手动核查".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法核查原生安装目录：{error}")),
        }
    }
    if require_empty {
        match std::fs::symlink_metadata(home.join(".local/bin/claude.exe")) {
            Ok(_) => {
                return Err("默认启动文件已存在但未确认归属，请选择已识别实例重装或手动核查".into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法核查默认启动文件：{error}")),
        }
    }
    Ok(())
}

async fn verify_npm_version(client: &reqwest::Client, version: &str) -> Result<(), String> {
    let bytes = releases::metadata(
        client,
        &format!("https://registry.npmjs.org/@anthropic-ai/claude-code/{version}"),
    )
    .await?;
    let package: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "npm 元数据格式无效")?;
    if package["name"] != "@anthropic-ai/claude-code" || package["version"] != version {
        return Err("npm 来源尚未提供目标版本".into());
    }
    Ok(())
}

fn verify_winget_version(version: &str) -> Result<(), String> {
    let winget = process::resolve("winget.exe").ok_or("WinGet 不可用")?;
    let mut command = Command::new(winget);
    command.args([
        "show",
        "--id",
        "Anthropic.ClaudeCode",
        "--exact",
        "--source",
        "winget",
        "--versions",
        "--disable-interactivity",
    ]);
    let output = process::capture(command, Duration::from_secs(60))?;
    if output.code != 0 || !output.stdout.split_whitespace().any(|v| v == version) {
        return Err("WinGet 来源尚未提供目标版本，请稍后检查".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_installation() -> Installation {
        Installation {
            id: "fixture".into(),
            source: Source::Npm,
            path: "C:/isolated/claude.cmd".into(),
            version: Some("2.1.100".into()),
            scope: "machine".into(),
            active: true,
            capabilities: Capabilities {
                upgrade: false,
                install_version: false,
                uninstall: false,
            },
            detail: String::new(),
            manual_command: None,
            npm_path: Some("C:/Node/npm.cmd".into()),
            prefix: Some("C:/Owner's tools".into()),
            fingerprint: "fixture".into(),
        }
    }

    #[test]
    fn operation_lock_excludes_another_manager_and_releases_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let first = Manager::new(directory.path().to_path_buf()).unwrap();
        let second = Manager::new(directory.path().to_path_buf()).unwrap();
        let guard = first.operation_lock().unwrap();
        assert!(second.operation_lock().is_err());
        drop(guard);
        assert!(second.operation_lock().is_ok());
    }

    #[test]
    fn opening_second_manager_does_not_recover_or_overwrite_live_task() {
        let directory = tempfile::tempdir().unwrap();
        let manager = Manager::new(directory.path().to_path_buf()).unwrap();
        let guard = manager.operation_lock().unwrap();
        let operation = Operation {
            id: "live-task".into(),
            kind: "update".into(),
            title: "fixture".into(),
            phase: "installing".into(),
            status: "running".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            messages: vec![],
            cancellable: false,
            progress: None,
            result: None,
            installation_id: None,
            target_version: Some("2.1.326".into()),
            child_pid: None,
        };
        manager.history.lock().unwrap().operations.push(operation);
        manager.persist_history().unwrap();
        let bytes = std::fs::read(directory.path().join("tasks.json")).unwrap();
        let observer = Manager::new(directory.path().to_path_buf()).unwrap();
        assert_eq!(observer.operation("live-task").unwrap().status, "running");
        assert_eq!(
            std::fs::read(directory.path().join("tasks.json")).unwrap(),
            bytes
        );
        drop(guard);
    }

    #[test]
    fn future_proxy_schema_turns_all_writes_read_only_without_overwriting() {
        let directory = tempfile::tempdir().unwrap();
        let manager = Manager::new(directory.path().to_path_buf()).unwrap();
        let path = directory.path().join("proxy.json");
        let bytes = br#"{"schemaVersion":999,"data":{"secret":"preserve"}}"#;
        std::fs::write(&path, bytes).unwrap();
        assert!(manager.operation_lock().is_err());
        assert!(manager.read_only.load(Ordering::Relaxed));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn manual_actions_bind_source_scope_version_and_quote_prefix() {
        let installation = fixture_installation();
        let upgrade = manual_command(&installation, false, Some("2.1.200"));
        assert!(upgrade.contains("@2.1.200"));
        assert!(upgrade.contains("'C:/Owner''s tools'"));
        let uninstall = manual_command(&installation, true, None);
        assert!(uninstall.contains(" uninstall "));
        assert!(!uninstall.contains("@latest"));
    }

    #[test]
    fn hold_drift_reports_changed_missing_and_unknown_versions() {
        let mut installation = fixture_installation();
        let baseline = [(installation.id.clone(), "2.1.100".into())]
            .into_iter()
            .collect();
        assert!(hold_drift(&baseline, &[installation.clone()]).is_empty());
        installation.version = Some("2.1.200".into());
        assert!(hold_drift(&baseline, &[installation.clone()])[0].contains("2.1.200"));
        installation.version = None;
        assert_eq!(hold_drift(&baseline, &[installation]).len(), 1);
        assert_eq!(hold_drift(&baseline, &[]).len(), 1);
    }

    #[test]
    fn fresh_native_install_does_not_overwrite_unknown_launcher() {
        let directory = tempfile::tempdir().unwrap();
        validate_native_destination(directory.path(), true).unwrap();
        let launcher = directory.path().join(".local/bin/claude.exe");
        std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        std::fs::write(&launcher, b"unknown owner").unwrap();
        assert!(validate_native_destination(directory.path(), true).is_err());
        assert_eq!(std::fs::read(launcher).unwrap(), b"unknown owner");
    }
}
