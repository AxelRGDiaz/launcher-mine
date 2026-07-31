//! Instalación de una versión Vanilla: descarga el client.jar, las librerías
//! (con reglas por SO) y los assets, todo verificado por SHA1 contra el JSON
//! oficial de la versión.

use super::{manifest, GamePaths, McError};
use crate::download::{DownloadManager, DownloadRequest, ProgressCallback};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionDetail {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    pub assets: String,
    pub downloads: VersionDownloads,
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersionRef>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments", default)]
    pub legacy_arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JavaVersionRef {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionDownloads {
    pub client: DownloadArtifact,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DownloadArtifact {
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Library {
    pub name: String,
    pub downloads: LibraryDownloads,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadArtifactPath>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, DownloadArtifactPath>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DownloadArtifactPath {
    pub path: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OsRule {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

/// true si no hay reglas (siempre aplica) o si la última regla que matchea el
/// SO actual es "allow" — algoritmo oficial de Mojang para libraries/args.
pub fn rule_applies(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_matches = match &rule.os {
            None => true,
            Some(os) => os
                .name
                .as_deref()
                .map(|n| n == super::current_os_name())
                .unwrap_or(true),
        };
        if os_matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

async fn fetch_version_detail(
    client: &reqwest::Client,
    url: &str,
    cache_path: &Path,
) -> Result<VersionDetail, McError> {
    let detail: VersionDetail = client.get(url).send().await?.error_for_status()?.json().await?;
    if let Some(parent) = cache_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(cache_path, serde_json::to_vec_pretty(&detail)?).await?;
    Ok(detail)
}

pub async fn load_version_detail(
    client: &reqwest::Client,
    paths: &GamePaths,
    version_id: &str,
    manifest_cache_dir: &Path,
) -> Result<VersionDetail, McError> {
    let version_manifest = manifest::fetch_manifest(client, manifest_cache_dir).await?;
    let entry = manifest::find_version(&version_manifest, version_id)
        .ok_or_else(|| McError::UnknownVersion(version_id.to_string()))?;

    let cache_path = paths.versions.join(version_id).join(format!("{version_id}.json"));
    if cache_path.exists() {
        let raw = tokio::fs::read_to_string(&cache_path).await?;
        if let Ok(detail) = serde_json::from_str(&raw) {
            return Ok(detail);
        }
    }
    fetch_version_detail(client, &entry.url, &cache_path).await
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetObject {
    hash: String,
    // Presente en el índice oficial de Mojang; no se usa todavía (sin barra
    // de progreso por bytes totales de assets), se deja para no perder el
    // campo al deserializar y por si se usa para precalcular el tamaño total.
    #[allow(dead_code)]
    size: u64,
}

/// Descarga e instala todo lo necesario para lanzar `version_id` en Vanilla:
/// client.jar, librerías aplicables al SO actual y el índice de assets con
/// sus objetos. Todo verificado por SHA1 contra el JSON oficial de Mojang.
pub async fn install_vanilla(
    http: &reqwest::Client,
    downloads: &DownloadManager,
    paths: &GamePaths,
    version_id: &str,
    on_progress: ProgressCallback,
) -> Result<VersionDetail, McError> {
    let detail = load_version_detail(http, paths, version_id, &paths.cache).await?;

    let version_dir = paths.versions.join(&detail.id);
    tokio::fs::create_dir_all(&version_dir).await?;

    let client_jar_path = version_dir.join(format!("{}.jar", detail.id));
    let mut requests = vec![DownloadRequest {
        url: detail.downloads.client.url.clone(),
        destination: client_jar_path,
        expected_sha1: Some(detail.downloads.client.sha1.clone()),
        label: format!("{} (client.jar)", detail.id),
    }];

    for library in &detail.libraries {
        if !rule_applies(&library.rules) {
            continue;
        }
        if let Some(artifact) = &library.downloads.artifact {
            requests.push(DownloadRequest {
                url: artifact.url.clone(),
                destination: paths.libraries.join(&artifact.path),
                expected_sha1: Some(artifact.sha1.clone()),
                label: library.name.clone(),
            });
        }
        if let (Some(natives_map), Some(classifiers)) = (&library.natives, &library.downloads.classifiers) {
            if let Some(classifier_key) = natives_map.get(super::current_os_name()) {
                if let Some(artifact) = classifiers.get(classifier_key) {
                    requests.push(DownloadRequest {
                        url: artifact.url.clone(),
                        destination: paths.libraries.join(&artifact.path),
                        expected_sha1: Some(artifact.sha1.clone()),
                        label: format!("{} (natives)", library.name),
                    });
                }
            }
        }
    }

    downloads.fetch_many(requests, &paths.cache, on_progress.clone()).await?;

    install_assets(http, downloads, paths, &detail, on_progress).await?;

    tokio::fs::write(version_dir.join(".installed"), b"ok").await?;
    Ok(detail)
}

async fn install_assets(
    http: &reqwest::Client,
    downloads: &DownloadManager,
    paths: &GamePaths,
    detail: &VersionDetail,
    on_progress: ProgressCallback,
) -> Result<(), McError> {
    let index_path = paths.assets.join("indexes").join(format!("{}.json", detail.asset_index.id));

    let index_bytes = if index_path.exists() {
        tokio::fs::read(&index_path).await?
    } else {
        let bytes = http
            .get(&detail.asset_index.url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        if let Some(parent) = index_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&index_path, &bytes).await?;
        bytes.to_vec()
    };

    let index: AssetIndex = serde_json::from_slice(&index_bytes)?;
    let objects_dir = paths.assets.join("objects");

    // Muchos nombres virtuales distintos comparten el mismo contenido (mismo
    // hash) — p.ej. varios idiomas sin traducir apuntando al mismo JSON. Como
    // el destino ya está direccionado por hash, basta con encolar cada hash
    // una sola vez (el `DownloadManager` además serializa por destino como
    // red de seguridad, pero deduplicar aquí evita trabajo redundante).
    let mut seen_hashes = std::collections::HashSet::new();
    let requests: Vec<DownloadRequest> = index
        .objects
        .into_iter()
        .filter(|(_, object)| seen_hashes.insert(object.hash.clone()))
        .map(|(name, object)| {
            let hash_prefix = &object.hash[0..2];
            DownloadRequest {
                url: format!("https://resources.download.minecraft.net/{hash_prefix}/{}", object.hash),
                destination: objects_dir.join(hash_prefix).join(&object.hash),
                expected_sha1: Some(object.hash.clone()),
                label: name,
            }
        })
        .collect();

    downloads.fetch_many(requests, &paths.cache, on_progress).await?;
    Ok(())
}

pub fn is_installed(paths: &GamePaths, version_id: &str) -> bool {
    paths.versions.join(version_id).join(".installed").exists()
}

/// Fusiona un `VersionDetail` "hijo" que declara `inheritsFrom` pero no trae
/// `assetIndex`/`downloads`/`assets` propios — el caso de Fabric, Quilt,
/// Forge y NeoForge, todos apoyados en la versión Vanilla de la que
/// dependen. Usado por `fabric_like` y `forge_like` para no duplicar esta
/// lógica.
pub fn merge_with_parent(
    parent: &VersionDetail,
    child_id: String,
    child_main_class: String,
    child_libraries: Vec<Library>,
    child_arguments: Option<Arguments>,
    child_legacy_arguments: Option<String>,
) -> VersionDetail {
    let merged_arguments = match &parent.arguments {
        Some(parent_args) => {
            let child_args = child_arguments.unwrap_or_default();
            Some(Arguments {
                game: [parent_args.game.clone(), child_args.game].concat(),
                jvm: [parent_args.jvm.clone(), child_args.jvm].concat(),
            })
        }
        None => child_arguments,
    };

    VersionDetail {
        id: child_id,
        main_class: child_main_class,
        asset_index: parent.asset_index.clone(),
        assets: parent.assets.clone(),
        downloads: parent.downloads.clone(),
        libraries: [parent.libraries.clone(), child_libraries].concat(),
        java_version: parent.java_version.clone(),
        arguments: merged_arguments,
        legacy_arguments: child_legacy_arguments.or_else(|| parent.legacy_arguments.clone()),
    }
}
