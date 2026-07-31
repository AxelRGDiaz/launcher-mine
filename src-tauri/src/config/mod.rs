//! Configuración del launcher: se carga un `config.default.json` embebido en el
//! binario y se combina con (o se sobreescribe por) una copia editable por el
//! usuario en el directorio de datos de la app. Nada de branding va hardcodeado
//! en el resto del código: todo se lee de aquí.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Valores por defecto embebidos en el binario en tiempo de compilación, para
/// que el launcher funcione incluso si el usuario borra su config local.
const DEFAULT_CONFIG_JSON: &str = include_str!("../../../config/config.default.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherConfig {
    pub launcher_name: String,
    pub logo_path: String,
    pub icon_path: String,
    pub theme: String,
    pub primary_color: String,
    pub background_image: Option<String>,
    pub welcome_text: String,
    pub support_url: String,
    pub default_min_ram_mb: u32,
    pub default_max_ram_mb: u32,
    pub auto_update_java: bool,
    pub show_snapshots: bool,
    pub instances_dir: Option<String>,
    pub java_dir: Option<String>,
}

impl LauncherConfig {
    pub fn defaults() -> Self {
        serde_json::from_str(DEFAULT_CONFIG_JSON)
            .expect("config/config.default.json debe ser JSON válido")
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("no se pudo leer la config: {0}")]
    Io(#[from] std::io::Error),
    #[error("config.json inválido: {0}")]
    Parse(#[from] serde_json::Error),
}

fn user_config_path(app: &AppHandle) -> Result<PathBuf, ConfigError> {
    let dir = app
        .path()
        .app_config_dir()
        .expect("no se pudo resolver el directorio de configuración de la app");
    Ok(dir.join("config.json"))
}

/// Carga la config del usuario si existe; si no, crea una copia editable a
/// partir de los valores por defecto embebidos y la persiste.
pub fn load(app: &AppHandle) -> Result<LauncherConfig, ConfigError> {
    let path = user_config_path(app)?;
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        // Si el usuario dejó el JSON corrupto, no tumbamos el launcher: caemos
        // a los defaults en memoria (sin sobreescribir su archivo, por si lo
        // quiere recuperar a mano).
        match serde_json::from_str::<LauncherConfig>(&raw) {
            Ok(cfg) => Ok(cfg),
            Err(err) => {
                tracing::warn!("config.json inválido ({err}), usando valores por defecto");
                Ok(LauncherConfig::defaults())
            }
        }
    } else {
        let defaults = LauncherConfig::defaults();
        save(app, &defaults)?;
        Ok(defaults)
    }
}

pub fn save(app: &AppHandle, config: &LauncherConfig) -> Result<(), ConfigError> {
    let path = user_config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, pretty)?;
    Ok(())
}

pub fn reset_to_defaults(app: &AppHandle) -> Result<LauncherConfig, ConfigError> {
    let defaults = LauncherConfig::defaults();
    save(app, &defaults)?;
    Ok(defaults)
}

/// Directorio base de datos del launcher (instancias, java-runtime, caché de
/// descargas, cuentas). Todo vive bajo el directorio de datos de la app salvo
/// que el usuario redirija `instancesDir`/`javaDir` desde la config.
pub fn app_data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("no se pudo resolver el directorio de datos de la app")
}

pub fn instances_dir(app: &AppHandle, config: &LauncherConfig) -> PathBuf {
    match &config.instances_dir {
        Some(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => app_data_dir(app).join("instances"),
    }
}

pub fn java_runtime_dir(app: &AppHandle, config: &LauncherConfig) -> PathBuf {
    match &config.java_dir {
        Some(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => app_data_dir(app).join("java-runtime"),
    }
}

pub fn cache_dir(app: &AppHandle) -> PathBuf {
    app_data_dir(app).join("cache")
}

// Todavía sin un comando IPC que lo exponga: la UI no renderiza logo/icono
// como imagen en esta fase (ver "Limitaciones conocidas" en el README). Punto
// de extensión listo para cuando se resuelva vía el protocolo `asset://`.
#[allow(dead_code)]
pub fn resolve_asset_path(app: &AppHandle, relative: &str) -> PathBuf {
    // El usuario puede apuntar logo/icon a una ruta absoluta propia, o dejar
    // la relativa por defecto que vive junto a los recursos empaquetados.
    let p = Path::new(relative);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    app.path()
        .resource_dir()
        .map(|r| r.join(relative))
        .unwrap_or_else(|_| p.to_path_buf())
}
