//! Gestor de descargas genérico usado por los módulos de Java y Minecraft.
//!
//! Responsabilidades:
//! - Verificación de integridad por SHA1 contra los manifiestos oficiales.
//! - Reanudación de descargas interrumpidas vía `Range` HTTP.
//! - Caché de contenido direccionado por hash (`cache/<sha1[0:2]>/<sha1>`) para
//!   que instancias distintas que comparten librerías/assets no las vuelvan a
//!   descargar — clave para mantener el launcher ligero en disco.
//! - Descargas concurrentes con límite, para no saturar la conexión del usuario.

use futures_util::StreamExt;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Semaphore;

const MAX_CONCURRENT_DOWNLOADS: usize = 8;

#[derive(thiserror::Error, Debug)]
pub enum DownloadError {
    #[error("error de red descargando {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("error de E/S en {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("SHA1 no coincide para {label}: esperado {expected}, obtenido {actual}")]
    HashMismatch {
        label: String,
        expected: String,
        actual: String,
    },
    #[error("respuesta HTTP {status} descargando {url}")]
    BadStatus { url: String, status: u16 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub label: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    /// Progreso agregado del lote (para barras de progreso de "instalando versión X").
    pub completed_files: usize,
    pub total_files: usize,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub destination: PathBuf,
    /// SHA1 esperado en minúsculas hex, si el manifiesto oficial lo provee.
    pub expected_sha1: Option<String>,
    pub label: String,
}

pub type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

pub struct DownloadManager {
    client: reqwest::Client,
}

impl DownloadManager {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("MiLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("no se pudo construir el cliente HTTP");
        Self { client }
    }

    /// Descarga un único archivo, usando caché por hash y reanudación.
    /// Si el archivo ya existe en destino y su SHA1 coincide, no hace red.
    pub async fn fetch(
        &self,
        req: &DownloadRequest,
        cache_dir: &Path,
        on_progress: &ProgressCallback,
    ) -> Result<(), DownloadError> {
        if let Some(parent) = req.destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| io_err(parent, e))?;
        }

        // 1. ¿Ya está instalado y es válido? No hacemos nada.
        if req.destination.exists() {
            if let Some(expected) = &req.expected_sha1 {
                if verify_sha1(&req.destination, expected).await.unwrap_or(false) {
                    emit_done(&req.label, &req.destination, on_progress).await;
                    return Ok(());
                }
            } else {
                emit_done(&req.label, &req.destination, on_progress).await;
                return Ok(());
            }
        }

        // 2. ¿Está en caché de contenido (compartida entre instancias)?
        if let Some(expected) = &req.expected_sha1 {
            let cached = cache_path(cache_dir, expected);
            if cached.exists() && verify_sha1(&cached, expected).await.unwrap_or(false) {
                fs::copy(&cached, &req.destination)
                    .await
                    .map_err(|e| io_err(&req.destination, e))?;
                emit_done(&req.label, &req.destination, on_progress).await;
                return Ok(());
            }
        }

        // 3. Descarga real, con reanudación.
        self.download_with_resume(req, on_progress).await?;

        // 4. Verificación de integridad + población de caché.
        if let Some(expected) = &req.expected_sha1 {
            let actual = hash_of(&req.destination).await.map_err(|e| io_err(&req.destination, e))?;
            if !actual.eq_ignore_ascii_case(expected) {
                let _ = fs::remove_file(&req.destination).await;
                return Err(DownloadError::HashMismatch {
                    label: req.label.clone(),
                    expected: expected.clone(),
                    actual,
                });
            }
            let cached = cache_path(cache_dir, expected);
            if let Some(parent) = cached.parent() {
                let _ = fs::create_dir_all(parent).await;
            }
            let _ = fs::copy(&req.destination, &cached).await;
        }

