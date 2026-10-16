#[cfg(windows)]
use tauri_plugin_deep_link::DeepLinkExt;
use crate::archive::{self, UA};
use crate::loaders::Loader;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::LazyLock;
use tauri::AppHandle;

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
const GAME: &str = "overthehill";

#[derive(Serialize)]
pub struct Requirement {
    name: String,
    url: String,
    notes: String,
    /// `None` = requisito externo (ex.: loader no GitHub)
    mod_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMod {
    mod_id: i64,
    name: String,
    summary: String,
    version: String,
    author: String,
    thumbnail_url: Option<String>,
    endorsements: i64,
    downloads: i64,
    updated_at: String,
    loader: Option<Loader>,
    requirements: Vec<Requirement>,
}

#[derive(Serialize)]
pub struct Account {
    name: String,
    premium: bool,
}

fn query(offset: u32) -> String {
    format!(
        r#"{{ mods(filter:{{gameDomainName:[{{value:"{GAME}"}}]}}, count:50, offset:{offset}, sort:[{{updatedAt:{{direction:DESC}}}}]){{ nodes{{ modId name summary version author thumbnailUrl endorsements downloads updatedAt description modRequirements{{ nexusRequirements{{ nodes{{ modId modName url notes }} }} }} }} }} }}"#
    )
}

async fn page(offset: u32) -> Result<Vec<Value>, String> {
    let v: Value = HTTP
        .post("https://api.nexusmods.com/v2/graphql")
        .header("User-Agent", UA)
        .json(&json!({ "query": query(offset) }))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    v.pointer("/data/mods/nodes").and_then(Value::as_array).cloned().ok_or_else(|| "resposta inesperada".to_string())
}

fn map(n: &Value) -> NexusMod {
    let s = |k: &str| n[k].as_str().unwrap_or_default().to_string();
    let d = s("description").to_lowercase();
    let requirements = n
        .pointer("/modRequirements/nexusRequirements/nodes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|r| Requirement {
                    name: r["modName"].as_str().unwrap_or_default().into(),
                    url: r["url"].as_str().unwrap_or_default().into(),
                    notes: r["notes"].as_str().unwrap_or_default().into(),
                    // a API devolve o id como string; "0" = externo
                    mod_id: r["modId"].as_str().and_then(|i| i.parse().ok()).filter(|&i| i > 0),
                })
                .collect()
        })
        .unwrap_or_default();
    NexusMod {
        mod_id: n["modId"].as_i64().unwrap_or_default(),
        name: s("name"),
        summary: s("summary"),
        version: s("version"),
        author: s("author"),
        thumbnail_url: n["thumbnailUrl"].as_str().map(String::from),
        endorsements: n["endorsements"].as_i64().unwrap_or_default(),
        downloads: n["downloads"].as_i64().unwrap_or_default(),
        updated_at: s("updatedAt"),
        loader: if d.contains("melonloader") {
            Some(Loader::Melon)
        } else if d.contains("bepinex") {
            Some(Loader::Bepinex)
        } else {
            None
        },
        requirements,
    }
}

// ponytail: 50 por página (o limite de complexidade da API barra 100 com requisitos); 3 páginas = 150 mods, aumentar quando passar disso
#[tauri::command]
pub async fn nexus_mods() -> Result<Vec<NexusMod>, String> {
    let go = async {
        let mut all = vec![];
        for p in 0..3 {
            let nodes = page(p * 50).await?;
            let last = nodes.len() < 50;
            all.extend(nodes.iter().map(map));
            if last {
                break;
            }
        }
        Ok::<_, String>(all)
    };
    go.await.map_err(|e| format!("Falha ao carregar a Nexus: {e}"))
}

// ---------- conta ----------

async fn v1(key: &str, path: &str, q: &[(&str, String)]) -> Result<Value, String> {
    let r = HTTP
        .get(format!("https://api.nexusmods.com/v1/{path}"))
        .query(q)
        .header("apikey", key)
        .header("Application-Name", "OTH Mod Manager")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .header("User-Agent", UA)
        .send()
        .await
        .map_err(|e| format!("Falha na Nexus: {e}"))?;
    let st = r.status();
    if st == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Chave da Nexus inválida".into());
    }
    if st == reqwest::StatusCode::FORBIDDEN {
        return Err("A Nexus exige Premium para download direto".into());
    }
    if !st.is_success() {
        return Err(format!("Falha na Nexus: HTTP {st}"));
    }
    r.json().await.map_err(|e| format!("Falha na Nexus: {e}"))
}

