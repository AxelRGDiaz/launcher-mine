pub mod fabric_like;
pub mod forge_like;
pub mod install;
pub mod instance;
pub mod launch;
pub mod manifest;
pub mod optifine;

use std::path::{Path, PathBuf};

/// Rutas compartidas por todas las instancias: librerías y assets son
/// direccionados por versión/hash, así que Vanilla/Forge/Fabric/etc. de
/// distintas instancias que comparten versión no duplican nada en disco.
#[derive(Debug, Clone)]
pub struct GamePaths {
    pub libraries: PathBuf,
    pub assets: PathBuf,
    pub versions: PathBuf,
    pub instances: PathBuf,
    pub cache: PathBuf,
}

impl GamePaths {
    pub fn new(app_data_dir: &Path, instances_dir: &Path, cache_dir: &Path) -> Self {
        Self {
            libraries: app_data_dir.join("libraries"),
            assets: app_data_dir.join("assets"),
            versions: app_data_dir.join("versions"),
            instances: instances_dir.to_path_buf(),
            cache: cache_dir.to_path_buf(),
        }
    }
}

pub fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

#[derive(thiserror::Error, Debug)]
pub enum McError {
    #[error("versión de Minecraft desconocida: {0}")]
    UnknownVersion(String),
    #[error(transparent)]
    Download(#[from] crate::download::DownloadError),
    #[error("error de red: {0}")]
    Network(#[from] reqwest::Error),
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
    #[error("instancia no encontrada: {0}")]
    InstanceNotFound(String),
    #[error("el instalador falló: {0}")]
    InstallerFailed(String),
}
