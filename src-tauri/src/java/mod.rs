//! Detección de instalaciones de Java compatibles (8, 17, 21) en el sistema.
//! Si no hay ninguna válida, `adoptium` se encarga de descargar e instalar un
//! runtime Temurin en una carpeta propia del launcher, sin intervención manual.

pub mod adoptium;

use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstallation {
    pub path: PathBuf,
    pub version: String,
    pub major_version: u32,
    pub arch: String,
    pub is_64bit: bool,
    /// true si el launcher la instaló él mismo (java-runtime/), false si es
    /// una instalación del sistema detectada.
    pub managed_by_launcher: bool,
}

/// Versiones de Java que Minecraft usa según la era de la versión:
/// 1.16 e inferiores -> 8, 1.17-1.20.4 -> 17, 1.20.5+ -> 21.
/// Hoy la UI (`Settings.tsx`) tiene esta misma lista duplicada en TS; queda
/// aquí como la fuente de verdad para cuando se exponga por IPC.
#[allow(dead_code)]
pub const SUPPORTED_MAJORS: [u32; 3] = [8, 17, 21];

pub fn current_os_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

fn candidate_paths(managed_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("java")]; // resuelve por PATH

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        candidates.push(bin_java(Path::new(&java_home)));
    }

    #[cfg(target_os = "macos")]
    {
        push_glob(&mut candidates, "/Library/Java/JavaVirtualMachines/*/Contents/Home");
        push_glob(&mut candidates, "/opt/homebrew/opt/openjdk*/libexec/openjdk.jdk/Contents/Home");
        push_glob(&mut candidates, "/usr/local/opt/openjdk*/libexec/openjdk.jdk/Contents/Home");
    }

    #[cfg(target_os = "windows")]
    {
        push_glob(&mut candidates, "C:/Program Files/Java/*");
        push_glob(&mut candidates, "C:/Program Files/Eclipse Adoptium/*");
        push_glob(&mut candidates, "C:/Program Files/Microsoft/jdk-*");
        candidates.extend(adoptium::windows_registry_java_homes());
    }

    #[cfg(target_os = "linux")]
    {
        push_glob(&mut candidates, "/usr/lib/jvm/*");
        candidates.push(PathBuf::from("/usr/bin/java"));
    }

    // Runtimes que el propio launcher instaló anteriormente. Se buscan de
    // forma recursiva (no con un glob de un nivel) porque los archivos de
    // Adoptium se extraen dentro de una carpeta propia y, en macOS, además
    // envuelven `Contents/Home` — la profundidad real no es fija.
    find_java_binaries_recursive(managed_dir, &mut candidates);

    candidates
        .into_iter()
        .map(|p| bin_java(&p))
        .filter(|p| p.file_name().is_some())
        .collect()
}

fn bin_java(java_home: &Path) -> PathBuf {
    if java_home.file_name().map(|n| n == "java" || n == "java.exe").unwrap_or(false) {
        return java_home.to_path_buf();
    }
    let exe = if cfg!(target_os = "windows") { "java.exe" } else { "java" };
    java_home.join("bin").join(exe)
}

/// Busca ejecutables `java`/`java.exe` bajo `root`, sin asumir una
/// profundidad fija (los runtimes de Adoptium extraen su propia carpeta raíz
/// y macOS además anida `Contents/Home`). Misma lógica que
/// `adoptium::find_java_executable`, pero acumulando todos los matches en
/// vez de devolver solo el primero, porque aquí puede haber varios majores
/// (8, 17, 21) instalados a la vez.
fn find_java_binaries_recursive(root: &Path, out: &mut Vec<PathBuf>) {
    let target_name = if cfg!(target_os = "windows") { "java.exe" } else { "java" };
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0;
    while let Some(dir) = stack.pop() {
        visited += 1;
        if visited > 5000 {
            break; // salvaguarda contra árboles inesperadamente grandes
        }
        let candidate = dir.join("bin").join(target_name);
        if candidate.is_file() {
            out.push(candidate);
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    stack.push(entry.path());
                }
            }
        }
    }
}

fn push_glob(out: &mut Vec<PathBuf>, pattern: &str) {
    if let Ok(paths) = glob_lite(pattern) {
        out.extend(paths);
    }
}

