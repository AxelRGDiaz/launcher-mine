//! Autenticación oficial de Microsoft/Xbox/Minecraft — el mismo flujo que
//! usa el launcher oficial de Mojang y MultiMC/Prism Launcher:
//!
//! 1. **Device code** (OAuth2, Microsoft identity platform): se pide un
//!    código corto que el usuario teclea en una página web, sin necesidad de
//!    un navegador embebido en el launcher.
//! 2. **Xbox Live**: se cambia el token de Microsoft por uno de Xbox Live.
//! 3. **XSTS**: se autoriza ese token para usarlo específicamente contra
//!    Minecraft Services.
//! 4. **Minecraft Services**: se cambia el token XSTS por el access token
//!    real de Minecraft, y con ese se pide el perfil (UUID, nombre, skin).
//!
//! El `client_id` viene de la configuración (`microsoftClientId`), nunca
//! hardcodeado: cada quien despliega este launcher con su propia app
//! registrada en Microsoft Entra (ver README).

use super::{Account, AccountKind};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const MC_SKIN_URL: &str = "https://api.minecraftservices.com/minecraft/profile/skins";

#[derive(thiserror::Error, Debug)]
pub enum MicrosoftAuthError {
    #[error("error de red: {0}")]
    Network(#[from] reqwest::Error),
    #[error("{0}")]
    Api(String),
    #[error("se agotó el tiempo de espera del login — vuelve a intentarlo")]
    TimedOut,
    #[error("iniciaste sesión cancelando el login")]
    Declined,
    #[error("esta cuenta de Microsoft no tiene Minecraft: Java Edition comprado")]
    NoGameOwnership,
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
}

/// Lo que se le muestra al usuario para completar el login en su navegador.
/// El `device_code` real (necesario para seguir haciendo polling) se queda
/// en el backend, no se manda al frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub struct PendingLogin {
    pub device_code: String,
    pub interval: u64,
    pub expires_at: std::time::Instant,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Convierte una respuesta de error HTTP en un mensaje legible, intentando
/// primero el formato de error de Microsoft Entra (`error_description`, con
/// el código `AADSTSxxxxx` real) antes de caer a un texto genérico — un
/// `.error_for_status()` normal descarta el cuerpo y solo deja "401
/// Unauthorized", que no dice nada sobre la causa real (p. ej. que el
/// registro de la app no tiene activado "Allow public client flows").
fn describe_error_body(status: u16, body: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(desc) = json.get("error_description").and_then(|v| v.as_str()) {
            let first_line = desc.lines().next().unwrap_or(desc);
            return format!("Microsoft rechazó la petición ({status}): {first_line}");
        }
        if let Some(msg) = json.get("Message").and_then(|v| v.as_str()) {
            return format!("Microsoft rechazó la petición ({status}): {msg}");
        }
    }
    format!("Microsoft rechazó la petición ({status}): {body}")
}

async fn send_json<T: DeserializeOwned>(builder: reqwest::RequestBuilder) -> Result<T, MicrosoftAuthError> {
    let resp = builder.send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(MicrosoftAuthError::Api(describe_error_body(status.as_u16(), &text)));
    }
    Ok(serde_json::from_str(&text)?)
}

