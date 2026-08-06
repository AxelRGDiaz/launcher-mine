//! OptiFine: a diferencia de Fabric/Quilt/Forge/NeoForge, aquí no se
//! descarga nada — el usuario ya bajó el `.jar` él mismo desde optifine.net
//! (su licencia restringe la redistribución automatizada más que la de
//! Forge, así que no hay un repositorio público al que apuntar).
//!
//! Como tampoco se tiene certeza verificada del formato interno que produce
//! el instalador de OptiFine para versiones recientes de Minecraft (no se
//! descargó ninguna copia para inspeccionarla, por la misma razón de
//! arriba), el enfoque más seguro es **dejar que el propio instalador de
//! OptiFine haga el trabajo real**: se corre `java -jar <archivo>`, lo que
//! abre su ventana de instalación normal, el usuario instala ahí como
//! siempre, y este módulo detecta qué versión nueva apareció en el
//! `.minecraft` estándar del sistema (donde OptiFine instala por defecto) y
//! la copia a la estructura compartida del launcher — reutilizando el mismo
//! parseo de `version.json` genérico que ya se usa para Forge, sin tener
//! que adivinar nada específico de OptiFine.

use super::install::{Arguments, Library, VersionDetail};
use super::{GamePaths, McError};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn imports_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("optifine-imports")
}

/// Copia el `.jar` que el usuario seleccionó a la carpeta propia del
/// launcher, para no tener que volver a pedirlo en futuras instancias de la
/// misma versión. Devuelve el nombre de archivo, que es lo que se guarda
/// como `loader_version` de la instancia.
pub async fn import_file(app_data_dir: &Path, source_path: &Path) -> Result<String, McError> {
    let file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| McError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "nombre de archivo inválido")))?
        .to_string();

    let dest_dir = imports_dir(app_data_dir);
    tokio::fs::create_dir_all(&dest_dir).await?;
    let dest_path = dest_dir.join(&file_name);
    tokio::fs::copy(source_path, &dest_path).await?;
    Ok(file_name)
}

/// Lista los `.jar` ya importados cuyo nombre menciona la versión de
/// Minecraft dada. Heurístico basado en el patrón de nombre habitual de
/// OptiFine (`OptiFine_<version>_HD_U_...jar`), no una garantía — si un
/// archivo no matchea, siempre se puede reimportar señalándolo de nuevo.
pub async fn list_imports(app_data_dir: &Path, minecraft_version: &str) -> Result<Vec<String>, McError> {
    let dir = imports_dir(app_data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let needle = format!("_{minecraft_version}_");
    let mut results = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".jar") && (name.contains(&needle) || name.contains(minecraft_version)) {
            results.push(name);
        }
    }
    results.sort();
    Ok(results)
}

/// Ruta del `.minecraft` real del sistema operativo — la misma convención
/// que usa el launcher oficial de Mojang. Es donde el instalador de
/// OptiFine escribe por defecto; de ahí se copia el resultado hacia la
/// estructura propia del launcher.
fn standard_minecraft_dir() -> Result<PathBuf, McError> {
    let home = dirs::home_dir().ok_or_else(|| {
        McError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "no se pudo resolver el directorio home"))
    })?;
    let path = if cfg!(target_os = "windows") {
        dirs::config_dir().unwrap_or(home).join(".minecraft")
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/minecraft")
    } else {
        home.join(".minecraft")
    };
    Ok(path)
}

async fn ensure_dummy_launcher_profile(minecraft_dir: &Path) -> Result<(), McError> {
    let path = minecraft_dir.join("launcher_profiles.json");
    if path.exists() {
        return Ok(());
    }
    let dummy = serde_json::json!({
        "profiles": {},
        "selectedProfile": null,
        "clientToken": "00000000-0000-0000-0000-000000000000",
        "authenticationDatabase": {},
        "launcherVersion": { "name": "2.1.1", "format": 21 }
    });
    tokio::fs::create_dir_all(minecraft_dir).await?;
    tokio::fs::write(path, serde_json::to_vec_pretty(&dummy)?).await?;
    Ok(())
}

