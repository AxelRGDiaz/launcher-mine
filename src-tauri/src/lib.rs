mod accounts;
mod commands;
mod config;
mod download;
mod java;
mod minecraft;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config::load(&handle).expect("no se pudo cargar la configuración del launcher");
            app.manage(AppState::new(cfg));
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
        ])
        .run(tauri::generate_context!())
        .expect("error corriendo la aplicación Tauri");
}