pub async fn start_device_code(
    http: &reqwest::Client,
    client_id: &str,
) -> Result<(DeviceCodeInfo, PendingLogin), MicrosoftAuthError> {
    let params = [("client_id", client_id), ("scope", "XboxLive.signin offline_access")];
    let resp: DeviceCodeResponse = send_json(http.post(DEVICE_CODE_URL).form(&params)).await?;

    let info = DeviceCodeInfo {
        user_code: resp.user_code,
        verification_uri: resp.verification_uri,
        expires_in: resp.expires_in,
        interval: resp.interval,
    };
    let pending = PendingLogin {
        device_code: resp.device_code,
        interval: resp.interval,
        expires_at: std::time::Instant::now() + std::time::Duration::from_secs(resp.expires_in),
    };
    Ok((info, pending))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

async fn poll_for_token(
    http: &reqwest::Client,
    client_id: &str,
    pending: &PendingLogin,
) -> Result<TokenResponse, MicrosoftAuthError> {
    loop {
        if std::time::Instant::now() >= pending.expires_at {
            return Err(MicrosoftAuthError::TimedOut);
        }
        tokio::time::sleep(std::time::Duration::from_secs(pending.interval)).await;

        let params = [
            ("client_id", client_id),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", pending.device_code.as_str()),
        ];
        let resp = http.post(TOKEN_URL).form(&params).send().await?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await?;

        if status.is_success() {
            return Ok(serde_json::from_value(body)?);
        }

        match body.get("error").and_then(|v| v.as_str()).unwrap_or("") {
            "authorization_pending" => continue,
            "slow_down" => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            "expired_token" => return Err(MicrosoftAuthError::TimedOut),
            "authorization_declined" => return Err(MicrosoftAuthError::Declined),
            other => {
                let desc = body.get("error_description").and_then(|v| v.as_str()).unwrap_or(other);
                return Err(MicrosoftAuthError::Api(format!("Microsoft rechazó el login: {desc}")));
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblUserHash>,
}

#[derive(Debug, Deserialize)]
struct XblUserHash {
    uhs: String,
}

async fn authenticate_xbox_live(http: &reqwest::Client, ms_access_token: &str) -> Result<XblResponse, MicrosoftAuthError> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    send_json(http.post(XBL_AUTH_URL).json(&body)).await
}

/// Códigos `XErr` documentados de Xbox Live — los más comunes para cuentas
/// personales/familiares, justo el caso de uso de este launcher (alumnos de
/// primaria suelen tener cuentas infantiles bajo un grupo familiar).
fn describe_xsts_error(xerr: u64) -> String {
    match xerr {
        2148916233 => {
            "Esta cuenta de Microsoft no tiene un perfil de Xbox todavía — hay que crear uno en xbox.com antes de poder usarla aquí.".to_string()
        }
        2148916235 => "Xbox Live no está disponible en tu país o región.".to_string(),
        2148916236 | 2148916237 => {
            "Hay que verificar la edad de esta cuenta desde la página de la familia de Microsoft (account.microsoft.com/family) antes de poder usarla.".to_string()
        }
        2148916238 => {
            "Esta es una cuenta infantil (menor de edad) que todavía no está en un grupo familiar — un adulto debe agregarla en account.microsoft.com/family y dar el permiso correspondiente.".to_string()
        }
        _ => format!("Xbox Live rechazó la sesión (código {xerr})."),
    }
}

async fn authenticate_xsts(http: &reqwest::Client, xbl_token: &str) -> Result<XblResponse, MicrosoftAuthError> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });
    let resp = http.post(XSTS_AUTH_URL).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if status.as_u16() == 401 {
        let err_body: serde_json::Value = serde_json::from_str(&text)?;
        let xerr = err_body.get("XErr").and_then(|v| v.as_u64()).unwrap_or(0);
        return Err(MicrosoftAuthError::Api(describe_xsts_error(xerr)));
    }
    if !status.is_success() {
        return Err(MicrosoftAuthError::Api(describe_error_body(status.as_u16(), &text)));
    }
    Ok(serde_json::from_str(&text)?)
}

#[derive(Debug, Deserialize)]
struct McAuthResponse {
    access_token: String,
}

async fn authenticate_minecraft(
    http: &reqwest::Client,
    xsts_token: &str,
    user_hash: &str,
) -> Result<String, MicrosoftAuthError> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
    });
    let resp: McAuthResponse = send_json(http.post(MC_LOGIN_URL).json(&body)).await?;
    Ok(resp.access_token)
}

#[derive(Debug, Deserialize)]
struct McProfile {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<McSkin>,
}

