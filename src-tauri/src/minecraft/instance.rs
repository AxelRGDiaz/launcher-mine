//! Modelo de instancia: una carpeta independiente con su propio `.minecraft`
//! (saves, mods, resourcepacks, config, screenshots) que referencia una
//! versión/loader compartidos globalmente. Ver `GamePaths` para el porqué de
//! compartir librerías/assets/versiones entre instancias.

use super::McError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Vanilla,
    Forge,
    NeoForge,
    Fabric,
    Quilt,
    Optifine,
}

impl Default for LoaderKind {
    fn default() -> Self {
        LoaderKind::Vanilla
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft_version: String,
    #[serde(default)]
    pub loader: LoaderKind,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub min_ram_mb: Option<u32>,
    pub max_ram_mb: Option<u32>,
    #[serde(default)]
    pub extra_jvm_args: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_played: Option<DateTime<Utc>>,
    #[serde(default)]
    pub total_playtime_secs: u64,
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        slug.to_lowercase()
    }
}

pub fn instance_dir(instances_root: &Path, id: &str) -> PathBuf {
    instances_root.join(id)
}

pub fn minecraft_dir(instances_root: &Path, id: &str) -> PathBuf {
    instance_dir(instances_root, id).join("minecraft")
}

fn instance_json_path(instances_root: &Path, id: &str) -> PathBuf {
    instance_dir(instances_root, id).join("instance.json")
}

pub async fn create(
    instances_root: &Path,
    name: &str,
    minecraft_version: &str,
    loader: LoaderKind,
    loader_version: Option<String>,
    default_server: Option<(&str, &str)>,
    apply_title_screen_pack: bool,
) -> Result<Instance, McError> {
    let base_slug = slugify(name);
    let mut id = base_slug.clone();
    let mut suffix = 1;
    while instance_dir(instances_root, &id).exists() {
        suffix += 1;
        id = format!("{base_slug}-{suffix}");
    }

    let instance = Instance {
        id: id.clone(),
        name: name.to_string(),
        minecraft_version: minecraft_version.to_string(),
        loader,
        loader_version,
        min_ram_mb: None,
        max_ram_mb: None,
        extra_jvm_args: None,
        created_at: Utc::now(),
        last_played: None,
        total_playtime_secs: 0,
    };

    let mc_dir = minecraft_dir(instances_root, &id);
    tokio::fs::create_dir_all(&mc_dir).await?;
    save(instances_root, &instance).await?;

    if let Some((server_name, server_address)) = default_server {
        write_default_servers_dat(&mc_dir, server_name, server_address).await?;
    }
    if apply_title_screen_pack {
        write_title_screen_pack(&mc_dir).await?;
    }

    Ok(instance)
}

// ------------------------------------------------ Branding de la partida --
// A diferencia del resto de la instancia (versión/loader/RAM), esto no es
// funcionalidad del juego: es branding que se escribe una vez al crear la
// instancia, dentro de su propio `.minecraft`, usando los mismos formatos
// que el launcher oficial de Mojang (servers.dat) y el sistema estándar de
// resource packs — nada de esto requiere un mod ni modifica el jar del juego.

fn write_be_u16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn write_be_i32(buf: &mut Vec<u8>, value: i32) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn write_nbt_tag_header(buf: &mut Vec<u8>, tag_type: u8, name: &str) {
    buf.push(tag_type);
    write_be_u16(buf, name.len() as u16);
    buf.extend_from_slice(name.as_bytes());
}

fn write_nbt_string_payload(buf: &mut Vec<u8>, value: &str) {
    write_be_u16(buf, value.len() as u16);
    buf.extend_from_slice(value.as_bytes());
}

