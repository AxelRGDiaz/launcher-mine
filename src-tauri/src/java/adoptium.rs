//! Cliente de la API pública de Eclipse Adoptium (Temurin) para descargar e
//! instalar automáticamente un runtime de Java compatible cuando el sistema
//! no tiene ninguno. Ver https://api.adoptium.net/q/swagger-ui/.

use crate::download::{DownloadProgress, ProgressCallback};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const API_BASE: &str = "https://api.adoptium.net/v3";

#[derive(thiserror::Error, Debug)]
pub enum JavaError {
    #[error("no se encontró un runtime Temurin para Java {major} ({os}/{arch})")]
    NoReleaseAvailable { major: u32, os: String, arch: String },
    #[error("error de red: {0}")]
    Network(#[from] reqwest::Error),
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("checksum SHA256 no coincide para el runtime de Java {major}")]
    ChecksumMismatch { major: u32 },
    #[error("no se pudo localizar el ejecutable java dentro del runtime extraído")]
    ExecutableNotFound,
    #[error("error extrayendo el archivo descargado: {0}")]
    Extract(String),
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinaryInfo,
    version: AdoptiumVersion,
}

#[derive(Debug, Deserialize)]
struct AdoptiumVersion {
    semver: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinaryInfo {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
    name: String,
}

fn os_identifier() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

fn arch_identifier() -> &'static str {
    crate::java::current_os_arch()
}

async fn fetch_latest_asset(client: &reqwest::Client, major: u32) -> Result<AdoptiumAsset, JavaError> {
    let url = format!(
        "{API_BASE}/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse",
        arch = arch_identifier(),
        os = os_identifier(),
    );
    let assets: Vec<AdoptiumAsset> = client
        .get(&url)
        .header("User-Agent", concat!("MiLauncher/", env!("CARGO_PKG_VERSION")))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    assets.into_iter().next().ok_or_else(|| JavaError::NoReleaseAvailable {
        major,
        os: os_identifier().to_string(),
        arch: arch_identifier().to_string(),
    })
}

/// Descarga e instala el runtime Temurin `major` en `dest_root/<major>-<version>/`,
/// devolviendo la ruta al ejecutable `java` listo para usar.
pub async fn install(
    client: &reqwest::Client,
    major: u32,
    dest_root: &Path,
    on_progress: ProgressCallback,
) -> Result<PathBuf, JavaError> {
    let asset = fetch_latest_asset(client, major).await?;
    let package = asset.binary.package;

    tokio::fs::create_dir_all(dest_root).await?;
    let archive_path = dest_root.join(&package.name);
    download_with_progress(client, &package.link, &archive_path, &package.checksum, major, &on_progress).await?;

    let install_dir = dest_root.join(format!("{major}-{}", asset.version.semver.replace(['+', '/'], "_")));
    if install_dir.exists() {
        tokio::fs::remove_dir_all(&install_dir).await.ok();
    }
    tokio::fs::create_dir_all(&install_dir).await?;

    extract_archive(&archive_path, &install_dir).await?;
    let _ = tokio::fs::remove_file(&archive_path).await;

    find_java_executable(&install_dir).ok_or(JavaError::ExecutableNotFound)
}

async fn download_with_progress(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    expected_sha256: &str,
    major: u32,
    on_progress: &ProgressCallback,
) -> Result<(), JavaError> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let total_bytes = response.content_length();

    let mut file = tokio::fs::File::create(destination).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;

    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_progress(DownloadProgress {
            label: format!("Java {major} (Temurin)"),
            downloaded_bytes: downloaded,
            total_bytes,
            completed_files: 0,
            total_files: 0,
        });
    }
    file.flush().await?;

    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = tokio::fs::remove_file(destination).await;
        return Err(JavaError::ChecksumMismatch { major });
    }
    Ok(())
}

async fn extract_archive(archive_path: &Path, install_dir: &Path) -> Result<(), JavaError> {
    let archive_path = archive_path.to_path_buf();
    let install_dir = install_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_archive_blocking(&archive_path, &install_dir))
        .await
        .map_err(|e| JavaError::Extract(e.to_string()))?
}

fn extract_archive_blocking(archive_path: &Path, install_dir: &Path) -> Result<(), JavaError> {
    let file = std::fs::File::open(archive_path)?;
    let name = archive_path.to_string_lossy();

    if name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(file).map_err(|e| JavaError::Extract(e.to_string()))?;
        archive
            .extract(install_dir)
            .map_err(|e| JavaError::Extract(e.to_string()))?;
    } else {
        // .tar.gz (macOS / Linux)
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(install_dir)?;
    }
    Ok(())
}

/// Los runtimes de Adoptium se extraen dentro de una carpeta raíz propia
/// (p.ej. `jdk-21.0.1+12-jre`); en macOS además envuelven `Contents/Home`.
/// Buscamos `bin/java(.exe)` unos niveles hacia abajo en vez de asumir la ruta.
fn find_java_executable(root: &Path) -> Option<PathBuf> {
    let target_name = if cfg!(target_os = "windows") { "java.exe" } else { "java" };
    let mut stack = vec![root.to_path_buf()];
    let mut depth = 0;
    while let Some(dir) = stack.pop() {
        depth += 1;
        if depth > 5000 {
            break; // salvaguarda contra árboles inesperadamente grandes
        }
        let bin_candidate = dir.join("bin").join(target_name);
        if bin_candidate.is_file() {
            return Some(bin_candidate);
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    stack.push(entry.path());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
pub fn windows_registry_java_homes() -> Vec<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;

    let mut homes = Vec::new();
    let roots = [
        ("SOFTWARE\\JavaSoft\\JDK", HKEY_LOCAL_MACHINE),
        ("SOFTWARE\\JavaSoft\\JRE", HKEY_LOCAL_MACHINE),
        ("SOFTWARE\\Eclipse Adoptium\\JDK", HKEY_LOCAL_MACHINE),
        ("SOFTWARE\\Eclipse Adoptium\\JRE", HKEY_LOCAL_MACHINE),
    ];
    for (subkey, hive) in roots {
        let base = RegKey::predef(hive);
        if let Ok(key) = base.open_subkey(subkey) {
            for version in key.enum_keys().flatten() {
                if let Ok(version_key) = key.open_subkey(&version) {
                    if let Ok(home) = version_key.get_value::<String, _>("JavaHome") {
                        homes.push(PathBuf::from(home));
                    }
                }
            }
        }
    }
    homes
}
