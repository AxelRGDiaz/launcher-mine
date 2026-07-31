//! Manifiesto oficial de versiones de Mojang (Piston Meta). Es la única
//! fuente de verdad para qué versiones Vanilla existen y dónde descargarlas.

use super::McError;
use serde::{Deserialize, Serialize};
use std::path::Path;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String, // "release" | "snapshot" | "old_beta" | "old_alpha"
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

/// Descarga el manifiesto y lo cachea localmente. Si la red falla, usa la
/// última copia cacheada para que el launcher siga siendo usable offline
/// para versiones ya vistas antes.
pub async fn fetch_manifest(client: &reqwest::Client, cache_dir: &Path) -> Result<VersionManifest, McError> {
    let cache_path = cache_dir.join("version_manifest_v2.json");

    match fetch_and_cache(client, &cache_path).await {
        Ok(manifest) => Ok(manifest),
        Err(err) => {
            if cache_path.exists() {
                tracing::warn!("no se pudo refrescar el manifiesto de versiones ({err}), usando caché");
                let raw = tokio::fs::read_to_string(&cache_path).await?;
                Ok(serde_json::from_str(&raw)?)
            } else {
                Err(err)
            }
        }
    }
}

async fn fetch_and_cache(client: &reqwest::Client, cache_path: &Path) -> Result<VersionManifest, McError> {
    let manifest: VersionManifest = client.get(MANIFEST_URL).send().await?.error_for_status()?.json().await?;
    if let Some(parent) = cache_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(cache_path, serde_json::to_vec_pretty(&manifest)?).await?;
    Ok(manifest)
}

pub fn find_version<'a>(manifest: &'a VersionManifest, id: &str) -> Option<&'a VersionEntry> {
    manifest.versions.iter().find(|v| v.id == id)
}

pub fn visible_versions(manifest: &VersionManifest, show_snapshots: bool) -> Vec<&VersionEntry> {
    manifest
        .versions
        .iter()
        .filter(|v| show_snapshots || v.version_type == "release")
        .collect()
}
