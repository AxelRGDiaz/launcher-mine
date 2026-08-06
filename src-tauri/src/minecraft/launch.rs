//! Construcción de classpath/argumentos y lanzamiento real del proceso Java
//! para una instancia. Soporta tanto el formato moderno de argumentos
//! (`arguments.jvm` / `arguments.game`, 1.13+) como el legado
//! (`minecraftArguments`, versiones anteriores).

use super::install::{rule_applies, DownloadArtifactPath, VersionDetail};
use super::instance::Instance;
use super::{GamePaths, McError};
use crate::accounts::{Account, AccountKind};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};

pub struct LaunchRequest<'a> {
    pub instance: &'a Instance,
    pub detail: &'a VersionDetail,
    pub java_path: &'a Path,
    pub account: &'a Account,
    pub default_min_ram_mb: u32,
    pub default_max_ram_mb: u32,
    pub launcher_name: String,
    pub launcher_version: String,
    /// Texto que Minecraft muestra junto a "Minecraft <versión>" en la
    /// esquina del menú principal — separado del nombre del launcher porque
    /// suele convenir más corto.
    pub version_type_label: String,
}

pub async fn launch(paths: &GamePaths, req: LaunchRequest<'_>) -> Result<(Child, PathBuf), McError> {
    let instance_dir = super::instance::instance_dir(&paths.instances, &req.instance.id);
    let game_dir = super::instance::minecraft_dir(&paths.instances, &req.instance.id);
    let natives_dir = instance_dir.join("natives");
    tokio::fs::create_dir_all(&game_dir).await?;

    prepare_natives(paths, req.detail, &natives_dir).await?;

    // El client.jar siempre vive bajo la versión Vanilla base, aunque
    // `req.detail.id` sea el id fusionado de un loader (p.ej.
    // "fabric-loader-0.16.9-1.21.1") — ver `fabric_like::install`.
    let client_jar = paths
        .versions
        .join(&req.instance.minecraft_version)
        .join(format!("{}.jar", req.instance.minecraft_version));
    let classpath = build_classpath(paths, req.detail, &client_jar);

    let min_ram = req.instance.min_ram_mb.unwrap_or(req.default_min_ram_mb);
    let max_ram = req.instance.max_ram_mb.unwrap_or(req.default_max_ram_mb);

    let vars = build_substitution_vars(paths, &req, &game_dir, &natives_dir, &classpath);

    let mut jvm_args = vec![format!("-Xms{min_ram}M"), format!("-Xmx{max_ram}M")];
    if let Some(extra) = &req.instance.extra_jvm_args {
        jvm_args.extend(extra.split_whitespace().map(str::to_string));
    }

    match &req.detail.arguments {
        Some(arguments) => jvm_args.extend(resolve_argument_list(&arguments.jvm, &vars)),
        None => {
            jvm_args.push(format!("-Djava.library.path={}", natives_dir.display()));
            jvm_args.push("-cp".to_string());
            jvm_args.push(classpath.clone());
        }
    }

    let game_args = match &req.detail.arguments {
        Some(arguments) => resolve_argument_list(&arguments.game, &vars),
        None => req
            .detail
            .legacy_arguments
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(|token| substitute(token, &vars))
            .collect(),
    };

    let mut command = Command::new(req.java_path);
    command
        .args(&jvm_args)
        .arg(&req.detail.main_class)
        .args(&game_args)
        .current_dir(&game_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::process_ext::hide_console(&mut command);

    let child = command.spawn().map_err(McError::Io)?;

    let log_path = instance_dir.join("logs").join(format!(
        "{}.log",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    if let Some(parent) = log_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    Ok((child, log_path))
}

async fn prepare_natives(paths: &GamePaths, detail: &VersionDetail, natives_dir: &Path) -> Result<(), McError> {
    if natives_dir.exists() {
        tokio::fs::remove_dir_all(natives_dir).await?;
    }
    tokio::fs::create_dir_all(natives_dir).await?;

    for library in &detail.libraries {
        if !rule_applies(&library.rules) {
            continue;
        }
        let (Some(natives_map), Some(classifiers)) = (&library.natives, &library.downloads.classifiers) else {
            continue;
        };
        let Some(classifier_key) = natives_map.get(super::current_os_name()) else {
            continue;
        };
        let Some(artifact) = classifiers.get(classifier_key) else {
            continue;
        };
        let jar_path = paths.libraries.join(&artifact.path);
        extract_natives_jar(&jar_path, natives_dir).await?;
    }
    Ok(())
}

async fn extract_natives_jar(jar_path: &Path, dest: &Path) -> Result<(), McError> {
    let jar_path = jar_path.to_path_buf();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let file = std::fs::File::open(&jar_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
            let name = entry.name().to_string();
            if name.starts_with("META-INF/") || entry.is_dir() {
                continue;
            }
            let out_path = dest.join(&name);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| McError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))??;
    Ok(())
}

fn build_classpath(paths: &GamePaths, detail: &VersionDetail, client_jar: &Path) -> String {
    let separator = if cfg!(windows) { ";" } else { ":" };
    let library_entry = |artifact: &DownloadArtifactPath| paths.libraries.join(&artifact.path);

    let mut entries: Vec<String> = detail
        .libraries
        .iter()
        .filter(|library| rule_applies(&library.rules))
        .filter_map(|library| library.downloads.artifact.as_ref())
        .map(|artifact| library_entry(artifact).to_string_lossy().to_string())
        .collect();
    entries.push(client_jar.to_string_lossy().to_string());
    entries.join(separator)
}

fn build_substitution_vars(
    paths: &GamePaths,
    req: &LaunchRequest<'_>,
    game_dir: &Path,
    natives_dir: &Path,
    classpath: &str,
) -> HashMap<&'static str, String> {
    let mut vars = HashMap::new();
    vars.insert("natives_directory", natives_dir.to_string_lossy().to_string());
    vars.insert("launcher_name", req.launcher_name.clone());
    vars.insert("launcher_version", req.launcher_version.clone());
    vars.insert("classpath", classpath.to_string());
    vars.insert("classpath_separator", if cfg!(windows) { ";" } else { ":" }.to_string());
    vars.insert("library_directory", paths.libraries.to_string_lossy().to_string());

    vars.insert("auth_player_name", req.account.username.clone());
    vars.insert("version_name", req.detail.id.clone());
    vars.insert("game_directory", game_dir.to_string_lossy().to_string());
    vars.insert("assets_root", paths.assets.to_string_lossy().to_string());
    vars.insert("game_assets", paths.assets.to_string_lossy().to_string());
    vars.insert("assets_index_name", req.detail.assets.clone());
    vars.insert("auth_uuid", req.account.uuid.clone());
    vars.insert("auth_access_token", req.account.access_token.clone());
    vars.insert("auth_xuid", "0".to_string());
    vars.insert("clientid", "0".to_string());
    let user_type = match req.account.kind {
        AccountKind::Microsoft => "msa",
        AccountKind::Offline => "legacy",
    };
    vars.insert("user_type", user_type.to_string());
    vars.insert("user_properties", "{}".to_string());
    vars.insert("version_type", req.version_type_label.clone());
    vars.insert("resolution_width", "925".to_string());
    vars.insert("resolution_height", "530".to_string());
    vars
}

fn substitute(template: &str, vars: &HashMap<&'static str, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    result
}

/// Evalúa las reglas condicionales de un bloque de argumento moderno
/// (`{"rules": [...], "value": ...}`). Si la regla depende de una "feature"
/// que no soportamos todavía (demo, resolución custom, quick play...), el
/// argumento se omite en vez de arriesgarnos a mandarlo mal formado.
fn argument_rules_apply(rules: &[Value]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.get("features").is_some() {
            return false;
        }
        let action = rule.get("action").and_then(Value::as_str).unwrap_or("disallow");
        let os_matches = match rule.get("os").and_then(|os| os.get("name")).and_then(Value::as_str) {
            Some(name) => name == super::current_os_name(),
            None => true,
        };
        if os_matches {
            allowed = action == "allow";
        }
    }
    allowed
}

fn resolve_argument_list(raw: &[Value], vars: &HashMap<&'static str, String>) -> Vec<String> {
    let mut resolved = Vec::new();
    for entry in raw {
        match entry {
            Value::String(s) => resolved.push(substitute(s, vars)),
            Value::Object(map) => {
                let rules: Vec<Value> = map
                    .get("rules")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !argument_rules_apply(&rules) {
                    continue;
                }
                match map.get("value") {
                    Some(Value::String(s)) => resolved.push(substitute(s, vars)),
                    Some(Value::Array(values)) => {
                        for v in values {
                            if let Value::String(s) = v {
                                resolved.push(substitute(s, vars));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    resolved
}
