use crate::{archive, game};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const MELON_URL: &str = "https://github.com/LavaGang/MelonLoader/releases/download/v0.7.3/MelonLoader.x64.zip";
const BEPINEX_URL: &str = "https://builds.bepinex.dev/projects/bepinex_be/788/BepInEx-Unity.IL2CPP-win-x64-6.0.0-be.788%2B5b766a3.zip";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Melon,
    Bepinex,
}

const ALL: [Loader; 2] = [Loader::Melon, Loader::Bepinex];

impl Loader {
    fn label(self) -> &'static str {
        match self {
            Loader::Melon => "MelonLoader 0.7.3",
            Loader::Bepinex => "BepInEx 6.0.0-be.788",
        }
    }
    fn proxy(self) -> &'static str {
        match self {
            Loader::Melon => "version.dll",
            Loader::Bepinex => "winhttp.dll",
        }
    }
    fn url(self) -> &'static str {
        match self {
            Loader::Melon => MELON_URL,
            Loader::Bepinex => BEPINEX_URL,
        }
    }
    fn installed_marker(self) -> &'static str {
        match self {
            Loader::Melon => "MelonLoader/net6/MelonLoader.dll",
            Loader::Bepinex => "BepInEx/core/BepInEx.Unity.IL2CPP.dll",
        }
    }
    // Não verificado: confirmar após o primeiro launch real.
    fn generated_marker(self) -> &'static str {
        match self {
            Loader::Melon => "MelonLoader/Il2CppAssemblies/Assembly-CSharp.dll",
            Loader::Bepinex => "BepInEx/interop/Assembly-CSharp.dll",
        }
    }
    fn owned_paths(self) -> &'static [&'static str] {
        match self {
            Loader::Melon => &["version.dll", "version.dll.disabled", "MelonLoader"],
            Loader::Bepinex => &[
                "winhttp.dll", "winhttp.dll.disabled", "doorstop_config.ini", ".doorstop_version", "changelog.txt", "dotnet",
                "BepInEx/core", "BepInEx/interop", "BepInEx/unity-libs", "BepInEx/cache",
            ],
        }
    }
}

#[derive(Serialize)]
pub struct LoaderStatus {
    loader: Loader,
    label: &'static str,
    installed: bool,
    active: bool,
    generated: bool,
}

pub fn status(g: &Path) -> Vec<LoaderStatus> {
    ALL.iter()
        .map(|&l| LoaderStatus {
            loader: l,
            label: l.label(),
            installed: g.join(l.installed_marker()).exists(),
            active: g.join(l.proxy()).exists(),
            generated: g.join(l.generated_marker()).exists(),
        })
        .collect()
}

#[tauri::command]
pub fn loaders_status(app: AppHandle) -> Result<Vec<LoaderStatus>, String> {
    Ok(status(&game::game_dir(&app)?))
}

fn rename_err(e: std::io::Error) -> String {
    format!("Feche o jogo e tente novamente: {e}")
}

fn activate(g: &Path, chosen: Option<Loader>) -> Result<(), String> {
    for l in ALL {
        let live = g.join(l.proxy());
        let off = g.join(format!("{}.disabled", l.proxy()));
        if Some(l) == chosen {
            if off.exists() && !live.exists() {
                std::fs::rename(&off, &live).map_err(rename_err)?;
            }
        } else if live.exists() {
            let _ = std::fs::remove_file(&off);
            std::fs::rename(&live, &off).map_err(rename_err)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn install_loader(app: AppHandle, loader: Loader) -> Result<(), String> {
    let g = game::game_dir(&app)?;
    let tmp = archive::temp_dir();
    let res = async {
        let zip = tmp.join("loader.zip");
        archive::download(loader.url(), &zip, &app).await?;
        let out = tmp.join("x");
        let (z, o) = (zip.clone(), out.clone());
        tauri::async_runtime::spawn_blocking(move || archive::extract(&z, &o)).await.map_err(|e| e.to_string())??;
        let g2 = g.clone();
        tauri::async_runtime::spawn_blocking(move || archive::copy_dir_all(&out, &g2).map_err(rename_err))
            .await
            .map_err(|e| e.to_string())??;
        activate(&g, Some(loader))
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

#[tauri::command]
pub fn set_active_loader(app: AppHandle, loader: Option<Loader>) -> Result<(), String> {
    activate(&game::game_dir(&app)?, loader)
}

fn remove(p: PathBuf) -> std::io::Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else if p.exists() {
        std::fs::remove_file(p)
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn uninstall_loader(app: AppHandle, loader: Loader) -> Result<(), String> {
    let g = game::game_dir(&app)?;
    for p in loader.owned_paths() {
        remove(g.join(p)).map_err(rename_err)?;
    }
    Ok(())
}
