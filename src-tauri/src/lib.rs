mod discovery;
mod domain;
mod manager;
mod path_edit;
mod platform;
mod policy;
mod process;
mod proxy;
mod releases;
mod storage;

use domain::*;
use manager::Manager;
use std::sync::Arc;
use tauri::{Manager as TauriManager, State};

#[tauri::command]
async fn get_snapshot(state: State<'_, Arc<Manager>>) -> AppResult<Snapshot> {
    let manager = state.inner().clone();
    tokio::task::spawn_blocking(move || manager.snapshot())
        .await
        .map_err(|e| AppError::new("worker_failed", e.to_string()))?
        .map_err(Into::into)
}

#[tauri::command]
async fn check_releases(
    state: State<'_, Arc<Manager>>,
    force: Option<bool>,
) -> AppResult<ReleaseCatalog> {
    let client = proxy::reqwest_client(&state.data_dir)?;
    releases::catalog(&client, &state.data_dir, force.unwrap_or(false))
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn create_plan(state: State<'_, Arc<Manager>>, request: PlanRequest) -> AppResult<Plan> {
    state.inner().create_plan(request).await.map_err(Into::into)
}

#[tauri::command]
fn execute_plan(
    app: tauri::AppHandle,
    state: State<'_, Arc<Manager>>,
    plan_id: String,
    allow_checksum_only: Option<bool>,
) -> AppResult<Operation> {
    state
        .inner()
        .execute(app, &plan_id, allow_checksum_only.unwrap_or(false))
        .map_err(Into::into)
}

#[tauri::command]
fn get_operation(state: State<'_, Arc<Manager>>, operation_id: String) -> AppResult<Operation> {
    state.operation(&operation_id).map_err(Into::into)
}

#[tauri::command]
fn cancel_operation(state: State<'_, Arc<Manager>>, operation_id: String) -> AppResult<Operation> {
    state.cancel(&operation_id).map_err(Into::into)
}

#[tauri::command]
fn save_settings(
    state: State<'_, Arc<Manager>>,
    settings: Settings,
    proxy_username: Option<String>,
    proxy_password: Option<String>,
) -> AppResult<Settings> {
    state
        .save_settings(settings, proxy_username, proxy_password)
        .map_err(Into::into)
}

#[tauri::command]
fn get_changes(state: State<'_, Arc<Manager>>) -> AppResult<Vec<ChangeEntry>> {
    state.changes().map_err(Into::into)
}

#[tauri::command]
async fn preview_diagnostics(state: State<'_, Arc<Manager>>) -> AppResult<String> {
    let manager = state.inner().clone();
    tokio::task::spawn_blocking(move || manager.diagnostics())
        .await
        .map_err(|e| AppError::new("worker_failed", e.to_string()))?
        .map_err(Into::into)
}

#[tauri::command]
async fn export_diagnostics(
    app: tauri::AppHandle,
    state: State<'_, Arc<Manager>>,
) -> AppResult<Option<serde_json::Value>> {
    use tauri_plugin_dialog::DialogExt;
    let manager = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<Option<serde_json::Value>, String> {
        let text = manager.diagnostics()?;
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name("claude-code-manager-diagnostics.txt")
            .add_filter("Text", &["txt"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| "请选择本地文件路径")?;
        storage::atomic_write(&path, text.as_bytes())?;
        Ok(Some(serde_json::json!({ "path": path.to_string_lossy() })))
    })
    .await
    .map_err(|e| AppError::new("worker_failed", e.to_string()))?
    .map_err(AppError::from)
}

#[tauri::command]
fn open_url(url: String) -> AppResult<()> {
    let parsed = url::Url::parse(&url).map_err(|_| AppError::new("invalid_url", "链接格式无效"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || url.len() > 4096
    {
        return Err(AppError::new("invalid_url", "只允许 HTTPS 网页链接"));
    }
    let mut command = process::ps("Start-Process -FilePath $env:CCM_OPEN_URL");
    command.env("CCM_OPEN_URL", parsed.as_str());
    let output = process::capture(command, std::time::Duration::from_secs(15))?;
    if output.code != 0 {
        return Err(AppError::new("open_failed", "系统浏览器打开失败"));
    }
    Ok(())
}

#[tauri::command]
async fn check_manager_update(
    app: tauri::AppHandle,
    state: State<'_, Arc<Manager>>,
) -> AppResult<ManagerUpdate> {
    state.manager_update(&app).await.map_err(Into::into)
}

#[tauri::command]
fn install_manager_update(
    app: tauri::AppHandle,
    state: State<'_, Arc<Manager>>,
    expected_version: String,
) -> AppResult<Operation> {
    state
        .inner()
        .install_manager_update(app, expected_version)
        .map_err(Into::into)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(manager) = window.try_state::<Arc<Manager>>() {
                    if manager.prevent_close_while_active(window.app_handle()) {
                        api.prevent_close();
                    }
                }
            }
        })
        .setup(|app| {
            let path = app.path().app_data_dir()?;
            let manager = Manager::new(path).map_err(std::io::Error::other)?;
            app.manage(Arc::new(manager));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            check_releases,
            create_plan,
            execute_plan,
            get_operation,
            cancel_operation,
            save_settings,
            get_changes,
            preview_diagnostics,
            export_diagnostics,
            open_url,
            check_manager_update,
            install_manager_update
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start Claude Code Manager");
}
