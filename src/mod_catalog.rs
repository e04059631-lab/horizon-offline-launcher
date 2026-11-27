//! Remote GitHub mod catalog loader, icon cache, and safe mod downloader.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCatalog {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub mods: Vec<RemoteMod>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMod {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub kind: String,
    pub file_name: String,
    pub download_url: String,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub size_kb: i32,
}

/// Normalizes user-provided GitHub URL into a direct raw `catalog.json` URL.
/// Supports:
/// - `https://github.com/user/repo` -> `https://raw.githubusercontent.com/user/repo/main/catalog.json`
/// - `https://github.com/user/repo/tree/branch` -> `https://raw.githubusercontent.com/user/repo/branch/catalog.json`
/// - `https://raw.githubusercontent.com/user/repo/main` -> `.../catalog.json`
/// - Any direct URL ending with `.json`
pub fn normalize_catalog_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            let user = parts[0];
            let repo = parts[1].trim_end_matches(".git");
            if parts.len() >= 4 && (parts[2] == "tree" || parts[2] == "blob") {
                let branch = parts[3];
                if parts.len() > 4 {
                    let tail = parts[4..].join("/");
                    if tail.ends_with(".json") {
                        return format!("https://raw.githubusercontent.com/{user}/{repo}/{branch}/{tail}");
                    }
                    return format!("https://raw.githubusercontent.com/{user}/{repo}/{branch}/{tail}/catalog.json");
                }
                return format!("https://raw.githubusercontent.com/{user}/{repo}/{branch}/catalog.json");
            }
            return format!("https://raw.githubusercontent.com/{user}/{repo}/main/catalog.json");
        }
    }

    if trimmed.ends_with(".json") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/catalog.json")
    }
}

/// Resolves relative paths like `mods/foo.hmod` or `icons/foo.png` against the catalog's base URL.
pub fn resolve_asset_url(catalog_url: &str, asset_url: &str) -> String {
    let trimmed = asset_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    let base = match catalog_url.rfind('/') {
        Some(idx) => &catalog_url[..=idx],
        None => catalog_url,
    };
    format!("{base}{}", trimmed.trim_start_matches('/'))
}

fn build_agent(connect_secs: u64, read_secs: u64) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_secs))
        .timeout_read(Duration::from_secs(read_secs))
        .user_agent("Horizon-Offline-Launcher/1.0");

    if let Ok(tls) = native_tls::TlsConnector::new() {
        builder = builder.tls_connector(Arc::new(tls));
    }

    // Honor standard HTTP(S)_PROXY environment variables if configured
    let proxy_env = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok();
    if let Some(proxy_str) = proxy_env {
        if let Ok(proxy) = ureq::Proxy::new(&proxy_str) {
            builder = builder.proxy(proxy);
        }
    }

    builder.build()
}

fn http_agent() -> ureq::Agent {
    build_agent(8, 30)
}

/// Fetches and parses `catalog.json` from the normalized URL.
pub fn fetch_catalog(raw_url: &str) -> Result<(String, Vec<RemoteMod>), String> {
    let url = normalize_catalog_url(raw_url);
    if url.is_empty() {
        return Err("URL репозитория модов не указан в настройках".to_string());
    }

    let agent = http_agent();
    let resp = agent.get(&url).call().map_err(|e| match e {
        ureq::Error::Status(404, _) => {
            "Файл catalog.json не найден (404). Проверь ссылку на GitHub-репозиторий в Настройках."
                .to_string()
        }
        ureq::Error::Status(code, _) => format!("Сервер модов вернул ошибку HTTP {code}"),
        ureq::Error::Transport(t) => {
            format!("Нет подключения к интернету или сервер недоступен ({})", t.kind())
        }
    })?;

    let mut catalog: RemoteCatalog = resp
        .into_json()
        .map_err(|e| format!("Ошибка формата catalog.json: {e}"))?;

    for m in &mut catalog.mods {
        m.download_url = resolve_asset_url(&url, &m.download_url);
        m.icon_url = resolve_asset_url(&url, &m.icon_url);
    }

    Ok((url, catalog.mods))
}

