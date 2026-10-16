use crate::loaders::Loader;
use crate::{archive, game, liveries};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Clone)]
struct ModRecord {
    id: String,
    name: String,
    loader: Loader,
    /// relativos ao jogo, com `/`
    files: Vec<String>,
    enabled: bool,
    #[serde(default)]
    nexus_id: Option<i64>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    mods: Vec<ModRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    id: String,
    name: String,
    loader: Loader,
    enabled: bool,
    managed: bool,
    nexus_id: Option<i64>,
    version: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Plan {
    /// (pasta de origem, nome da pasta de destino)
    Liveries(Vec<(PathBuf, String)>),
    Code { loader: Loader, copies: Vec<(PathBuf, String)> },
}

const WRAPPERS: [&str; 7] = ["mods", "plugins", "userdata", "userlibs", "bepinex", "patchers", "config"];
const UNKNOWN: &str = "Estrutura de mod não reconhecida";

fn lower(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn children(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default()
}

fn files_under(dir: &Path, dst_prefix: &str, out: &mut Vec<(PathBuf, String)>) {
    for e in WalkDir::new(dir).sort_by_file_name().into_iter().flatten().filter(|e| e.file_type().is_file()) {
        let rel = e.path().strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
        out.push((e.path().to_path_buf(), format!("{dst_prefix}/{rel}")));
    }
}

fn is_dll(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll"))
}

pub fn classify(root: &Path, mod_name: &str) -> Result<Plan, String> {
    // 1. unwrap
    let mut root = root.to_path_buf();
    loop {
        let kids = children(&root);
        if kids.len() == 1 && kids[0].is_dir() && !WRAPPERS.contains(&lower(&kids[0]).as_str()) {
            root = kids[0].clone();
        } else {
            break;
        }
    }

    // 2. liveries
    let mut packs = vec![];
    for e in WalkDir::new(&root).max_depth(2).sort_by_file_name().into_iter().flatten() {
        if e.file_type().is_file() && e.file_name().eq_ignore_ascii_case("manifest.json") {
            let ok = std::fs::read(e.path())
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .is_some_and(|v| v.get("liveries").and_then(|l| l.as_array()).is_some_and(|a| !a.is_empty()));
            if ok {
                let dir = e.path().parent().unwrap().to_path_buf();
                let name = if dir == root { mod_name.to_string() } else { dir.file_name().unwrap().to_string_lossy().into_owned() };
                packs.push((dir, name));
            }
        }
    }
    if !packs.is_empty() {
        return Ok(Plan::Liveries(packs));
    }

    // 3. loader bundle
    for e in WalkDir::new(&root).into_iter().flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        let rel = e.path().strip_prefix(&root).unwrap().to_string_lossy().to_lowercase().replace('\\', "/");
        let bundle = (e.file_type().is_file() && (n == "version.dll" || n == "winhttp.dll"))
            || (e.file_type().is_dir() && (n == "melonloader" || rel.ends_with("bepinex/core")));
        if bundle {
            return Err("O arquivo contém um loader; instale loaders em Ajustes".into());
        }
    }

    // 4. loader detection
    let kids = children(&root);
    let dir_named = |n: &str| kids.iter().find(|k| k.is_dir() && lower(k) == n).cloned();
    let loader = if dir_named("bepinex").is_some() {
        Loader::Bepinex
    } else {
        let (mut melon, mut bep) = (false, false);
        for e in WalkDir::new(&root).into_iter().flatten().filter(|e| e.file_type().is_file() && is_dll(e.path())) {
            if let Ok(b) = std::fs::read(e.path()) {
                melon |= b.windows(11).any(|w| w == b"MelonLoader");
                bep |= b.windows(7).any(|w| w == b"BepInEx");
            }
        }
        if melon {
            Loader::Melon
        } else if bep {
            Loader::Bepinex
        } else if ["mods", "userdata", "userlibs"].iter().any(|n| dir_named(n).is_some()) {
            Loader::Melon
        } else {
            return Err(UNKNOWN.into());
        }
    };

    // 5. placement
    let mut copies = vec![];
    let root_dlls = kids.iter().filter(|k| k.is_file() && is_dll(k));
    match loader {
        Loader::Melon => {
            for (n, canon) in [("mods", "Mods"), ("plugins", "Plugins"), ("userdata", "UserData"), ("userlibs", "UserLibs")] {
                if let Some(d) = dir_named(n) {
                    files_under(&d, canon, &mut copies);
                }
            }
            for f in root_dlls {
                copies.push((f.clone(), format!("Mods/{}", f.file_name().unwrap().to_string_lossy())));
            }
        }
        Loader::Bepinex => {
            if let Some(d) = dir_named("bepinex") {
                files_under(&d, "BepInEx", &mut copies);
            } else {
                for n in ["plugins", "patchers", "config"] {
                    if let Some(d) = dir_named(n) {
                        files_under(&d, &format!("BepInEx/{n}"), &mut copies);
                    }
                }
                for f in root_dlls {
                    copies.push((f.clone(), format!("BepInEx/plugins/{mod_name}/{}", f.file_name().unwrap().to_string_lossy())));
                }
            }
        }
    }
    if copies.is_empty() {
        return Err(UNKNOWN.into());
    }
    Ok(Plan::Code { loader, copies })
}

#[derive(Debug, PartialEq)]
pub struct ArchiveName {
    pub name: String,
    pub nexus_id: Option<i64>,
    pub version: Option<String>,
}

pub fn parse_archive_name(path: &Path) -> ArchiveName {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let re = regex::Regex::new(r"^(.*?)-(\d+)-(.*)-(\d{9,})$").unwrap();
    match re.captures(&stem) {
        Some(c) => ArchiveName { name: c[1].trim().to_string(), nexus_id: c[2].parse().ok(), version: Some(c[3].to_string()) },
        None => ArchiveName { name: stem.trim().to_string(), nexus_id: None, version: None },
    }
}

fn mod_id(name: &str) -> String {
    let re = regex::Regex::new(r"[^a-z0-9]+").unwrap();
    re.replace_all(&name.to_lowercase(), "-").trim_matches('-').to_string()
}

// ---------- store ----------

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("mods.json"))
}

