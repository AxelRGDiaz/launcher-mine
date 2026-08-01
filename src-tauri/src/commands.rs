//! Superficie de comandos IPC expuesta al frontend (`invoke("...")` desde
//! React). Cada comando es una función fina que delega en los módulos de
//! dominio (`config`, `java`, `minecraft`, `accounts`) — la lógica de verdad
//! vive ahí, no aquí.

use crate::accounts::{self, Account};
use crate::config::{self, LauncherConfig};
use crate::download::DownloadProgress;
use crate::java::{self, JavaInstallation};
use crate::minecraft::install::VersionDetail;
use crate::minecraft::{
    fabric_like, forge_like, install as mc_install, instance as mc_instance, launch as mc_launch, manifest, optifine,
    GamePaths,
};
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

fn flavor_for(loader: mc_instance::LoaderKind) -> Result<fabric_like::LoaderFlavor, String> {
    match loader {
        mc_instance::LoaderKind::Fabric => Ok(fabric_like::LoaderFlavor::Fabric),
        mc_instance::LoaderKind::Quilt => Ok(fabric_like::LoaderFlavor::Quilt),
        other => Err(format!("{other:?} no es Fabric/Quilt")),
    }
}

fn forge_flavor_for(loader: mc_instance::LoaderKind) -> Result<forge_like::ForgeFlavor, String> {
    match loader {
        mc_instance::LoaderKind::Forge => Ok(forge_like::ForgeFlavor::Forge),
        mc_instance::LoaderKind::NeoForge => Ok(forge_like::ForgeFlavor::NeoForge),
        other => Err(format!("{other:?} no es Forge/NeoForge")),
    }
}

/// Resuelve el Java requerido por la versión de Minecraft (no por el
/// loader): usado tanto para lanzar el juego como para correr el
/// instalador de Forge/NeoForge, que necesita un JRE igualmente.
async fn required_java_for(
    app: &AppHandle,
    state: &State<'_, AppState>,
    cfg: &LauncherConfig,
    paths: &GamePaths,
    minecraft_version: &str,
) -> Result<java::JavaInstallation, String> {
    let parent_detail = mc_install::load_version_detail(&state.http, paths, minecraft_version, &paths.cache)
        .await
        .map_err(to_err)?;
    let required_major = parent_detail.java_version.as_ref().map(|j| j.major_version).unwrap_or(17);
    let managed_dir = config::java_runtime_dir(app, cfg);
    let installations = java::detect_installations(&managed_dir).await;
    java::find_best(&installations, required_major).cloned().ok_or_else(|| {
        format!("No hay un Java {required_major}+ instalado. Ve a Configuración > Java para instalarlo.")
    })
}

// ---------------------------------------------------------------- Config --

#[tauri::command]
pub fn get_config(state: State<AppState>) -> LauncherConfig {
    state.config.read().unwrap().clone()
}