/// Arma un `servers.dat` (NBT sin comprimir) con un único servidor
/// precargado. Formato: TAG_Compound raíz sin nombre > TAG_List "servers" de
/// TAG_Compound > tags `name`/`ip`/`acceptTextures` — el mismo que escribe el
/// launcher oficial de Mojang.
fn build_servers_dat(name: &str, address: &str) -> Vec<u8> {
    const TAG_END: u8 = 0;
    const TAG_BYTE: u8 = 1;
    const TAG_STRING: u8 = 8;
    const TAG_LIST: u8 = 9;
    const TAG_COMPOUND: u8 = 10;

    let mut buf = Vec::new();
    write_nbt_tag_header(&mut buf, TAG_COMPOUND, "");

    write_nbt_tag_header(&mut buf, TAG_LIST, "servers");
    buf.push(TAG_COMPOUND);
    write_be_i32(&mut buf, 1); // un solo servidor en la lista

    write_nbt_tag_header(&mut buf, TAG_STRING, "name");
    write_nbt_string_payload(&mut buf, name);
    write_nbt_tag_header(&mut buf, TAG_STRING, "ip");
    write_nbt_string_payload(&mut buf, address);
    write_nbt_tag_header(&mut buf, TAG_BYTE, "acceptTextures");
    buf.push(1); // acepta el resource pack del servidor sin preguntar cada vez
    buf.push(TAG_END); // cierra el compound del servidor

    buf.push(TAG_END); // cierra el compound raíz
    buf
}

async fn write_default_servers_dat(minecraft_dir: &Path, name: &str, address: &str) -> Result<(), McError> {
    let path = minecraft_dir.join("servers.dat");
    if path.exists() {
        return Ok(()); // no pisar una lista que el jugador ya haya editado
    }
    tokio::fs::write(path, build_servers_dat(name, address)).await?;
    Ok(())
}

/// Copia el resource pack embebido (panorama de título personalizado) a la
/// instancia y lo activa en `options.txt`. Limitación conocida: como el
/// panorama real del juego es un entorno 3D de 6 caras y el pack solo pone
/// una imagen recortada repetida, se ve como "la imagen de fondo repetida al
/// girar la cámara", no como un ambiente continuo — es una decisión de
/// diseño, no un bug.
async fn write_title_screen_pack(minecraft_dir: &Path) -> Result<(), McError> {
    let resourcepacks_dir = minecraft_dir.join("resourcepacks");
    tokio::fs::create_dir_all(&resourcepacks_dir).await?;
    tokio::fs::write(
        resourcepacks_dir.join("title_resourcepack.zip"),
        crate::config::TITLE_SCREEN_RESOURCE_PACK,
    )
    .await?;

    let options_path = minecraft_dir.join("options.txt");
    let mut lines: Vec<String> = if options_path.exists() {
        tokio::fs::read_to_string(&options_path)
            .await?
            .lines()
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    lines.retain(|line| !line.starts_with("resourcePacks:"));
    lines.push(r#"resourcePacks:["vanilla","file/title_resourcepack.zip"]"#.to_string());
    tokio::fs::write(&options_path, lines.join("\n") + "\n").await?;
    Ok(())
}

pub async fn save(instances_root: &Path, instance: &Instance) -> Result<(), McError> {
    let path = instance_json_path(instances_root, &instance.id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_vec_pretty(instance)?).await?;
    Ok(())
}

pub async fn load(instances_root: &Path, id: &str) -> Result<Instance, McError> {
    let path = instance_json_path(instances_root, id);
    let raw = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| McError::InstanceNotFound(id.to_string()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub async fn list(instances_root: &Path) -> Result<Vec<Instance>, McError> {
    let mut result = Vec::new();
    if !instances_root.exists() {
        return Ok(result);
    }
    let mut entries = tokio::fs::read_dir(instances_root).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if let Ok(instance) = load(instances_root, &id).await {
            result.push(instance);
        }
    }
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(result)
}

pub async fn delete(instances_root: &Path, id: &str) -> Result<(), McError> {
    let dir = instance_dir(instances_root, id);
    if dir.exists() {
        tokio::fs::remove_dir_all(dir).await?;
    }
    Ok(())
}

pub async fn record_session(instances_root: &Path, id: &str, played_secs: u64) -> Result<(), McError> {
    let mut instance = load(instances_root, id).await?;
    instance.last_played = Some(Utc::now());
    instance.total_playtime_secs += played_secs;
    save(instances_root, &instance).await
}
