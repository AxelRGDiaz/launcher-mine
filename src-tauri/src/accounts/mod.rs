//! Gestión de cuentas: **offline/dev** (pruebas locales, ver nota legal más
//! abajo) y **Microsoft** (login real vía device code + Xbox Live + XSTS +
//! Minecraft Services, implementado en `microsoft.rs`).
//!
//! IMPORTANTE (nota legal/técnica sobre `Offline`): esto NO es un "crack" ni
//! un bypass de autenticación. Es exactamente el mismo modo "cuenta sin
//! conexión" que ofrecen MultiMC/Prism Launcher para pruebas en un solo
//! jugador con una copia ya poseída. El multijugador y Realms los valida el
//! propio servidor de Mojang/Xbox del lado remoto, así que este modo no
//! permite (ni intenta permitir) jugar online sin una cuenta de Microsoft
//! real.

pub mod microsoft;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub kind: AccountKind,
    pub username: String,
    pub uuid: String,
    /// Para `Offline` esto no es un token real de sesión: el juego lo recibe
    /// pero no se valida contra ningún servicio de Mojang/Xbox. Para
    /// `Microsoft` es el access token real de Minecraft Services.
    pub access_token: String,
    pub skin_url: Option<String>,
    /// Solo `Microsoft`: para renovar la sesión sin pedir login de nuevo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// UUID determinista igual al que calcula el propio cliente de Minecraft
/// para cuentas sin conexión: `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(UTF_8))`.
pub fn offline_uuid(username: &str) -> uuid::Uuid {
    let digest = md5::compute(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // versión 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variante RFC4122
    uuid::Uuid::from_bytes(bytes)
}

pub fn create_offline_account(username: &str) -> Account {
    let uuid = offline_uuid(username);
    Account {
        id: uuid.to_string(),
        kind: AccountKind::Offline,
        username: username.to_string(),
        uuid: uuid.to_string(),
        access_token: "0".to_string(),
        skin_url: None,
        refresh_token: None,
        expires_at: None,
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AccountError {
    #[error("nombre de usuario inválido: debe tener entre 3 y 16 caracteres alfanuméricos o '_'")]
    InvalidUsername,
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn validate_username(username: &str) -> Result<(), AccountError> {
    let valid = (3..=16).contains(&username.len())
        && username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(AccountError::InvalidUsername)
    }
}

fn accounts_path(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join("accounts.json")
}

pub async fn list_accounts(app_data_dir: &std::path::Path) -> Result<Vec<Account>, AccountError> {
    let path = accounts_path(app_data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

async fn save_accounts(app_data_dir: &std::path::Path, accounts: &[Account]) -> Result<(), AccountError> {
    let path = accounts_path(app_data_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_vec_pretty(accounts)?).await?;
    Ok(())
}

pub async fn add_account(app_data_dir: &std::path::Path, username: &str) -> Result<Account, AccountError> {
    validate_username(username)?;
    let mut accounts = list_accounts(app_data_dir).await?;
    let account = create_offline_account(username);
    accounts.retain(|a| a.id != account.id);
    accounts.push(account.clone());
    save_accounts(app_data_dir, &accounts).await?;
    Ok(account)
}

pub async fn remove_account(app_data_dir: &std::path::Path, account_id: &str) -> Result<(), AccountError> {
    let mut accounts = list_accounts(app_data_dir).await?;
    accounts.retain(|a| a.id != account_id);
    save_accounts(app_data_dir, &accounts).await
}

/// Agrega la cuenta o, si ya existía una con el mismo `id` (mismo UUID de
/// Minecraft), la reemplaza — así renovar el token de una cuenta Microsoft
/// no crea una entrada duplicada.
pub async fn upsert_account(app_data_dir: &std::path::Path, account: Account) -> Result<Account, AccountError> {
    let mut accounts = list_accounts(app_data_dir).await?;
    accounts.retain(|a| a.id != account.id);
    accounts.push(account.clone());
    save_accounts(app_data_dir, &accounts).await?;
    Ok(account)
}