/// Mini implementación de glob para un único `*` en la ruta (evita añadir la
/// dependencia `glob` solo para esto). Soporta tanto un comodín de segmento
/// completo (`.../JavaVirtualMachines/*/Contents/Home`) como un comodín que
/// completa un nombre parcial (`.../jdk-*`, `.../openjdk*/...`).
fn glob_lite(pattern: &str) -> std::io::Result<Vec<PathBuf>> {
    let Some(star_idx) = pattern.find('*') else {
        return Ok(vec![PathBuf::from(pattern)]);
    };
    let before_star = &pattern[..star_idx];
    let after_star = &pattern[star_idx + 1..];

    // Directorio a listar: hasta la última '/' antes del '*'. `name_prefix`
    // es lo que debe matchear al inicio del nombre de cada entrada dentro de
    // ese directorio (vacío si el '*' viene justo tras una '/').
    let last_slash = before_star.rfind('/').unwrap_or(0);
    let dir = if last_slash == 0 { "/" } else { &before_star[..last_slash] };
    let name_prefix = &before_star[last_slash + 1..];

    // Lo que sigue al '*' puede tener más ruta después del segmento comodín
    // (p.ej. `/Contents/Home`); lo separamos del resto del nombre de archivo.
    let (name_suffix, rest_path) = match after_star.find('/') {
        Some(i) => (&after_star[..i], &after_star[i..]),
        None => (after_star, ""),
    };

    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(name_prefix) && name.ends_with(name_suffix) {
                let candidate = format!("{}{}", entry.path().display(), rest_path);
                if rest_path.is_empty() || Path::new(&candidate).exists() {
                    results.push(PathBuf::from(candidate));
                }
            }
        }
    }
    Ok(results)
}

/// Ejecuta `java -XshowSettings:properties -version` y parsea la versión y
/// arquitectura reales de esa instalación (más fiable que asumir por la ruta).
pub async fn probe(java_bin: &Path, managed_dir: &Path) -> Option<JavaInstallation> {
    let mut cmd = Command::new(java_bin);
    cmd.arg("-XshowSettings:properties").arg("-version");
    crate::process_ext::hide_console(&mut cmd);
    let output = cmd.output().await.ok()?;

    // java -version imprime en stderr.
    let text = String::from_utf8_lossy(&output.stderr);

    let version = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("java.version = "))
        .map(|s| s.to_string())?;

    let arch = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("os.arch = "))
        .unwrap_or("unknown")
        .to_string();

    let major_version = parse_major_version(&version);
    let is_64bit = arch.contains("64");
    let managed_by_launcher = java_bin.starts_with(managed_dir);

    Some(JavaInstallation {
        path: java_bin.to_path_buf(),
        version,
        major_version,
        arch,
        is_64bit,
        managed_by_launcher,
    })
}

/// "1.8.0_392" -> 8, "21.0.1" -> 21, "17" -> 17
fn parse_major_version(version: &str) -> u32 {
    let mut parts = version.split(['.', '_']);
    let first = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
    if first == 1 {
        parts.next().and_then(|s| s.parse().ok()).unwrap_or(0)
    } else {
        first
    }
}

/// Escanea el sistema en busca de instalaciones de Java utilizables. Deduplica
/// por ruta canónica para no listar la misma instalación varias veces.
pub async fn detect_installations(managed_dir: &Path) -> Vec<JavaInstallation> {
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();

    for candidate in candidate_paths(managed_dir) {
        let canonical = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if !seen.insert(canonical.clone()) {
            continue;
        }
        if let Some(installation) = probe(&candidate, managed_dir).await {
            results.push(installation);
        }
    }

    results
}

/// Elige la mejor instalación disponible para un `major` requerido: coincidencia
/// exacta si existe, o la más cercana por arriba (compatibilidad razonable),
/// priorizando siempre arquitectura de 64 bits.
pub fn find_best<'a>(
    installations: &'a [JavaInstallation],
    required_major: u32,
) -> Option<&'a JavaInstallation> {
    installations
        .iter()
        .filter(|i| i.is_64bit)
        .filter(|i| i.major_version == required_major)
        .next()
        .or_else(|| {
            installations
                .iter()
                .filter(|i| i.is_64bit && i.major_version >= required_major)
                .min_by_key(|i| i.major_version)
        })
}