#[derive(Debug, Deserialize)]
struct McSkin {
    url: String,
    state: String,
}

async fn fetch_profile(http: &reqwest::Client, mc_access_token: &str) -> Result<McProfile, MicrosoftAuthError> {
    let resp = http.get(MC_PROFILE_URL).bearer_auth(mc_access_token).send().await?;
    if resp.status().as_u16() == 404 {
        return Err(MicrosoftAuthError::NoGameOwnership);
    }
    Ok(resp.error_for_status()?.json().await?)
}

/// Sube una skin nueva (PNG 64x64) para una cuenta de Microsoft ya
/// autenticada. `variant` es `"classic"` (modelo Steve) o `"slim"` (modelo
/// Alex). Devuelve la URL de la skin activa tras el cambio.
pub async fn upload_skin(
    http: &reqwest::Client,
    mc_access_token: &str,
    file_bytes: Vec<u8>,
    variant: &str,
) -> Result<Option<String>, MicrosoftAuthError> {
    let part = reqwest::multipart::Part::bytes(file_bytes).file_name("skin.png").mime_str("image/png")?;
    let form = reqwest::multipart::Form::new().text("variant", variant.to_string()).part("file", part);
    let profile: McProfile = http
        .post(MC_SKIN_URL)
        .bearer_auth(mc_access_token)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(profile.skins.into_iter().find(|s| s.state == "ACTIVE").map(|s| s.url))
}

async fn build_account_from_ms_token(
    http: &reqwest::Client,
    ms_access_token: &str,
    refresh_token: Option<String>,
    expires_in: u64,
) -> Result<Account, MicrosoftAuthError> {
    let xbl = authenticate_xbox_live(http, ms_access_token).await?;
    let user_hash = xbl
        .display_claims
        .xui
        .first()
        .map(|x| x.uhs.clone())
        .ok_or_else(|| MicrosoftAuthError::Api("Xbox Live no devolvió un userhash válido".to_string()))?;
    let xsts = authenticate_xsts(http, &xbl.token).await?;
    let mc_access_token = authenticate_minecraft(http, &xsts.token, &user_hash).await?;
    let profile = fetch_profile(http, &mc_access_token).await?;
    let skin_url = profile.skins.into_iter().find(|s| s.state == "ACTIVE").map(|s| s.url);

    Ok(Account {
        id: profile.id.clone(),
        kind: AccountKind::Microsoft,
        username: profile.name,
        uuid: profile.id,
        access_token: mc_access_token,
        skin_url,
        refresh_token,
        expires_at: Some(chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64)),
    })
}

/// Completa el login: hace polling hasta que el usuario termine en el
/// navegador (o expire/cancele), y arma la cuenta con perfil real.
pub async fn complete_login(
    http: &reqwest::Client,
    client_id: &str,
    pending: &PendingLogin,
) -> Result<Account, MicrosoftAuthError> {
    let token = poll_for_token(http, client_id, pending).await?;
    build_account_from_ms_token(http, &token.access_token, token.refresh_token, token.expires_in).await
}

/// Renueva una sesión ya existente sin pedirle login al usuario de nuevo.
pub async fn refresh_account(
    http: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<Account, MicrosoftAuthError> {
    let params = [
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", "XboxLive.signin offline_access"),
    ];
    let token: TokenResponse = send_json(http.post(TOKEN_URL).form(&params)).await?;
    let next_refresh_token = token.refresh_token.clone().unwrap_or_else(|| refresh_token.to_string());
    build_account_from_ms_token(http, &token.access_token, Some(next_refresh_token), token.expires_in).await
}

/// true si falta menos de 5 minutos para que expire (o ya expiró) — margen
/// para que no se venza a mitad del proceso de lanzar el juego.
pub fn needs_refresh(account: &Account) -> bool {
    match account.expires_at {
        Some(expires_at) => expires_at - chrono::Utc::now() < chrono::Duration::minutes(5),
        None => false,
    }
}
