//! Forge y NeoForge, a diferencia de Fabric/Quilt, no publican un JSON de
//! lanzamiento ya resuelto: parte de lo necesario (el client.jar parcheado)
//! no existe en ningún sitio descargable — lo genera localmente el propio
//! instalador oficial aplicando parches binarios sobre el client.jar
//! Vanilla. Por eso aquí NO se reimplementa esa lógica de parcheo: se
//! invoca `java -jar forge-installer.jar --installClient <dir>`, exactamente
//! como hacen MultiMC/Prism Launcher/ATLauncher.
//!
//! Todo esto se verificó contra un instalador real antes de escribir el
//! código (descargando `forge-1.21.1-52.1.16-installer.jar` y ejecutándolo).
//!
//! **Nota legal/ética importante**: el propio `install_profile.json` del
//! instalador de Forge incluye este comentario textual:
//! *"Please do not automate the download and installation of Forge. Our
//! efforts are supported by ads from the download page."* No hay forma de
//! automatizar una instalación de Forge respetando eso al pie de la letra.
//! La práctica estándar entre launchers de código abierto (MultiMC, Prism,
//! ATLauncher) es descargar desde el repositorio Maven público de Forge
//! (nunca desde la página con anuncios) — es lo que se hace aquí también,
//! pero es una tensión real con lo que pide el mantenedor, no una zona
//! legalmente gris resuelta: quien distribuya este launcher debe saber que
//! existe este desacuerdo explícito.
//!
//! El instalador además exige que el directorio destino tenga un
//! `launcher_profiles.json` (revisa que "parezca" un `.minecraft` real); se
//! crea uno mínimo sintético si no existe, igual que MultiMC/Prism.

use super::install::{Arguments, Library, VersionDetail};
use super::{GamePaths, McError};
use crate::download::{DownloadManager, DownloadRequest, ProgressCallback};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeFlavor {
    Forge,
    NeoForge,
}

impl ForgeFlavor {
    fn maven_base(self) -> &'static str {
        match self {
            ForgeFlavor::Forge => "https://maven.minecraftforge.net",
            ForgeFlavor::NeoForge => "https://maven.neoforged.net/releases",
        }
    }

    fn group_artifact_path(self) -> &'static str {
        match self {
            ForgeFlavor::Forge => "net/minecraftforge/forge",
            ForgeFlavor::NeoForge => "net/neoforged/neoforge",
        }
    }

    fn installer_artifact(self) -> &'static str {
        match self {
            ForgeFlavor::Forge => "forge",
            ForgeFlavor::NeoForge => "neoforge",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            ForgeFlavor::Forge => "forge",
            ForgeFlavor::NeoForge => "neoforge",
        }
    }
}

/// Convierte `(minecraft_version, loader_version)` en la coordenada Maven
/// completa que usa cada proyecto: Forge prefija con la versión de
/// Minecraft (`1.21.1-52.1.16`), NeoForge no (`21.1.238`).
fn maven_version_string(flavor: ForgeFlavor, minecraft_version: &str, loader_version: &str) -> String {
    match flavor {
        ForgeFlavor::Forge => format!("{minecraft_version}-{loader_version}"),
        ForgeFlavor::NeoForge => loader_version.to_string(),
    }
}

/// NeoForge nombra sus versiones `<minor>.<patch>.<build>[-beta]` a partir
/// de Minecraft `1.<minor>.<patch>` (Minecraft 1.21.1 -> prefijo "21.1.").
fn neoforge_prefix(minecraft_version: &str) -> String {
    let stripped = minecraft_version.strip_prefix("1.").unwrap_or(minecraft_version);
    format!("{stripped}.")
}

fn extract_xml_tag_values(xml: &str, tag: &str) -> Vec<String> {
    let open_tag = format!("<{tag}>");
    let close_tag = format!("</{tag}>");
    let mut results = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open_tag) {
        rest = &rest[start + open_tag.len()..];
        let Some(end) = rest.find(&close_tag) else { break };
        results.push(rest[..end].to_string());
        rest = &rest[end + close_tag.len()..];
    }
    results
}

