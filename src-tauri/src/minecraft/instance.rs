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
        loader_version: None,
        min_ram_mb: None,
        max_ram_mb: None,
        extra_jvm_args: None,
        created_at: Utc::now(),
        last_played: None,
        total_playtime_secs: 0,
    };

    tokio::fs::create_dir_all(minecraft_dir(instances_root, &id)).await?;
    save(instances_root, &instance).await?;
    Ok(instance)
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
