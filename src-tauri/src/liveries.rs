use crate::game::{disabled_liveries_dir, liveries_dir};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Manifest {
    #[serde(alias = "Liveries")]
    liveries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(alias = "Car")]
    car: String,
    #[serde(alias = "Name")]
    name: String,
    #[serde(alias = "Texture")]
    texture: String,
    #[serde(default, alias = "Icon")]
    icon: Option<String>,
}

#[derive(Serialize)]
pub struct LiveryEntry {
    car: String,
    name: String,
}

#[derive(Serialize)]
pub struct LiveryPack {
    folder: String,
    enabled: bool,
    entries: Vec<LiveryEntry>,
    thumb: Option<String>,
    error: Option<String>,
}

fn safe(folder: &str) -> Result<(), String> {
    let one_normal = {
        let mut c = Path::new(folder).components();
        matches!((c.next(), c.next()), (Some(std::path::Component::Normal(_)), None))
    };
    if !one_normal || folder.contains(['/', '\\', ':']) || folder.ends_with(['.', ' ']) {
        return Err("Nome inválido".into());
    }
    Ok(())
}

fn rm(p: &Path) -> Result<(), String> {
    if p.exists() {
        std::fs::remove_dir_all(p).map_err(|e| format!("Feche o jogo e tente novamente: {e}"))?;
    }
    Ok(())
}

pub fn install(packs: &[(PathBuf, String)]) -> Result<Vec<String>, String> {
    let (on, off) = (liveries_dir(), disabled_liveries_dir());
    std::fs::create_dir_all(&on).map_err(|e| e.to_string())?;
    let mut names = vec![];
    for (src, folder) in packs {
        safe(folder)?;
        rm(&on.join(folder))?;
        rm(&off.join(folder))?;
        crate::archive::copy_dir_all(src, &on.join(folder)).map_err(|e| e.to_string())?;
        names.push(folder.clone());
    }
    Ok(names)
}

fn thumb(dir: &Path, e: &Entry) -> Option<String> {
    let rel = e.icon.as_deref().filter(|i| !i.is_empty()).unwrap_or(&e.texture);
    let img = image::open(dir.join(rel)).ok()?.thumbnail(320, 320);
    let mut buf = std::io::Cursor::new(vec![]);
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
}

fn read_pack(dir: &Path, enabled: bool) -> LiveryPack {
    let folder = dir.file_name().unwrap().to_string_lossy().into_owned();
    let m = std::fs::read(dir.join("manifest.json")).ok().and_then(|b| serde_json::from_slice::<Manifest>(&b).ok());
    let Some(m) = m else {
        return LiveryPack { folder, enabled, entries: vec![], thumb: None, error: Some("manifest.json ausente ou inválido".into()) };
    };
    let error = m.liveries.iter().find(|e| !dir.join(&e.texture).is_file()).map(|e| format!("Textura não encontrada: {}", e.texture));
    LiveryPack {
        thumb: m.liveries.first().and_then(|e| thumb(dir, e)),
        entries: m.liveries.into_iter().map(|e| LiveryEntry { car: e.car, name: e.name }).collect(),
        folder,
        enabled,
        error,
    }
}

#[tauri::command]
pub fn list_liveries() -> Result<Vec<LiveryPack>, String> {
    let on = liveries_dir();
    std::fs::create_dir_all(&on).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for (d, enabled) in [(on, true), (disabled_liveries_dir(), false)] {
        if let Ok(rd) = std::fs::read_dir(d) {
            out.extend(rd.flatten().filter(|e| e.path().is_dir()).map(|e| read_pack(&e.path(), enabled)));
        }
    }
    out.sort_by_key(|p| p.folder.to_lowercase());
    Ok(out)
}

#[tauri::command]
pub fn set_livery_enabled(folder: String, enabled: bool) -> Result<(), String> {
    safe(&folder)?;
    let (on, off) = (liveries_dir(), disabled_liveries_dir());
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
    if from.join(&folder).exists() {
        std::fs::rename(from.join(&folder), to.join(&folder)).map_err(|e| format!("Feche o jogo e tente novamente: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn remove_livery(folder: String) -> Result<(), String> {
    safe(&folder)?;
    rm(&liveries_dir().join(&folder))?;
    rm(&disabled_liveries_dir().join(&folder))
}

#[cfg(test)]
mod tests {
    use super::safe;

    #[test]
    fn safe_rejects_root_and_traversal_names() {
        for bad in ["", ".", "..", "a/b", "a\\b", "C:", "x.", "x ", ".."] {
            assert!(safe(bad).is_err(), "{bad:?}");
        }
        assert!(safe("OTHMM Test").is_ok());
    }
}