        Ok(())
    }

    async fn download_with_resume(
        &self,
        req: &DownloadRequest,
        on_progress: &ProgressCallback,
    ) -> Result<(), DownloadError> {
        let part_path = part_path_for(&req.destination);
        let mut existing_len = fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);

        let mut builder = self.client.get(&req.url);
        if existing_len > 0 {
            builder = builder.header("Range", format!("bytes={existing_len}-"));
        }

        let response = builder.send().await.map_err(|e| DownloadError::Network {
            url: req.url.clone(),
            source: e,
        })?;

        let status = response.status();
        let resumed = status.as_u16() == 206;
        if !resumed && existing_len > 0 {
            // El servidor no soporta rangos: empezamos de cero.
            existing_len = 0;
        }
        if !status.is_success() {
            return Err(DownloadError::BadStatus {
                url: req.url.clone(),
                status: status.as_u16(),
            });
        }

        let total_bytes = response
            .content_length()
            .map(|len| if resumed { len + existing_len } else { len });

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&part_path)
            .await
            .map_err(|e| io_err(&part_path, e))?;

        if resumed {
            file.seek(SeekFrom::End(0)).await.map_err(|e| io_err(&part_path, e))?;
        } else {
            file.set_len(0).await.map_err(|e| io_err(&part_path, e))?;
            file.seek(SeekFrom::Start(0)).await.map_err(|e| io_err(&part_path, e))?;
        }

        let mut downloaded = existing_len;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| DownloadError::Network {
                url: req.url.clone(),
                source: e,
            })?;
            file.write_all(&chunk).await.map_err(|e| io_err(&part_path, e))?;
            downloaded += chunk.len() as u64;
            on_progress(DownloadProgress {
                label: req.label.clone(),
                downloaded_bytes: downloaded,
                total_bytes,
                completed_files: 0,
                total_files: 0,
            });
        }
        file.flush().await.map_err(|e| io_err(&part_path, e))?;
        drop(file);

        fs::rename(&part_path, &req.destination)
            .await
            .map_err(|e| io_err(&req.destination, e))?;
        Ok(())
    }

    /// Descarga un lote de archivos con concurrencia limitada, reportando
    /// progreso agregado (archivos completados / total) además del progreso
    /// individual de cada uno.
    pub async fn fetch_many(
        &self,
        requests: Vec<DownloadRequest>,
        cache_dir: &Path,
        on_progress: ProgressCallback,
    ) -> Result<(), DownloadError> {
        let total_files = requests.len();
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));

        let results: Vec<Result<(), DownloadError>> = futures_util::stream::iter(requests)
            .map(move |req| {
                let semaphore = semaphore.clone();
                let completed = completed.clone();
                let on_progress = on_progress.clone();
                let cache_dir = cache_dir.to_path_buf();
                async move {
                    let _permit = semaphore.acquire().await.expect("semáforo cerrado");
                    let per_file_progress: ProgressCallback = {
                        let on_progress = on_progress.clone();
                        let completed = completed.clone();
                        Arc::new(move |mut p: DownloadProgress| {
                            p.completed_files = completed.load(std::sync::atomic::Ordering::Relaxed);
                            p.total_files = total_files;
                            on_progress(p);
                        })
                    };
                    let result = self.fetch(&req, &cache_dir, &per_file_progress).await;
                    let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    on_progress(DownloadProgress {
                        label: req.label.clone(),
                        downloaded_bytes: 0,
                        total_bytes: None,
                        completed_files: done,
                        total_files,
                    });
                    result
                }
            })
            .buffer_unordered(MAX_CONCURRENT_DOWNLOADS)
            .collect()
            .await;

        results.into_iter().collect::<Result<Vec<()>, _>>()?;
        Ok(())
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

fn part_path_for(destination: &Path) -> PathBuf {
    let mut p = destination.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

fn cache_path(cache_dir: &Path, sha1: &str) -> PathBuf {
    let sha1 = sha1.to_lowercase();
    cache_dir.join(&sha1[0..2]).join(sha1)
}

async fn hash_of(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn verify_sha1(path: &Path, expected: &str) -> std::io::Result<bool> {
    let actual = hash_of(path).await?;
    Ok(actual.eq_ignore_ascii_case(expected))
}

async fn emit_done(label: &str, path: &Path, on_progress: &ProgressCallback) {
    let total = fs::metadata(path).await.map(|m| m.len()).unwrap_or(0);
    on_progress(DownloadProgress {
        label: label.to_string(),
        downloaded_bytes: total,
        total_bytes: Some(total),
        completed_files: 0,
        total_files: 0,
    });
}

fn io_err(path: &Path, source: std::io::Error) -> DownloadError {
    DownloadError::Io {
        path: path.to_path_buf(),
        source,
    }
}