fn load_store(app: &AppHandle) -> Store {
    store_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_store(app: &AppHandle, s: &Store) -> Result<(), String> {
    let p = store_path(app)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(p, serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn rename_err(e: std::io::Error) -> String {
    format!("Feche o jogo e tente novamente: {e}")
}

fn disabled_of(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".disabled");
    PathBuf::from(s)
}

fn toggle_file(g: &Path, rel: &str, enabled: bool) -> Result<(), String> {
    let on = g.join(rel);
    let off = disabled_of(&on);
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if from.exists() && !to.exists() {
        std::fs::rename(from, to).map_err(rename_err)?;
    }
    Ok(())
}

const KEEP: [&str; 7] = ["mods", "plugins", "userdata", "userlibs", "bepinex", "bepinex/plugins", "bepinex/config"];

fn remove_files(g: &Path, files: &[String]) -> Result<(), String> {
    for rel in files {
        let p = g.join(rel);
        for f in [disabled_of(&p), p.clone()] {
            if f.exists() {
                std::fs::remove_file(&f).map_err(rename_err)?;
            }
        }
        let mut dir = p.parent().map(Path::to_path_buf);
        while let Some(d) = dir {
            let rel_d = d.strip_prefix(g).map(|r| r.to_string_lossy().to_lowercase().replace('\\', "/")).unwrap_or_default();
            if rel_d.is_empty() || KEEP.contains(&rel_d.as_str()) || std::fs::remove_dir(&d).is_err() {
                break;
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }
    Ok(())
}

fn install_code(app: &AppHandle, loader: Loader, copies: Vec<(PathBuf, String)>, name: &str, nexus_id: Option<i64>, version: Option<String>) -> Result<(), String> {
    let g = game::game_dir(app)?;
    let mut store = load_store(app);
    let id = mod_id(name);
    let pos = store.mods.iter().position(|m| m.id == id || (nexus_id.is_some() && m.nexus_id == nexus_id));
    if let Some(i) = pos {
        let old = store.mods.remove(i);
        remove_files(&g, &old.files)?;
    }
    for (_, dst) in &copies {
        let key = dst.to_lowercase();
        if let Some(o) = store.mods.iter().find(|m| m.files.iter().any(|f| f.to_lowercase() == key)) {
            return Err(format!("Conflito com o mod {}: {dst}", o.name));
        }
    }
    let mut files = vec![];
    for (src, dst) in copies {
        let target = g.join(&dst);
        let low = dst.to_lowercase();
        if (low.starts_with("userdata/") || low.starts_with("bepinex/config/")) && target.exists() {
            continue;
        }
        std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::copy(&src, &target).map_err(rename_err)?;
        files.push(dst);
    }
    store.mods.push(ModRecord { id, name: name.to_string(), loader, files, enabled: true, nexus_id, version });
    save_store(app, &store)
}

#[derive(Serialize)]
pub struct InstallResult {
    names: Vec<String>,
    loader: Option<Loader>,
}

/// Bloqueante: extrai, classifica e instala. `meta` (id, versão) sobrescreve o que vem do nome do arquivo.
pub(crate) fn install_path(app: &AppHandle, path: &Path, meta: Option<(i64, String)>) -> Result<InstallResult, String> {
    let tmp = archive::temp_dir();
    let res = (|| {
        archive::extract(path, &tmp)?;
        let mut a = parse_archive_name(path);
        if let Some((id, ver)) = meta {
            a.nexus_id = Some(id);
            a.version = Some(ver);
        }
        match classify(&tmp, &a.name)? {
            Plan::Liveries(packs) => liveries::install(&packs).map(|names| InstallResult { names, loader: None }),
            Plan::Code { loader, copies } => {
                install_code(app, loader, copies, &a.name, a.nexus_id, a.version).map(|_| InstallResult { names: vec![a.name], loader: Some(loader) })
            }
        }
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

#[tauri::command]
pub async fn install_archive(app: AppHandle, path: String) -> Result<InstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || install_path(&app, Path::new(&path), None))
        .await
        .map_err(|e| e.to_string())?
}

// ---------- list / toggle / uninstall ----------

fn unmanaged(g: &Path, store: &Store) -> Vec<(String, String, Loader, bool)> {
    let known: Vec<String> = store.mods.iter().flat_map(|m| m.files.iter().map(|f| f.to_lowercase())).collect();
    let mut out = vec![];
    for (sub, depth, loader) in [("Mods", 1, Loader::Melon), ("Plugins", 1, Loader::Melon), ("BepInEx/plugins", usize::MAX, Loader::Bepinex)] {
        for e in WalkDir::new(g.join(sub)).max_depth(depth).into_iter().flatten().filter(|e| e.file_type().is_file()) {
            let full = e.path().to_string_lossy().replace('\\', "/");
            let (full, enabled) = match full.strip_suffix(".disabled") {
                Some(s) => (s.to_string(), false),
                None => (full, true),
            };
            if !full.to_lowercase().ends_with(".dll") {
                continue;
            }
            let rel = Path::new(&full).strip_prefix(g.to_string_lossy().replace('\\', "/")).map(|r| r.to_string_lossy().into_owned()).unwrap_or(full.clone());
            if known.contains(&rel.to_lowercase()) {
                continue;
            }
            let name = Path::new(&rel).file_stem().unwrap().to_string_lossy().into_owned();
            out.push((format!("unmanaged:{rel}"), name, loader, enabled));
        }
    }
    out
}

#[tauri::command]
pub fn list_mods(app: AppHandle) -> Result<Vec<ModEntry>, String> {
    let store = load_store(&app);
    let mut managed: Vec<ModEntry> = store
        .mods
        .iter()
        .map(|m| ModEntry { id: m.id.clone(), name: m.name.clone(), loader: m.loader, enabled: m.enabled, managed: true, nexus_id: m.nexus_id, version: m.version.clone() })
        .collect();
    managed.sort_by_key(|m| m.name.to_lowercase());
    let mut un: Vec<ModEntry> = match game::game_dir(&app) {
        Ok(g) => unmanaged(&g, &store).into_iter().map(|(id, name, loader, enabled)| ModEntry { id, name, loader, enabled, managed: false, nexus_id: None, version: None }).collect(),
        Err(_) => vec![],
    };
    un.sort_by_key(|m| m.name.to_lowercase());
    managed.extend(un);
    Ok(managed)
}

#[tauri::command]
pub fn set_mod_enabled(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let g = game::game_dir(&app)?;
    if let Some(rel) = id.strip_prefix("unmanaged:") {
        return toggle_file(&g, rel, enabled);
    }
    let mut store = load_store(&app);
    let m = store.mods.iter_mut().find(|m| m.id == id).ok_or("Mod não encontrado")?;
    for f in m.files.iter().filter(|f| f.to_lowercase().ends_with(".dll")) {
        toggle_file(&g, f, enabled)?;
    }
    m.enabled = enabled;
    save_store(&app, &store)
}

#[tauri::command]
pub fn uninstall_mod(app: AppHandle, id: String) -> Result<(), String> {
    let g = game::game_dir(&app)?;
    if let Some(rel) = id.strip_prefix("unmanaged:") {
        return remove_files(&g, &[rel.to_string()]);
    }
    let mut store = load_store(&app);
    if let Some(i) = store.mods.iter().position(|m| m.id == id) {
        let m = store.mods.remove(i);
        remove_files(&g, &m.files)?;
        save_store(&app, &store)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &[u8])]) -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!("oth-test-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        for (p, b) in files {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        root
    }

    fn code(p: Result<Plan, String>) -> (Loader, Vec<String>) {
        match p.unwrap() {
            Plan::Code { loader, copies } => (loader, copies.into_iter().map(|c| c.1).collect()),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn melon_root_dll() {
        let r = tree(&[("Foo.dll", b"xxMelonLoaderxx")]);
        assert_eq!(code(classify(&r, "Foo")), (Loader::Melon, vec!["Mods/Foo.dll".into()]));
    }

    #[test]
    fn unwraps_and_keeps_userdata() {
        let r = tree(&[("Foo-1.0/Mods/Foo.dll", b"x"), ("Foo-1.0/UserData/Foo.cfg", b"x")]);
        let (l, f) = code(classify(&r, "Foo"));
        assert_eq!((l, f), (Loader::Melon, vec!["Mods/Foo.dll".into(), "UserData/Foo.cfg".into()]));
    }

    #[test]
    fn bepinex_tree_and_loose_dll() {
        let r = tree(&[("BepInEx/plugins/Bar/Bar.dll", b"x")]);
        assert_eq!(code(classify(&r, "Bar")), (Loader::Bepinex, vec!["BepInEx/plugins/Bar/Bar.dll".into()]));
        let r = tree(&[("Bar.dll", b"BepInEx")]);
        assert_eq!(code(classify(&r, "Bar Mod")).1, vec!["BepInEx/plugins/Bar Mod/Bar.dll".to_string()]);
    }

    #[test]
    fn rejects_loader_bundle_and_unknown() {
        let r = tree(&[("version.dll", b"x"), ("MelonLoader/x", b"x")]);
        assert!(classify(&r, "L").unwrap_err().contains("loader"));
        let r = tree(&[("readme.txt", b"x")]);
        assert_eq!(classify(&r, "L").unwrap_err(), UNKNOWN);
    }

    #[test]
    fn livery_pack_at_root() {
        let r = tree(&[("manifest.json", br#"{"liveries":[{"car":"c","name":"n","texture":"t.png"}]}"#)]);
        assert_eq!(classify(&r, "P").unwrap(), Plan::Liveries(vec![(r.clone(), "P".into())]));
    }

    #[test]
    fn archive_name() {
        assert_eq!(
            parse_archive_name(Path::new("Camera Angles and Speedometer-4-1-1-1792080686.zip")),
            ArchiveName { name: "Camera Angles and Speedometer".into(), nexus_id: Some(4), version: Some("1-1".into()) }
        );
        assert_eq!(parse_archive_name(Path::new("MyMod.zip")), ArchiveName { name: "MyMod".into(), nexus_id: None, version: None });
    }
}
