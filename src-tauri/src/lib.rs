mod accounts;
mod commands;
mod config;
mod discord;
mod download;
mod java;
mod minecraft;
mod process_ext;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // En release, Windows compila la app sin consola adjunta (para
            // que no aparezca una ventana negra detrás de la GUI) — por eso
            // los logs por stdout no sirven para diagnosticar nada ahí, ni
            // siquiera corriendo el .exe desde cmd/PowerShell. Se escriben a
            // un archivo real en su lugar.
            let log_dir = config::app_data_dir(&handle);
            let _ = std::fs::create_dir_all(&log_dir);
            let log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_dir.join("launcher.log"));
            let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
            match log_file {
                Ok(file) => {
                    tracing_subscriber::fmt().with_env_filter(env_filter).with_ansi(false).with_writer(file).init();
                }
                Err(_) => {
                    tracing_subscriber::fmt().with_env_filter(env_filter).init();
                }
            }

            let cfg = config::load(&handle).expect("no se pudo cargar la configuración del launcher");
            let launcher_name = cfg.launcher_name.clone();
            let discord_client_id = cfg.discord_client_id.clone();
            app.manage(AppState::new(cfg));

            // La conexión IPC a Discord es bloqueante y no debe demorar el
            // arranque de la app — y si Discord no está corriendo, debe
            // fallar en silencio sin afectar nada más.
            let discord_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let state = discord_handle.state::<AppState>();
                    state.discord.configure(discord_client_id);
                    state.discord.set_menu(&launcher_name);
                })
                .await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::get_branding_images,
            commands::save_config,
            commands::reset_config,
            commands::system_memory_mb,
            commands::detect_java,
            commands::install_java,
            commands::list_minecraft_versions,
            commands::list_instances,
            commands::create_instance,
            commands::list_loader_versions,
            commands::import_optifine,
            commands::list_optifine_imports,
            commands::update_instance,
            commands::delete_instance,
            commands::is_instance_installed,
            commands::install_instance,
            commands::launch_instance,
            commands::is_instance_running,
            commands::list_mods,
            commands::toggle_mod,
            commands::list_accounts,
            commands::add_account,
            commands::remove_account,
            commands::start_microsoft_login,
            commands::complete_microsoft_login,
            commands::change_skin,
        ])
        .run(tauri::generate_context!())
        .expect("error corriendo la aplicación Tauri");
}
