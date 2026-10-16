use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const STEAM: &str = r"C:\Program Files (x86)\Steam";
const APP_ID: &str = "2929250";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub dir: Option<String>,
    pub liveries_dir: String,
}

pub fn valid_game_dir(p: &Path) -> bool {
    p.join("over the hill.exe").exists() && p.join("GameAssembly.dll").exists()
}

pub fn detect() -> Option<PathBuf> {
    let vdf = std::fs::read_to_string(Path::new(STEAM).join("steamapps/libraryfolders.vdf")).unwrap_or_default();
    let re = regex::Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
    let mut libs: Vec<PathBuf> = re.captures_iter(&vdf).map(|c| PathBuf::from(c[1].replace("\\\\", "\\"))).collect();
    libs.push(PathBuf::from(STEAM));
    libs.into_iter().find_map(|l| {
        let apps = l.join("steamapps");
        let dir = apps.join("common/over the hill");
        (apps.join(format!("appmanifest_{APP_ID}.acf")).exists() && valid_game_dir(&dir)).then_some(dir)
    })
}

fn my_games() -> PathBuf {
    dirs::document_dir().unwrap_or_default().join("My Games").join("over the hill")
}
pub fn liveries_dir() -> PathBuf {
    my_games().join("LocalLiveries")
}
pub fn disabled_liveries_dir() -> PathBuf {
    my_games().join("LocalLiveries.disabled")
}

pub fn game_dir(app: &AppHandle) -> Result<PathBuf, String> {
    crate::load_settings(app)
        .game_dir
        .map(PathBuf::from)
        .filter(|p| valid_game_dir(p))
        .ok_or_else(|| "Pasta do jogo não configurada".to_string())
}

fn info(dir: Option<String>) -> GameInfo {
    GameInfo { dir, liveries_dir: liveries_dir().to_string_lossy().into_owned() }
}

#[tauri::command]
pub fn get_game(app: AppHandle) -> Result<GameInfo, String> {
    if let Ok(d) = game_dir(&app) {
        return Ok(info(Some(d.to_string_lossy().into_owned())));
    }
    let found = detect().map(|d| d.to_string_lossy().into_owned());
    let mut s = crate::load_settings(&app);
    s.game_dir = found.clone();
    crate::save_settings(&app, &s)?;
    Ok(info(found))
}

#[tauri::command]
pub fn set_game_dir(app: AppHandle, dir: String) -> Result<GameInfo, String> {
    if !valid_game_dir(Path::new(&dir)) {
        return Err("Pasta inválida: não contém over the hill.exe e GameAssembly.dll".into());
    }
    let mut s = crate::load_settings(&app);
    s.game_dir = Some(dir.clone());
    crate::save_settings(&app, &s)?;
    Ok(info(Some(dir)))
}

#[tauri::command]
pub fn game_art() -> Option<String> {
    use base64::Engine;
    let lib = Path::new(STEAM).join("appcache/librarycache");
    let hero = walkdir::WalkDir::new(lib.join(APP_ID))
        .max_depth(2)
        .into_iter()
        .flatten()
        .find(|e| e.file_name() == "library_hero.jpg")
        .map(|e| e.into_path())
        // layout antigo da Steam (não verificado)
        .or_else(|| Some(lib.join(format!("{APP_ID}_library_hero.jpg"))))?;
    let bytes = std::fs::read(hero).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}