/// Downloads a mod icon (PNG/JPG) into local cache and returns its RGBA bytes + dimensions.
pub fn fetch_icon_cached(
    cache_dir: &Path,
    mod_id: &str,
    icon_url: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    if icon_url.trim().is_empty() {
        return None;
    }

    let _ = fs::create_dir_all(cache_dir);
    let safe_id: String = mod_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let cached_path = cache_dir.join(format!("{safe_id}.img"));

    let raw_bytes = if cached_path.exists() {
        fs::read(&cached_path).ok()
    } else {
        let agent = build_agent(5, 10);

        match agent.get(icon_url).call() {
            Ok(resp) => {
                let mut buf = Vec::new();
                // Cap icon size to 2 MB for safety
                if resp.into_reader().take(2 * 1024 * 1024).read_to_end(&mut buf).is_ok() {
                    let _ = fs::write(&cached_path, &buf);
                    Some(buf)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }?;

    let decoded = image::load_from_memory(&raw_bytes).ok()?;
    let resized = decoded.resize_to_fill(96, 96, image::imageops::FilterType::Triangle);
    let rgba = resized.to_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Safely downloads a mod file into `mods_dir` using a `.part` temporary file.
pub fn download_mod_file(mods_dir: &Path, file_name: &str, download_url: &str) -> Result<PathBuf, String> {
    let clean_name = Path::new(file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Некорректное имя файла мода".to_string())?;

    if clean_name.is_empty() {
        return Err("Пустое имя файла мода".to_string());
    }

    let _ = fs::create_dir_all(mods_dir);
    let final_path = mods_dir.join(clean_name);
    let part_path = mods_dir.join(format!("{clean_name}.part"));

    let agent = build_agent(10, 120);

    let resp = agent
        .get(download_url)
        .call()
        .map_err(|e| format!("Не удалось скачать {clean_name}: {e}"))?;

    let mut reader = resp.into_reader();
    let mut out = fs::File::create(&part_path)
        .map_err(|e| format!("Не удалось создать файл {}: {e}", part_path.display()))?;

    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Ошибка чтения потока: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(|e| format!("Ошибка записи на диск: {e}"))?;
        total += n as u64;
    }
    out.flush()
        .map_err(|e| format!("Ошибка сохранения файла: {e}"))?;
    drop(out);

    if total == 0 {
        let _ = fs::remove_file(&part_path);
        return Err("Скачан пустой файл".to_string());
    }

    // If a disabled version existed, remove it so the newly downloaded mod is cleanly enabled
    let disabled_path = mods_dir.join(format!("{clean_name}.disabled"));
    if disabled_path.exists() {
        let _ = fs::remove_file(&disabled_path);
    }
    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }

    fs::rename(&part_path, &final_path)
        .map_err(|e| format!("Не удалось сохранить мод: {e}"))?;

    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_live_github_catalog() {
        let url = "https://raw.githubusercontent.com/e04059631-lab/horizon-mods/main/catalog.json";
        let (resolved, mods) = fetch_catalog(url).expect("catalog fetch should succeed over HTTPS");
        assert_eq!(resolved, url);
        assert!(mods.len() >= 68, "catalog should contain all 68 mods, got {}", mods.len());
        assert!(mods.iter().any(|m| m.id == "motion-blur"));
        assert!(mods.iter().any(|m| m.id == "ruby-arsenal"));
        let tmp_cache = std::env::temp_dir().join("horizon_test_icon_cache");
        let icon = fetch_icon_cached(&tmp_cache, &mods[0].id, &mods[0].icon_url);
        assert!(icon.is_some(), "icon download and decode should succeed");
        let _ = fs::remove_dir_all(&tmp_cache);
    }
}