async fn validate(key: &str) -> Result<Account, String> {
    let v = v1(key, "users/validate.json", &[]).await?;
    Ok(Account { name: v["name"].as_str().unwrap_or_default().into(), premium: v["is_premium"].as_bool().unwrap_or(false) })
}

fn saved_key(app: &AppHandle) -> Option<String> {
    crate::load_settings(app).nexus_key
}

#[tauri::command]
pub async fn nexus_account(app: AppHandle) -> Result<Option<Account>, String> {
    match saved_key(&app) {
        Some(k) => validate(&k).await.map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn nexus_connect(app: AppHandle, key: String) -> Result<Account, String> {
    let key = key.trim().to_string();
    let acc = validate(&key).await?;
    // ponytail: chave em texto no settings.json do app; mover para o Credential Manager se virar problema
    let mut s = crate::load_settings(&app);
    s.nexus_key = Some(key);
    crate::save_settings(&app, &s)?;
    // quem conecta a conta quer baixar pelo botão "Mod Manager Download" do site
    #[cfg(windows)]
    let _ = app.deep_link().register("nxm");
    Ok(acc)
}

#[tauri::command]
pub fn nexus_disconnect(app: AppHandle) -> Result<(), String> {
    let mut s = crate::load_settings(&app);
    s.nexus_key = None;
    #[cfg(windows)]
    let _ = app.deep_link().unregister("nxm");
    crate::save_settings(&app, &s)
}

/// Baixa e instala um arquivo da Nexus. `file_id` vazio = arquivo principal mais recente.
/// `key`/`expires` vêm do link nxm:// (obrigatórios para quem não é Premium).
#[tauri::command]
pub async fn nexus_install(app: AppHandle, mod_id: i64, file_id: Option<i64>, key: Option<String>, expires: Option<i64>) -> Result<crate::mods::InstallResult, String> {
    let api = saved_key(&app).ok_or("Conecte sua conta da Nexus")?;
    let files = v1(&api, &format!("games/{GAME}/mods/{mod_id}/files.json"), &[("category", "main,update,optional".into())]).await?;
    let files = files["files"].as_array().ok_or("Falha na Nexus: resposta inesperada")?;
    let file = match file_id {
        Some(id) => files.iter().find(|f| f["file_id"].as_i64() == Some(id)),
        None => files
            .iter()
            .filter(|f| f["category_name"].as_str() == Some("MAIN"))
            .max_by_key(|f| f["uploaded_timestamp"].as_i64().unwrap_or(0))
            .or_else(|| files.iter().max_by_key(|f| f["uploaded_timestamp"].as_i64().unwrap_or(0))),
    }
    .ok_or("Arquivo não encontrado na Nexus")?;
    let (fid, fname) = (file["file_id"].as_i64().unwrap_or_default(), file["file_name"].as_str().unwrap_or("mod.zip").to_string());
    let fver = file["version"].as_str().unwrap_or_default().to_string();

    let mut q = vec![];
    if let (Some(k), Some(e)) = (key, expires) {
        q.push(("key", k));
        q.push(("expires", e.to_string()));
    }
    let links = v1(&api, &format!("games/{GAME}/mods/{mod_id}/files/{fid}/download_link.json"), &q).await?;
    let uri = links[0]["URI"].as_str().ok_or("Falha na Nexus: sem link de download")?;

    let tmp = archive::temp_dir();
    // o nome do arquivo precisa ficar como na Nexus: `parse_archive_name` deriva o nome do mod dele
    let dest = tmp.join(fname.replace(['/', '\\'], "_"));
    let (app2, dest2) = (app.clone(), dest.clone());
    let res = async {
        archive::download(uri, &dest, &app).await?;
        tauri::async_runtime::spawn_blocking(move || crate::mods::install_path(&app2, &dest2, Some((mod_id, fver))))
            .await
            .map_err(|e| e.to_string())?
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    res
}
