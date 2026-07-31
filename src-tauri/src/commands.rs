//! Superficie de comandos IPC expuesta al frontend (`invoke("...")` desde
//! React). Cada comando es una función fina que delega en los módulos de
//! dominio (`config`, `java`, `minecraft`, `accounts`) — la lógica de verdad
//! vive ahí, no aquí.

use crate::accounts::{self, Account};
use crate::config::{self, LauncherConfig};
use crate::download::DownloadProgress;
use crate::java::{self, JavaInstallation};
use crate::minecraft::{install as mc_install, instance as mc_instance, launch as mc_launch, manifest, GamePaths};
use crate::state::{AppState, RunningGame};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn game_paths(app: &AppHandle, cfg: &LauncherConfig) -> GamePaths {
    let app_data = config::app_data_dir(app);
    let instances = config::instances_dir(app, cfg);
    let cache = config::cache_dir(app);
    GamePaths::new(&app_data, &instances, &cache)
}

// ---------------------------------------------------------------- Config --

#[tauri::command]
pub fn get_config(state: State<AppState>) -> LauncherConfig {
    state.config.read().unwrap().clone()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: LauncherConfig) -> Result<LauncherConfig, String> {
    config::save(&app, &config).map_err(to_err)?;
    *state.config.write().unwrap() = config.clone();
    Ok(config)
}

#[tauri::command]
pub fn reset_config(app: AppHandle, state: State<AppState>) -> Result<LauncherConfig, String> {
    let defaults = config::reset_to_defaults(&app).map_err(to_err)?;
    *state.config.write().unwrap() = defaults.clone();
    Ok(defaults)
}

#[tauri::command]
pub fn system_memory_mb() -> u64 {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    sys.total_memory() / (1024 * 1024)
}

// ------------------------------------------------------------------ Java --

#[tauri::command]
pub async fn detect_java(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<JavaInstallation>, String> {
    let cfg = state.config.read().unwrap().clone();
    let managed_dir = config::java_runtime_dir(&app, &cfg);
    Ok(java::detect_installations(&managed_dir).await)
}

#[tauri::command]
pub async fn install_java(app: AppHandle, state: State<'_, AppState>, major: u32) -> Result<JavaInstallation, String> {
    let cfg = state.config.read().unwrap().clone();
    let managed_dir = config::java_runtime_dir(&app, &cfg);
    let http = state.http.clone();

    let app_for_progress = app.clone();
    let on_progress: crate::download::ProgressCallback = Arc::new(move |p: DownloadProgress| {
        let _ = app_for_progress.emit("java-install-progress", p);
    });

    let java_path = java::adoptium::install(&http, major, &managed_dir, on_progress)
        .await
        .map_err(to_err)?;

    java::probe(&java_path, &managed_dir)
        .await
        .ok_or_else(|| "no se pudo verificar el runtime recién instalado".to_string())
}

// ------------------------------------------------------------- Versiones --

#[tauri::command]
pub async fn list_minecraft_versions(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<manifest::VersionEntry>, String> {
    let cfg = state.config.read().unwrap().clone();
    let paths = game_paths(&app, &cfg);
    let version_manifest = manifest::fetch_manifest(&state.http, &paths.cache).await.map_err(to_err)?;
    Ok(manifest::visible_versions(&version_manifest, cfg.show_snapshots)
        .into_iter()
        .cloned()
        .collect())
}

// -------------------------------------------------------------- Instancias --

#[tauri::command]
pub async fn list_instances(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<mc_instance::Instance>, String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    mc_instance::list(&instances_dir).await.map_err(to_err)
}

#[tauri::command]
pub async fn create_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    minecraft_version: String,
) -> Result<mc_instance::Instance, String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    mc_instance::create(&instances_dir, &name, &minecraft_version, mc_instance::LoaderKind::Vanilla)
        .await
        .map_err(to_err)
}

#[tauri::command]
pub async fn update_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance: mc_instance::Instance,
) -> Result<(), String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    mc_instance::save(&instances_dir, &instance).await.map_err(to_err)
}

#[tauri::command]
pub async fn delete_instance(app: AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    mc_instance::delete(&instances_dir, &instance_id).await.map_err(to_err)
}

#[tauri::command]
pub fn is_version_installed(app: AppHandle, state: State<AppState>, minecraft_version: String) -> bool {
    let cfg = state.config.read().unwrap().clone();
    let paths = game_paths(&app, &cfg);
    mc_install::is_installed(&paths, &minecraft_version)
}

#[tauri::command]
pub async fn install_instance(app: AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    let instance = mc_instance::load(&instances_dir, &instance_id).await.map_err(to_err)?;
    let paths = game_paths(&app, &cfg);

    let app_for_progress = app.clone();
    let on_progress: crate::download::ProgressCallback = Arc::new(move |p: DownloadProgress| {
        let _ = app_for_progress.emit("install-progress", p);
    });

    mc_install::install_vanilla(&state.http, &state.downloads, &paths, &instance.minecraft_version, on_progress)
        .await
        .map_err(to_err)?;
    Ok(())
}

// ----------------------------------------------------------------- Lanzar --