async fn existing_version_ids(minecraft_dir: &Path) -> HashSet<String> {
    let versions_dir = minecraft_dir.join("versions");
    let mut ids = HashSet::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&versions_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                ids.insert(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    ids
}

fn link_path(paths: &GamePaths, minecraft_version: &str, imported_filename: &str) -> PathBuf {
    paths.versions.join(format!(".optifine-{minecraft_version}-{imported_filename}.id"))
}

pub fn is_installed(paths: &GamePaths, minecraft_version: &str, imported_filename: &str) -> bool {
    link_path(paths, minecraft_version, imported_filename).exists()
}

/// Borra el directorio de versión fusionada más el marcador propio. Solo
/// debe llamarse cuando ninguna otra instancia sigue usando este archivo
/// importado — ver `commands::delete_instance`.
pub async fn forget_installation(paths: &GamePaths, minecraft_version: &str, imported_filename: &str) {
    let link = link_path(paths, minecraft_version, imported_filename);
    if let Ok(id) = tokio::fs::read_to_string(&link).await {
        let _ = tokio::fs::remove_dir_all(paths.versions.join(id.trim())).await;
    }
    let _ = tokio::fs::remove_file(&link).await;
}

pub async fn load_cached_detail(
    paths: &GamePaths,
    minecraft_version: &str,
    imported_filename: &str,
) -> Result<VersionDetail, McError> {
    let id = tokio::fs::read_to_string(link_path(paths, minecraft_version, imported_filename)).await?;
    let id = id.trim();
    let raw = tokio::fs::read_to_string(paths.versions.join(id).join(format!("{id}.json"))).await?;
    Ok(serde_json::from_str(&raw)?)
}

/// Igual de parcial que el de Forge: mismo `downloads.artifact` anidado
/// para las librerías, sin `assetIndex`/`downloads` propios porque hereda
/// de la versión Vanilla.
#[derive(Debug, Deserialize)]
struct GenericChildVersionJson {
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

async fn copy_referenced_libraries(
    standard_minecraft_dir: &Path,
    paths: &GamePaths,
    libraries: &[Library],
) -> Result<(), McError> {
    for library in libraries {
        let Some(artifact) = &library.downloads.artifact else { continue };
        let destination = paths.libraries.join(&artifact.path);
        if destination.exists() {
            continue; // ya la tenemos (probablemente compartida con Vanilla)
        }
        let source = standard_minecraft_dir.join("libraries").join(&artifact.path);
        if !source.exists() {
            // Puede ser una librería normal que el propio OptiFine descargó
            // desde Maven y cuyo archivo el instalador no dejó localmente
            // porque ya la tenía cacheada en otro lado — no es fatal, el
            // classpath solo fallará si de verdad falta al lanzar.
            continue;
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&source, &destination).await?;
    }
    Ok(())
}

/// Corre el instalador de OptiFine que el usuario ya importó, detecta la
/// versión nueva que produce en el `.minecraft` estándar del sistema, y la
/// copia/fusiona a la estructura compartida del launcher.
///
/// Nota: como es una GUI, este `await` no vuelve hasta que el usuario cierra
/// la ventana del instalador — el `on_progress` avisa esto antes de correrlo
/// para que la UI no parezca colgada.
pub async fn install(
    http: &reqwest::Client,
    downloads: &crate::download::DownloadManager,
    app_data_dir: &Path,
    paths: &GamePaths,
    java_path: &Path,
    minecraft_version: &str,
    imported_filename: &str,
    on_progress: crate::download::ProgressCallback,
) -> Result<VersionDetail, McError> {
    let parent = super::install::install_vanilla(http, downloads, paths, minecraft_version, on_progress.clone()).await?;

    let jar_path = imports_dir(app_data_dir).join(imported_filename);
    if !jar_path.exists() {
        return Err(McError::InstallerFailed(format!(
            "no se encontró el archivo importado {imported_filename}; vuelve a importarlo"
        )));
    }

    let minecraft_dir = standard_minecraft_dir()?;
    tokio::fs::create_dir_all(minecraft_dir.join("versions")).await?;
    tokio::fs::create_dir_all(minecraft_dir.join("libraries")).await?;
    ensure_dummy_launcher_profile(&minecraft_dir).await?;

    let before = existing_version_ids(&minecraft_dir).await;

    on_progress(crate::download::DownloadProgress {
        label: "Se abrió el instalador de OptiFine — instala ahí y cierra la ventana cuando termine.".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        completed_files: 0,
        total_files: 0,
    });

    let mut cmd = tokio::process::Command::new(java_path);
    cmd.arg("-jar").arg(&jar_path);
    crate::process_ext::hide_console(&mut cmd);
    let mut child = cmd.spawn().map_err(McError::Io)?;
    child.wait().await.map_err(McError::Io)?;

    let after = existing_version_ids(&minecraft_dir).await;
    let new_id = after
        .difference(&before)
        .next()
        .ok_or_else(|| McError::InstallerFailed(
            "no se detectó ninguna versión nueva — ¿llegaste a darle a \"Install\" en la ventana de OptiFine?".to_string(),
        ))?
        .clone();

    let version_json_path = minecraft_dir.join("versions").join(&new_id).join(format!("{new_id}.json"));
    let child_raw = tokio::fs::read_to_string(&version_json_path).await?;
    let child: GenericChildVersionJson = serde_json::from_str(&child_raw)?;

    copy_referenced_libraries(&minecraft_dir, paths, &child.libraries).await?;

    let merged = super::install::merge_with_parent(
        &parent,
        child.id,
        child.main_class,
        child.libraries,
        child.arguments,
        child.legacy_arguments,
    );

    let dest_version_dir = paths.versions.join(&merged.id);
    tokio::fs::create_dir_all(&dest_version_dir).await?;
    tokio::fs::write(dest_version_dir.join(format!("{}.json", merged.id)), serde_json::to_vec_pretty(&merged)?).await?;

    tokio::fs::write(link_path(paths, minecraft_version, imported_filename), merged.id.as_bytes()).await?;

    Ok(merged)
}