/// Lista las versiones del loader publicadas para una versión de Minecraft,
/// leyendo el `maven-metadata.xml` del repositorio correspondiente (no hay
/// una API JSON como la de Fabric/Quilt). El resultado ya viene sin el
/// prefijo de versión de Minecraft, para que la UI luzca igual que con
/// Fabric/Quilt.
pub async fn list_versions(
    client: &reqwest::Client,
    flavor: ForgeFlavor,
    minecraft_version: &str,
) -> Result<Vec<String>, McError> {
    let url = format!(
        "{}/{}/maven-metadata.xml",
        flavor.maven_base(),
        flavor.group_artifact_path()
    );
    let xml = client.get(&url).send().await?.error_for_status()?.text().await?;
    let all_versions = extract_xml_tag_values(&xml, "version");

    let matches: Vec<String> = match flavor {
        ForgeFlavor::Forge => {
            let prefix = format!("{minecraft_version}-");
            all_versions
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .map(|v| v.trim_start_matches(&prefix).to_string())
                .collect()
        }
        ForgeFlavor::NeoForge => {
            let prefix = neoforge_prefix(minecraft_version);
            all_versions.into_iter().filter(|v| v.starts_with(&prefix)).collect()
        }
    };

    // El metadata no garantiza orden ascendente estricto por versión de
    // Minecraft; invertir el orden de publicación es un heurístico
    // razonable para mostrar las más recientes primero, no una garantía.
    let mut matches = matches;
    matches.reverse();
    Ok(matches)
}

fn link_path(paths: &GamePaths, flavor: ForgeFlavor, minecraft_version: &str, loader_version: &str) -> PathBuf {
    paths
        .versions
        .join(format!(".{}-{minecraft_version}-{loader_version}.id", flavor.slug()))
}

pub fn is_installed(paths: &GamePaths, flavor: ForgeFlavor, minecraft_version: &str, loader_version: &str) -> bool {
    link_path(paths, flavor, minecraft_version, loader_version).exists()
}

/// Borra el directorio de versión fusionada que produjo el instalador, más
/// el marcador propio. Solo debe llamarse cuando ninguna otra instancia
/// sigue usando este loader/versión — ver `commands::delete_instance`.
pub async fn forget_installation(paths: &GamePaths, flavor: ForgeFlavor, minecraft_version: &str, loader_version: &str) {
    let link = link_path(paths, flavor, minecraft_version, loader_version);
    if let Ok(id) = tokio::fs::read_to_string(&link).await {
        let _ = tokio::fs::remove_dir_all(paths.versions.join(id.trim())).await;
    }
    let _ = tokio::fs::remove_file(&link).await;
}

pub async fn load_cached_detail(
    paths: &GamePaths,
    flavor: ForgeFlavor,
    minecraft_version: &str,
    loader_version: &str,
) -> Result<VersionDetail, McError> {
    let id = tokio::fs::read_to_string(link_path(paths, flavor, minecraft_version, loader_version)).await?;
    let id = id.trim();
    let raw = tokio::fs::read_to_string(paths.versions.join(id).join(format!("{id}.json"))).await?;
    Ok(serde_json::from_str(&raw)?)
}

/// Forma parcial del `version.json` que produce el instalador: igual que el
/// oficial de Mojang para las librerías (mismo `downloads.artifact` anidado
/// — se reutiliza `Library` tal cual), pero sin `assetIndex`/`downloads`
/// propios porque hereda de la versión Vanilla (`inheritsFrom`).
#[derive(Debug, Deserialize)]
struct ForgeVersionJson {
    id: String,
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments", default)]
    legacy_arguments: Option<String>,
    #[serde(default)]
    libraries: Vec<Library>,
}

async fn ensure_dummy_launcher_profile(app_data_dir: &Path) -> Result<(), McError> {
    let path = app_data_dir.join("launcher_profiles.json");
    if path.exists() {
        return Ok(());
    }
    // El instalador solo comprueba que el archivo exista y sea JSON válido
    // con esta forma — no le importa el contenido real de las cuentas.
    let dummy = serde_json::json!({
        "profiles": {},
        "selectedProfile": null,
        "clientToken": "00000000-0000-0000-0000-000000000000",
        "authenticationDatabase": {},
        "launcherVersion": { "name": "2.1.1", "format": 21 }
    });
    tokio::fs::create_dir_all(app_data_dir).await?;
    tokio::fs::write(path, serde_json::to_vec_pretty(&dummy)?).await?;
    Ok(())
}