#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    account_id: String,
) -> Result<(), String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    let instance = mc_instance::load(&instances_dir, &instance_id).await.map_err(to_err)?;
    let paths = game_paths(&app, &cfg);

    let detail = mc_install::load_version_detail(&state.http, &paths, &instance.minecraft_version, &paths.cache)
        .await
        .map_err(to_err)?;

    let required_major = detail.java_version.as_ref().map(|j| j.major_version).unwrap_or(17);
    let managed_dir = config::java_runtime_dir(&app, &cfg);
    let installations = java::detect_installations(&managed_dir).await;
    let java_installation = java::find_best(&installations, required_major).ok_or_else(|| {
        format!("No hay un Java {required_major}+ instalado. Ve a Configuración > Java para instalarlo.")
    })?;

    let app_data = config::app_data_dir(&app);
    let all_accounts = accounts::list_accounts(&app_data).await.map_err(to_err)?;
    let account = all_accounts
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| "cuenta no encontrada".to_string())?;

    let (mut child, log_path) = mc_launch::launch(
        &paths,
        mc_launch::LaunchRequest {
            instance: &instance,
            detail: &detail,
            java_path: &java_installation.path,
            account: &account,
            default_min_ram_mb: cfg.default_min_ram_mb,
            default_max_ram_mb: cfg.default_max_ram_mb,
            launcher_name: cfg.launcher_name.clone(),
            launcher_version: app.package_info().version.to_string(),
        },
    )
    .await
    .map_err(to_err)?;

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(app.clone(), instance_id.clone(), stdout, log_path.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(app.clone(), instance_id.clone(), stderr, log_path.clone());
    }

    let started_at = std::time::Instant::now();
    state
        .running_instances
        .write()
        .unwrap()
        .insert(instance_id.clone(), RunningGame { child, log_path });

    watch_for_exit(app, instances_dir, instance_id, started_at);
    Ok(())
}

#[tauri::command]
pub fn is_instance_running(state: State<AppState>, instance_id: String) -> bool {
    state.running_instances.read().unwrap().contains_key(&instance_id)
}

fn watch_for_exit(app: AppHandle, instances_dir: PathBuf, instance_id: String, started_at: std::time::Instant) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let exited = {
                let state = app.state::<AppState>();
                let mut running = state.running_instances.write().unwrap();
                match running.get_mut(&instance_id) {
                    Some(game) => match game.child.try_wait() {
                        Ok(Some(_status)) => {
                            running.remove(&instance_id);
                            true
                        }
                        Ok(None) => false,
                        Err(_) => {
                            running.remove(&instance_id);
                            true
                        }
                    },
                    None => true,
                }
            };
            if exited {
                let secs = started_at.elapsed().as_secs();
                let _ = mc_instance::record_session(&instances_dir, &instance_id, secs).await;
                let _ = app.emit("instance-exited", &instance_id);
                break;
            }
        }
    });
}

fn spawn_log_reader<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    app: AppHandle,
    instance_id: String,
    reader: R,
    log_path: PathBuf,
) {
    tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let mut log_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
            .ok();

        let mut lines = BufReader::new(reader).lines();
        let event_name = format!("instance-log:{instance_id}");
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(file) = log_file.as_mut() {
                let _ = file.write_all(format!("{line}\n").as_bytes()).await;
            }
            let _ = app.emit(&event_name, &line);
        }
    });
}

// ----------------------------------------------------------------- Mods --

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    pub file_name: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_mods(app: AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<Vec<ModEntry>, String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    let mods_dir = mc_instance::minecraft_dir(&instances_dir, &instance_id).join("mods");
    tokio::fs::create_dir_all(&mods_dir).await.map_err(to_err)?;

    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(&mods_dir).await.map_err(to_err)?;
    while let Some(entry) = read_dir.next_entry().await.map_err(to_err)? {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(base) = name.strip_suffix(".disabled") {
            if base.ends_with(".jar") {
                entries.push(ModEntry { file_name: base.to_string(), enabled: false });
            }
        } else if name.ends_with(".jar") {
            entries.push(ModEntry { file_name: name, enabled: true });
        }
    }
    entries.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(entries)
}

#[tauri::command]
pub async fn toggle_mod(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    enable: bool,
) -> Result<(), String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    let mods_dir = mc_instance::minecraft_dir(&instances_dir, &instance_id).join("mods");

    let (from, to) = if enable {
        (mods_dir.join(format!("{file_name}.disabled")), mods_dir.join(&file_name))
    } else {
        (mods_dir.join(&file_name), mods_dir.join(format!("{file_name}.disabled")))
    };
    tokio::fs::rename(from, to).await.map_err(to_err)
}

// -------------------------------------------------------------- Cuentas --

#[tauri::command]
pub async fn list_accounts(app: AppHandle) -> Result<Vec<Account>, String> {
    accounts::list_accounts(&config::app_data_dir(&app)).await.map_err(to_err)
}

#[tauri::command]
pub async fn add_account(app: AppHandle, username: String) -> Result<Account, String> {
    accounts::add_account(&config::app_data_dir(&app), &username).await.map_err(to_err)
}

#[tauri::command]
pub async fn remove_account(app: AppHandle, account_id: String) -> Result<(), String> {
    accounts::remove_account(&config::app_data_dir(&app), &account_id)
        .await
        .map_err(to_err)
}
