//! Fabric y Quilt exponen APIs "meta" con la misma forma (Quilt nació como
//! fork de Fabric y mantiene el formato), así que un solo módulo cubre
//! ambos, parametrizado por URL base. Verificado contra las respuestas
//! reales de `meta.fabricmc.net`/`meta.quiltmc.org` antes de implementar.
//!
//! A diferencia de Forge/NeoForge (que requieren correr su instalador
//! oficial), Fabric/Quilt publican un JSON de lanzamiento ya resuelto
//! (`/profile/json`) pensado para que launchers de terceros lo consuman
//! directamente — no hay nada que "instalar" en el sentido de ejecutar un
//! programa, solo descargar las librerías del loader y fusionar ese JSON
//! con la versión Vanilla de la que depende (`inheritsFrom`).

use super::install::{Arguments, DownloadArtifactPath, Library, LibraryDownloads, VersionDetail};
use super::{GamePaths, McError};
use crate::download::{DownloadManager, DownloadRequest, ProgressCallback};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderFlavor {
    Fabric,
    Quilt,
}

impl LoaderFlavor {
    fn meta_base(self) -> &'static str {
        match self {
            LoaderFlavor::Fabric => "https://meta.fabricmc.net/v2",
            LoaderFlavor::Quilt => "https://meta.quiltmc.org/v3",
        }
    }

    fn cache_slug(self) -> &'static str {
        match self {
            LoaderFlavor::Fabric => "fabric",
            LoaderFlavor::Quilt => "quilt",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersionEntry {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct LoaderListEntry {
    loader: LoaderVersionInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderVersionInfo {
    version: String,
    #[serde(default)]
    stable: bool,
}

/// Lista las versiones del loader disponibles para una versión de Minecraft
/// concreta (la API de Fabric/Quilt es específica por versión de juego).
pub async fn list_loader_versions(
    client: &reqwest::Client,
    flavor: LoaderFlavor,
    minecraft_version: &str,
) -> Result<Vec<LoaderVersionEntry>, McError> {
    let url = format!("{}/versions/loader/{minecraft_version}", flavor.meta_base());
    let entries: Vec<LoaderListEntry> = client.get(&url).send().await?.error_for_status()?.json().await?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersionEntry {
            version: e.loader.version,
            stable: e.loader.stable,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct LoaderProfile {
    id: String,
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    arguments: Option<Arguments>,
    libraries: Vec<FlatLibrary>,
}

/// Forma "plana" de las librerías en el JSON de Fabric/Quilt — nada que ver
/// con el `downloads.artifact.{path,url,sha1,size}` anidado de Mojang. La
/// ruta de descarga hay que derivarla de la coordenada Maven (`name`), y el
/// hash puede faltar (pasa siempre con el propio jar del loader y con
/// intermediary/hashed-mappings): en ese caso se descarga sin verificar.
#[derive(Debug, Deserialize)]
struct FlatLibrary {
    name: String,
    url: String,
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

fn maven_coordinate_to_path(coordinate: &str) -> Option<String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    let group_path = group.replace('.', "/");
    Some(format!("{group_path}/{artifact}/{version}/{artifact}-{version}{classifier}.jar"))
}

fn flat_library_to_mojang(lib: &FlatLibrary) -> Option<Library> {
    let path = maven_coordinate_to_path(&lib.name)?;
    let url = format!("{}{path}", lib.url);
    Some(Library {
        name: lib.name.clone(),
        downloads: LibraryDownloads {
            artifact: Some(DownloadArtifactPath {
                path,
                url,
                sha1: lib.sha1.clone().unwrap_or_default(),
                size: lib.size.unwrap_or(0),
            }),
            classifiers: None,
        },
        rules: Vec::new(),
        natives: None,
    })
}

fn cache_key(flavor: LoaderFlavor, minecraft_version: &str, loader_version: &str) -> String {
    format!("{}-{minecraft_version}-{loader_version}", flavor.cache_slug())
}

pub fn is_installed(paths: &GamePaths, flavor: LoaderFlavor, minecraft_version: &str, loader_version: &str) -> bool {
    paths
        .versions
        .join(cache_key(flavor, minecraft_version, loader_version))
        .join(".installed")
        .exists()
}

/// Borra el directorio de esta instalación (perfil fusionado + marcador).
/// Solo debe llamarse cuando ninguna otra instancia sigue usando este
/// loader/versión — ver `commands::delete_instance`.
pub async fn forget_installation(paths: &GamePaths, flavor: LoaderFlavor, minecraft_version: &str, loader_version: &str) {
    let dir = paths.versions.join(cache_key(flavor, minecraft_version, loader_version));
    let _ = tokio::fs::remove_dir_all(dir).await;
}

/// Carga el `VersionDetail` fusionado ya cacheado en disco (creado por
/// `install`). No vuelve a pegarle a la red — asume que ya se instaló.
pub async fn load_cached_detail(
    paths: &GamePaths,
    flavor: LoaderFlavor,
    minecraft_version: &str,
    loader_version: &str,
) -> Result<VersionDetail, McError> {
    let dir = paths.versions.join(cache_key(flavor, minecraft_version, loader_version));
    let raw = tokio::fs::read_to_string(dir.join("profile.json")).await?;
    Ok(serde_json::from_str(&raw)?)
}

/// Instala Fabric/Quilt para `minecraft_version`: primero asegura la versión
/// Vanilla de la que depende (`inheritsFrom`, idempotente si ya está),
/// descarga las librerías propias del loader, y fusiona ambos JSON en un
/// único `VersionDetail` cacheado — el resto del pipeline (classpath,
/// natives, lanzamiento) no necesita saber que existe Fabric/Quilt.
pub async fn install(
    http: &reqwest::Client,
    downloads: &DownloadManager,
    paths: &GamePaths,
    flavor: LoaderFlavor,
    minecraft_version: &str,
    loader_version: &str,
    on_progress: ProgressCallback,
) -> Result<VersionDetail, McError> {
    let parent =
        super::install::install_vanilla(http, downloads, paths, minecraft_version, on_progress.clone()).await?;

    let profile_url = format!(
        "{}/versions/loader/{minecraft_version}/{loader_version}/profile/json",
        flavor.meta_base()
    );
    let profile: LoaderProfile = http.get(&profile_url).send().await?.error_for_status()?.json().await?;

    let loader_libraries: Vec<Library> = profile.libraries.iter().filter_map(flat_library_to_mojang).collect();

    let merged = super::install::merge_with_parent(
        &parent,
        profile.id.clone(),
        profile.main_class.clone(),
        loader_libraries.clone(),
        profile.arguments.clone(),
        None,
    );

    let requests: Vec<DownloadRequest> = loader_libraries
        .iter()
        .filter_map(|lib| {
            lib.downloads.artifact.as_ref().map(|artifact| DownloadRequest {
                url: artifact.url.clone(),
                destination: paths.libraries.join(&artifact.path),
                expected_sha1: if artifact.sha1.is_empty() {
                    None
                } else {
                    Some(artifact.sha1.clone())
                },
                label: lib.name.clone(),
            })
        })
        .collect();
    downloads.fetch_many(requests, &paths.cache, on_progress).await?;

    let dir = paths.versions.join(cache_key(flavor, minecraft_version, loader_version));
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join("profile.json"), serde_json::to_vec_pretty(&merged)?).await?;
    tokio::fs::write(dir.join(".installed"), b"ok").await?;

    Ok(merged)
}
