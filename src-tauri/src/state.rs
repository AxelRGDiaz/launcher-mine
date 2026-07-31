use crate::config::LauncherConfig;
use crate::download::DownloadManager;
use std::collections::HashMap;
use std::sync::RwLock;
use tokio::process::Child;

/// Estado compartido de toda la app, inyectado por Tauri en cada comando.
pub struct AppState {
    pub config: RwLock<LauncherConfig>,
    pub downloads: DownloadManager,
    pub http: reqwest::Client,
    /// Procesos de Minecraft en ejecución, por id de instancia, para poder
    /// leer su log en vivo o matarlos desde la UI.
    pub running_instances: RwLock<HashMap<String, RunningGame>>,
}

pub struct RunningGame {
    pub child: Child,
    /// Se guarda para exponer más adelante un comando "ver log actual" desde
    /// la UI; hoy el streaming en vivo ya va por eventos, así que nada lo lee
    /// todavía.
    #[allow(dead_code)]
    pub log_path: std::path::PathBuf,
}

impl AppState {
    pub fn new(config: LauncherConfig) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("MiLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("no se pudo construir el cliente HTTP");
        Self {
            config: RwLock::new(config),
            downloads: DownloadManager::new(),
            http,
            running_instances: RwLock::new(HashMap::new()),
        }
    }
}
