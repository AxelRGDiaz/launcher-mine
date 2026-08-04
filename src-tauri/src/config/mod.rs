//! Configuración del launcher: se carga un `config.default.json` embebido en el
//! binario y se combina con (o se sobreescribe por) una copia editable por el
//! usuario en el directorio de datos de la app. Nada de branding va hardcodeado
//! en el resto del código: todo se lee de aquí.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    /// Servidor precargado en la lista de multijugador de cada instancia
    /// nueva (ver `minecraft::instance::write_default_servers_dat`). `None`
    /// = no se agrega ninguno.
    #[serde(default)]
    pub default_server_name: Option<String>,
    #[serde(default)]
    pub default_server_address: Option<String>,
    /// Aplica el resource pack embebido que reemplaza el panorama de la
    /// pantalla de título del juego (no del launcher) por el banner
    /// configurado. Ver `assets/title_resourcepack.zip`.
    #[serde(default = "default_true")]
    pub apply_title_screen_pack: bool,
    /// Texto que Minecraft dibuja junto a "Minecraft <versión>" en la
    /// esquina inferior izquierda del menú principal (el `version_type` del
    /// lanzamiento). Separado de `launcherName` porque conviene más corto.
    #[serde(default = "default_version_type_label")]
    pub version_type_label: String,
    /// "Application (client) ID" de la app registrada en Microsoft Entra
    /// para el login real de Microsoft/Xbox. `None` = el botón de "Agregar
    /// cuenta Microsoft" queda deshabilitado — ver README para cómo
    /// registrar la tuya (gratis, requisito de Microsoft, no se puede
    /// incluir un client_id genérico).
    #[serde(default)]
    pub microsoft_client_id: Option<String>,
    /// "Application ID" de una app registrada en el portal de
    /// desarrolladores de Discord (discord.com/developers/applications),
    /// para mostrar el launcher/el juego como "Playing" en Discord. `None` =
    /// función desactivada, sin ningún efecto en el resto del launcher.
    #[serde(default)]
    pub discord_client_id: Option<String>,
}

fn default_version_type_label() -> String {
    "PikiPiki".to_string()
}

fn default_true() -> bool {
    true
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
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(saved_value) => {
                // Se deserializa lo guardado fusionado sobre los defaults
                // (para que campos ausentes por venir de una versión vieja
                // no rompan el parseo), pero de ahí solo se copian los
                // campos que de verdad se editan desde Configuración —ver
                // `Settings.tsx`—. Todo lo demás (nombre, logo, colores,
                // supportUrl, client_ids...) se fija antes de compilar y
                // siempre debe reflejar lo que trae ESTE binario: si no,
                // una instalación vieja se queda para siempre con valores
                // placeholder o client_ids que ya no aplican.
                let mut merged = serde_json::to_value(LauncherConfig::defaults())?;
                if let (Some(merged_obj), Some(saved_obj)) = (merged.as_object_mut(), saved_value.as_object()) {
                    for (key, value) in saved_obj {
                        merged_obj.insert(key.clone(), value.clone());
                    }
                }
                match serde_json::from_value::<LauncherConfig>(merged) {
                    Ok(saved_cfg) => {
                        let mut cfg = LauncherConfig::defaults();
                        cfg.theme = saved_cfg.theme;
                        cfg.show_snapshots = saved_cfg.show_snapshots;
                        cfg.default_min_ram_mb = saved_cfg.default_min_ram_mb;
                        cfg.default_max_ram_mb = saved_cfg.default_max_ram_mb;
                        cfg.default_server_name = saved_cfg.default_server_name;
                        cfg.default_server_address = saved_cfg.default_server_address;
                        cfg.apply_title_screen_pack = saved_cfg.apply_title_screen_pack;
                        cfg.version_type_label = saved_cfg.version_type_label;
                        Ok(cfg)
                    }
                    Err(err) => {
                        tracing::warn!("config.json inválido ({err}), usando valores por defecto");
                        Ok(LauncherConfig::defaults())
                    }
                }
            }
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

// Igual que el nombre/color, estas imágenes se fijan antes de compilar, no
// se leen en tiempo de ejecución desde una ruta configurable: se embeben en
// el binario para no depender de la resolución de `resource_dir` (distinta
// entre `tauri dev` y el instalador final) ni de que el archivo exista en
// disco tal cual quedó en la máquina donde se compiló.
const LOGO_BYTES: &[u8] = include_bytes!("../../../assets/logo.png");
const ICON_BYTES: &[u8] = include_bytes!("../../../assets/icon.png");
const BANNER_BYTES: &[u8] = include_bytes!("../../../assets/banner.jpg");

/// Resource pack que reemplaza el panorama de la pantalla de título del
/// juego por el banner configurado. Ver el README para cómo se generó
/// (recorte cuadrado del banner en las 6 caras) y su limitación conocida:
/// se ve como la misma imagen repetida, no un entorno 360° real.
pub const TITLE_SCREEN_RESOURCE_PACK: &[u8] = include_bytes!("../../../assets/title_resourcepack.zip");

fn as_data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub fn logo_data_url() -> String {
    as_data_url("image/png", LOGO_BYTES)
}

pub fn icon_data_url() -> String {
    as_data_url("image/png", ICON_BYTES)
}

pub fn banner_data_url() -> String {
    as_data_url("image/jpeg", BANNER_BYTES)
}