/// Lee `install_profile.json` de dentro del propio jar del instalador para
/// saber con certeza qué id de versión va a producir — más robusto que
/// asumir un patrón de nombre fijo (varía entre eras de Forge).
async fn read_resulting_version_id(installer_path: &Path) -> Result<String, McError> {
    let installer_path = installer_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<String, McError> {
        let file = std::fs::File::open(&installer_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let mut entry = archive
            .by_name("install_profile.json")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut entry, &mut contents)?;
        let value: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        value
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "install_profile.json sin campo 'version'")
            })
            .map_err(McError::Io)
    })
    .await
    .map_err(|e| McError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}

async fn run_installer(java_path: &Path, installer_path: &Path, target_dir: &Path) -> Result<(), McError> {
    let mut cmd = tokio::process::Command::new(java_path);
    cmd.arg("-jar").arg(installer_path).arg("--installClient").arg(target_dir);
    crate::process_ext::hide_console(&mut cmd);
    let output = cmd.output().await.map_err(McError::Io)?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let last_line = stdout.trim().lines().last().unwrap_or("error desconocido durante la instalación");
        return Err(McError::InstallerFailed(last_line.to_string()));
    }
    Ok(())
}

/// Instala Forge/NeoForge para `minecraft_version`: asegura la versión
/// Vanilla de la que depende, descarga el instalador oficial desde el
/// repositorio Maven público, y lo ejecuta con `--installClient` apuntando
/// al mismo directorio de datos que usan las librerías/versiones
/// compartidas — así el resultado queda exactamente donde el resto del
/// pipeline (classpath, natives, lanzamiento) ya sabe buscar, sin cambios.
pub async fn install(
    http: &reqwest::Client,
    downloads: &DownloadManager,
    app_data_dir: &Path,
    paths: &GamePaths,
    java_path: &Path,
    flavor: ForgeFlavor,
    minecraft_version: &str,
    loader_version: &str,
    on_progress: ProgressCallback,
) -> Result<VersionDetail, McError> {
    let parent =
        super::install::install_vanilla(http, downloads, paths, minecraft_version, on_progress.clone()).await?;

    let maven_version = maven_version_string(flavor, minecraft_version, loader_version);
    let artifact = flavor.installer_artifact();
    let installer_url = format!(
        "{}/{}/{maven_version}/{artifact}-{maven_version}-installer.jar",
        flavor.maven_base(),
        flavor.group_artifact_path()
    );
    let installer_path = paths
        .cache
        .join("installers")
        .join(format!("{}-{maven_version}-installer.jar", flavor.slug()));

    downloads
        .fetch(
            &DownloadRequest {
                url: installer_url,
                destination: installer_path.clone(),
                // Forge/NeoForge no publican un SHA1 del propio instalador
                // en un índice consultable (a diferencia de las librerías
                // que sí traen checksum en el version.json resultante).
                expected_sha1: None,
                label: format!("Instalador de {} {maven_version}", flavor.slug()),
            },
            &paths.cache,
            &on_progress,
        )
        .await?;

    ensure_dummy_launcher_profile(app_data_dir).await?;
    run_installer(java_path, &installer_path, app_data_dir).await?;

    let version_id = read_resulting_version_id(&installer_path).await?;
    let version_json_path = paths.versions.join(&version_id).join(format!("{version_id}.json"));
    let child_raw = tokio::fs::read_to_string(&version_json_path).await?;
    let child: ForgeVersionJson = serde_json::from_str(&child_raw)?;

    let merged = super::install::merge_with_parent(
        &parent,
        child.id,
        child.main_class,
        child.libraries,
        child.arguments,
        child.legacy_arguments,
    );

    tokio::fs::write(&version_json_path, serde_json::to_vec_pretty(&merged)?).await?;
    tokio::fs::write(
        link_path(paths, flavor, minecraft_version, loader_version),
        version_id.as_bytes(),
    )
    .await?;

    Ok(merged)
}