#[tauri::command]
pub fn get_branding_images() -> BrandingImages {
    BrandingImages {
        logo: config::logo_data_url(),
        icon: config::icon_data_url(),
        banner: config::banner_data_url(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingImages {
    pub logo: String,
    pub icon: String,
    pub banner: String,
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: LauncherConfig) -> Result<LauncherConfig, String> {
    config::save(&app, &config).map_err(to_err)?;
    state.discord.configure(config.discord_client_id.clone());
    *state.config.write().unwrap() = config.clone();
    let discord_handle = app.clone();
    let launcher_name = config.launcher_name.clone();
    tokio::task::spawn_blocking(move || {
        discord_handle.state::<AppState>().discord.set_menu(&launcher_name);
    });
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
    loader: mc_instance::LoaderKind,
    loader_version: Option<String>,
) -> Result<mc_instance::Instance, String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    let default_server = match (&cfg.default_server_name, &cfg.default_server_address) {
        (Some(name), Some(address)) => Some((name.as_str(), address.as_str())),
        _ => None,
    };
    mc_instance::create(
        &instances_dir,
        &name,
        &minecraft_version,
        loader,
        loader_version,
        default_server,
        cfg.apply_title_screen_pack,
    )
    .await
        .map_err(to_err)
}

#[tauri::command]
pub async fn list_loader_versions(
    state: State<'_, AppState>,
    minecraft_version: String,
    loader: mc_instance::LoaderKind,
) -> Result<Vec<fabric_like::LoaderVersionEntry>, String> {
    match loader {
        mc_instance::LoaderKind::Fabric | mc_instance::LoaderKind::Quilt => {
            let flavor = flavor_for(loader)?;
            fabric_like::list_loader_versions(&state.http, flavor, &minecraft_version)
                .await
                .map_err(to_err)
        }
        mc_instance::LoaderKind::Forge | mc_instance::LoaderKind::NeoForge => {
            let flavor = forge_flavor_for(loader)?;
            let versions = forge_like::list_versions(&state.http, flavor, &minecraft_version)
                .await
                .map_err(to_err)?;
            Ok(versions
                .into_iter()
                .map(|version| {
                    let stable = !version.contains("beta") && !version.contains("pre");
                    fabric_like::LoaderVersionEntry { version, stable }
                })
                .collect())
        }
        mc_instance::LoaderKind::Vanilla | mc_instance::LoaderKind::Optifine => Ok(Vec::new()),
    }
}

// --------------------------------------------------------------- OptiFine --

#[tauri::command]
pub async fn import_optifine(app: AppHandle, source_path: String) -> Result<String, String> {
    let app_data = config::app_data_dir(&app);
    optifine::import_file(&app_data, std::path::Path::new(&source_path))
        .await
        .map_err(to_err)
}

#[tauri::command]
pub async fn list_optifine_imports(app: AppHandle, minecraft_version: String) -> Result<Vec<String>, String> {
    let app_data = config::app_data_dir(&app);
    optifine::list_imports(&app_data, &minecraft_version).await.map_err(to_err)
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
    let paths = game_paths(&app, &cfg);

    let deleted = mc_instance::load(&instances_dir, &instance_id).await.map_err(to_err)?;
    mc_instance::delete(&instances_dir, &instance_id).await.map_err(to_err)?;

    // Las librerías/assets/versiones se comparten entre instancias a
    // propósito (ver GamePaths) — pero eso significa que borrar la última
    // instancia que usaba una versión concreta no las limpia solo, y una
    // instancia nueva de esa misma versión aparecería "ya instalada" sin
    // haber descargado nada. Si ya nadie más la usa, sí la limpiamos.
    let remaining = mc_instance::list(&instances_dir).await.map_err(to_err)?;

    let mc_version_still_used = remaining.iter().any(|i| i.minecraft_version == deleted.minecraft_version);
    if !mc_version_still_used {
        let _ = tokio::fs::remove_dir_all(paths.versions.join(&deleted.minecraft_version)).await;
    }

    if deleted.loader != mc_instance::LoaderKind::Vanilla {
        let same_loader_still_used = remaining.iter().any(|i| {
            i.minecraft_version == deleted.minecraft_version
                && i.loader == deleted.loader
                && i.loader_version == deleted.loader_version
        });
        if !same_loader_still_used {
            if let Some(loader_version) = &deleted.loader_version {
                match deleted.loader {
                    mc_instance::LoaderKind::Fabric | mc_instance::LoaderKind::Quilt => {
                        if let Ok(flavor) = flavor_for(deleted.loader) {
                            fabric_like::forget_installation(&paths, flavor, &deleted.minecraft_version, loader_version)
                                .await;
                        }
                    }
                    mc_instance::LoaderKind::Forge | mc_instance::LoaderKind::NeoForge => {
                        if let Ok(flavor) = forge_flavor_for(deleted.loader) {
                            forge_like::forget_installation(&paths, flavor, &deleted.minecraft_version, loader_version)
                                .await;
                        }
                    }
                    mc_instance::LoaderKind::Optifine => {
                        optifine::forget_installation(&paths, &deleted.minecraft_version, loader_version).await;
                    }
                    mc_instance::LoaderKind::Vanilla => {}
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn is_instance_installed(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<bool, String> {
    let cfg = state.config.read().unwrap().clone();
    let instances_dir = config::instances_dir(&app, &cfg);
    let instance = mc_instance::load(&instances_dir, &instance_id).await.map_err(to_err)?;
    let paths = game_paths(&app, &cfg);

    Ok(match instance.loader {
        mc_instance::LoaderKind::Vanilla => mc_install::is_installed(&paths, &instance.minecraft_version),
        mc_instance::LoaderKind::Fabric | mc_instance::LoaderKind::Quilt => {
            let flavor = flavor_for(instance.loader)?;
            let loader_version = instance.loader_version.clone().unwrap_or_default();
            fabric_like::is_installed(&paths, flavor, &instance.minecraft_version, &loader_version)
        }
        mc_instance::LoaderKind::Forge | mc_instance::LoaderKind::NeoForge => {
            let flavor = forge_flavor_for(instance.loader)?;
            let loader_version = instance.loader_version.clone().unwrap_or_default();
            forge_like::is_installed(&paths, flavor, &instance.minecraft_version, &loader_version)
        }
        mc_instance::LoaderKind::Optifine => {
            let loader_version = instance.loader_version.clone().unwrap_or_default();
            optifine::is_installed(&paths, &instance.minecraft_version, &loader_version)
        }
    })
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

    match instance.loader {
        mc_instance::LoaderKind::Vanilla => {
            mc_install::install_vanilla(&state.http, &state.downloads, &paths, &instance.minecraft_version, on_progress)
                .await
                .map_err(to_err)?;
        }
        mc_instance::LoaderKind::Fabric | mc_instance::LoaderKind::Quilt => {
            let flavor = flavor_for(instance.loader)?;
            let loader_version = instance
                .loader_version
                .clone()
                .ok_or_else(|| "esta versión no tiene un loader asignado".to_string())?;
            fabric_like::install(
                &state.http,
                &state.downloads,
                &paths,
                flavor,
                &instance.minecraft_version,
                &loader_version,
                on_progress,
            )
            .await
            .map_err(to_err)?;
        }
        mc_instance::LoaderKind::Forge | mc_instance::LoaderKind::NeoForge => {
            let flavor = forge_flavor_for(instance.loader)?;
            let loader_version = instance
                .loader_version
                .clone()
                .ok_or_else(|| "esta versión no tiene un loader asignado".to_string())?;
            let java_installation = required_java_for(&app, &state, &cfg, &paths, &instance.minecraft_version).await?;
            let app_data = config::app_data_dir(&app);
            forge_like::install(
                &state.http,
                &state.downloads,
                &app_data,
                &paths,
                &java_installation.path,
                flavor,
                &instance.minecraft_version,
                &loader_version,
                on_progress,
            )
            .await
            .map_err(to_err)?;
        }
        mc_instance::LoaderKind::Optifine => {
            let loader_version = instance
                .loader_version
                .clone()
                .ok_or_else(|| "esta versión no tiene un archivo de OptiFine importado".to_string())?;
            let java_installation = required_java_for(&app, &state, &cfg, &paths, &instance.minecraft_version).await?;
            let app_data = config::app_data_dir(&app);
            optifine::install(
                &state.http,
                &state.downloads,
                &app_data,
                &paths,
                &java_installation.path,
                &instance.minecraft_version,
                &loader_version,
                on_progress,
            )
            .await
            .map_err(to_err)?;
        }
    }
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

    let detail: VersionDetail = match instance.loader {
        mc_instance::LoaderKind::Vanilla => {
            mc_install::load_version_detail(&state.http, &paths, &instance.minecraft_version, &paths.cache)
                .await
                .map_err(to_err)?
        }
        mc_instance::LoaderKind::Fabric | mc_instance::LoaderKind::Quilt => {
            let flavor = flavor_for(instance.loader)?;
            let loader_version = instance
                .loader_version
                .clone()
                .ok_or_else(|| "esta versión no tiene un loader asignado".to_string())?;
            fabric_like::load_cached_detail(&paths, flavor, &instance.minecraft_version, &loader_version)
                .await
                .map_err(to_err)?
        }
        mc_instance::LoaderKind::Forge | mc_instance::LoaderKind::NeoForge => {
            let flavor = forge_flavor_for(instance.loader)?;
            let loader_version = instance
                .loader_version
                .clone()
                .ok_or_else(|| "esta versión no tiene un loader asignado".to_string())?;
            forge_like::load_cached_detail(&paths, flavor, &instance.minecraft_version, &loader_version)
                .await
                .map_err(to_err)?
        }
        mc_instance::LoaderKind::Optifine => {
            let loader_version = instance
                .loader_version
                .clone()
                .ok_or_else(|| "esta versión no tiene un archivo de OptiFine importado".to_string())?;
            optifine::load_cached_detail(&paths, &instance.minecraft_version, &loader_version)
                .await
                .map_err(to_err)?
        }
    };

    let required_major = detail.java_version.as_ref().map(|j| j.major_version).unwrap_or(17);
    let managed_dir = config::java_runtime_dir(&app, &cfg);
    let installations = java::detect_installations(&managed_dir).await;
    let java_installation = java::find_best(&installations, required_major).ok_or_else(|| {
        format!("No hay un Java {required_major}+ instalado. Ve a Configuración > Java para instalarlo.")
    })?;

    let app_data = config::app_data_dir(&app);
    let all_accounts = accounts::list_accounts(&app_data).await.map_err(to_err)?;
    let mut account = all_accounts
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| "cuenta no encontrada".to_string())?;

    // Las cuentas de Microsoft expiran (~1h) — si ya casi vence, la
    // renovamos con el refresh token antes de lanzar, sin pedirle login al
    // usuario de nuevo.
    if matches!(account.kind, accounts::AccountKind::Microsoft) && accounts::microsoft::needs_refresh(&account) {
        let client_id = cfg
            .microsoft_client_id
            .clone()
            .ok_or_else(|| "No hay un microsoftClientId configurado.".to_string())?;
        let refresh_token = account
            .refresh_token
            .clone()
            .ok_or_else(|| "Esta cuenta de Microsoft no tiene refresh token — vuelve a iniciar sesión.".to_string())?;
        account = accounts::microsoft::refresh_account(&state.http, &client_id, &refresh_token)
            .await
            .map_err(to_err)?;
        accounts::upsert_account(&app_data, account.clone()).await.map_err(to_err)?;
    }

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
            version_type_label: cfg.version_type_label.clone(),
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

    {
        let discord_handle = app.clone();
        let instance_name = instance.name.clone();
        let minecraft_version = instance.minecraft_version.clone();
        let loader_label = format!("{:?}", instance.loader);
        let started_at_unix = chrono::Utc::now().timestamp();
        tokio::task::spawn_blocking(move || {
            let state = discord_handle.state::<AppState>();
            state.discord.set_playing(&instance_name, &minecraft_version, &loader_label, started_at_unix);
        });
    }

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

                let discord_handle = app.clone();
                let launcher_name = discord_handle.state::<AppState>().config.read().unwrap().launcher_name.clone();
                tokio::task::spawn_blocking(move || {
                    discord_handle.state::<AppState>().discord.set_menu(&launcher_name);
                });
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

#[tauri::command]
pub async fn start_microsoft_login(
    state: State<'_, AppState>,
) -> Result<accounts::microsoft::DeviceCodeInfo, String> {
    let client_id = {
        let cfg = state.config.read().unwrap();
        cfg.microsoft_client_id.clone()
    }
    .ok_or_else(|| {
        "No hay un microsoftClientId configurado. Ve al README para registrar tu propia app en Microsoft Entra."
            .to_string()
    })?;

    let (info, pending) = accounts::microsoft::start_device_code(&state.http, &client_id)
        .await
        .map_err(to_err)?;
    *state.pending_ms_login.write().unwrap() = Some(pending);
    Ok(info)
}

#[tauri::command]
pub async fn complete_microsoft_login(app: AppHandle, state: State<'_, AppState>) -> Result<Account, String> {
    let client_id = {
        let cfg = state.config.read().unwrap();
        cfg.microsoft_client_id.clone()
    }
    .ok_or_else(|| "No hay un microsoftClientId configurado.".to_string())?;

    // El login puede tardar minutos (el usuario tiene que ir a completar el
    // paso en el navegador) — el `PendingLogin` no se puede clonar/mantener
    // el lock abierto todo ese tiempo, así que lo sacamos del estado ahora y
    // lo regresamos si falla, para poder reintentar sin pedir un código nuevo.
    let pending = state
        .pending_ms_login
        .write()
        .unwrap()
        .take()
        .ok_or_else(|| "No hay un login de Microsoft en curso — empieza de nuevo.".to_string())?;

    let result = accounts::microsoft::complete_login(&state.http, &client_id, &pending).await;

    match result {
        Ok(account) => {
            let app_data = config::app_data_dir(&app);
            accounts::upsert_account(&app_data, account.clone()).await.map_err(to_err)?;
            Ok(account)
        }
        Err(err) => {
            // Timeout/cancelado: no tiene caso reintentar con el mismo
            // device_code (ya expiró o el usuario lo rechazó). Cualquier
            // otro error (de red, por ejemplo) sí vale la pena reintentar.
            if !matches!(
                err,
                accounts::microsoft::MicrosoftAuthError::TimedOut | accounts::microsoft::MicrosoftAuthError::Declined
            ) {
                *state.pending_ms_login.write().unwrap() = Some(pending);
            }
            Err(to_err(err))
        }
    }
}
