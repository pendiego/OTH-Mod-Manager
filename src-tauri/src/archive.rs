use serde::Serialize;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::LazyLock;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| reqwest::Client::builder().no_gzip().build().expect("cliente HTTP"));
pub const UA: &str = "OTH-Mod-Manager/0.1";
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Pasta temporária única; quem chama remove com `remove_dir_all`.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("oth-mm-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)))
}

#[derive(Serialize, Clone)]
struct Progress {
    received: u64,
    total: Option<u64>,
}


/// GET com `Range` a partir de `from` (0 = download inteiro).
async fn get(url: &str, from: u64) -> Result<reqwest::Response, String> {
    let mut req = HTTP.get(url).header("User-Agent", UA);
    if from > 0 {
        req = req.header("Range", format!("bytes={from}-"));
    }
    let resp = req.send().await.map_err(|e| format!("Falha ao baixar {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Falha ao baixar {url}: HTTP {}", resp.status()));
    }
    Ok(resp)
}

/// Se a conexão cair no meio, retoma de onde parou (até 3 vezes); servidor que ignora `Range` reinicia do zero.
pub async fn download(url: &str, dest: &Path, app: &AppHandle) -> Result<(), String> {
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut resp = get(url, 0).await?;
    let total = resp.content_length();
    let mut f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let (mut received, mut last, mut retries) = (0u64, 0u64, 0);
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                f.write_all(&chunk).map_err(|e| e.to_string())?;
                received += chunk.len() as u64;
                if received - last >= 256 * 1024 {
                    last = received;
                    let _ = app.emit("download-progress", Progress { received, total });
                }
            }
            Ok(None) => break,
            Err(e) => {
                retries += 1;
                if retries > 3 {
                    return Err(format!("Falha ao baixar {url}: {e}"));
                }
                resp = get(url, received).await?;
                if received > 0 && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                    f.set_len(0).and_then(|_| f.seek(SeekFrom::Start(0))).map_err(|e| e.to_string())?;
                    (received, last) = (0, 0);
                }
            }
        }
    }
    let _ = app.emit("download-progress", Progress { received, total });
    Ok(())
}

fn unsafe_path(p: &Path) -> bool {
    p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir))
        || p.to_string_lossy().replace('\\', "/").split('/').any(|s| s == ".." || s.contains(':'))
}

pub fn extract(archive: &Path, out: &Path) -> Result<(), String> {
    let ext = archive.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "zip" => {
            let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
            let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Falha ao extrair zip: {e}"))?;
            for n in 0..z.len() {
                let entry = z.by_index(n).map_err(|e| format!("Falha ao extrair zip: {e}"))?;
                if unsafe_path(Path::new(entry.name())) || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
                    return Err("Falha ao extrair zip: caminho inseguro".into());
                }
            }
            z.extract(out).map_err(|e| format!("Falha ao extrair zip: {e}"))
        }
        "7z" => sevenz_rust2::decompress_file(archive, out).map_err(|e| format!("Falha ao extrair 7z: {e}")),
        "rar" => {
            let mut ar = unrar::Archive::new(archive)
                .open_for_processing()
                .map_err(|e| format!("Falha ao abrir rar: {e}"))?;
            while let Some(h) = ar.read_header().map_err(|e| format!("Falha ao ler rar: {e}"))? {
                if unsafe_path(&h.entry().filename) {
                    return Err("Falha ao extrair rar: caminho inseguro".into());
                }
                ar = h.extract_with_base(out).map_err(|e| format!("Falha ao extrair rar: {e}"))?;
            }
            Ok(())
        }
        _ => Err("Formato não suportado".into()),
    }
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry_res in WalkDir::new(src) {
        let entry = entry_res.map_err(|e| {
            let msg = e.to_string();
            e.into_io_error().unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, msg))
        })?;
        let rel = entry.path().strip_prefix(src)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
