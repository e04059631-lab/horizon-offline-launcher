//! Cross-platform self-updater backed by GitHub Releases (`xwwerz/horizon-offline-launcher`).
//!
//! - Silently checks `/releases/latest` on startup (never blocks offline launch).
//! - Downloads the matching Windows/Linux binary to a temporary `.part` file with live progress.
//! - Verifies executable magic (`MZ` on Windows, `\x7fELF` on Linux) and optional `HORIZON_PAYLOAD` footer.
//! - Atomically replaces the running binary (`.old` rename trick on Windows, atomic `rename` on Linux)
//!   and restarts the updated launcher cleanly.

use crate::client_unpacker;
use serde::Deserialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/xwwerz/horizon-offline-launcher/releases/latest";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UpdateInfo {
    pub version: String,
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub download_url: String,
    pub asset_name: String,
    pub size_bytes: u64,
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct UpdateProgress {
    pub fraction: f32,
    pub status_text: String,
    pub percent_text: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Removes leftover `<exe>.old` binary after a Windows self-update.
pub fn cleanup_old_binary() {
    if let Ok(exe) = std::env::current_exe() {
        let old = old_backup_path(&exe);
        if old.exists() {
            let _ = fs::remove_file(&old);
        }
        if let Some(dir) = exe.parent() {
            let part = dir.join(".horizon-update.part");
            if part.exists() {
                let _ = fs::remove_file(&part);
            }
        }
    }
}

fn old_backup_path(exe: &Path) -> PathBuf {
    let mut os = exe.as_os_str().to_os_string();
    os.push(".old");
    PathBuf::from(os)
}

/// Parses semver strings like `v2.1.0`, `2.2`, `2.3.1-beta` into `(major, minor, patch)`.
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let clean = raw
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V');
    let core = clean.split('-').next()?.split('+').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Returns `true` if `remote` is strictly newer than `current`.
pub fn is_newer_version(current: &str, remote: &str) -> bool {
    match (parse_semver(current), parse_semver(remote)) {
        (Some(cur), Some(rem)) => rem > cur,
        _ => false,
    }
}

/// Formats byte size into a human-readable Russian string (e.g. `14.5 МБ`, `392 МБ`).
pub fn format_size_mb(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 100.0 {
        format!("{:.0} МБ", mb)
    } else if mb >= 10.0 {
        format!("{:.1} МБ", mb)
    } else {
        format!("{:.2} МБ", mb)
    }
}

/// Cleans Markdown headings/formatting from GitHub Release notes for crisp display in Slint.
pub fn format_release_notes(raw: &str) -> String {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        if trimmed.is_empty() {
            if !lines.is_empty() && lines.last().map(|s: &String| !s.is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            continue;
        }
        let cleaned = trimmed
            .trim_start_matches('#')
            .trim()
            .replace("**", "")
            .replace('`', "");
        let formatted = if let Some(rest) = cleaned.strip_prefix("- ").or_else(|| cleaned.strip_prefix("* ")) {
            format!("• {rest}")
        } else {
            cleaned
        };
        lines.push(formatted);
    }

    let joined = lines.join("\n").trim().to_string();
    if joined.is_empty() {
        return "Улучшения стабильности, интерфейса и совместимости.".to_string();
    }
    if joined.chars().count() > 900 {
        let short: String = joined.chars().take(897).collect();
        format!("{short}...")
    } else {
        joined
    }
}

fn build_agent(connect_secs: u64, read_secs: u64) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_secs))
        .timeout_read(Duration::from_secs(read_secs))
        .user_agent(&format!("Horizon-Offline-Launcher/{CURRENT_VERSION}"));

    if let Ok(tls) = native_tls::TlsConnector::new() {
        builder = builder.tls_connector(Arc::new(tls));
    }

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

fn pick_platform_asset(assets: &[GhAsset], client_already_installed: bool) -> Option<&GhAsset> {
    #[cfg(target_os = "windows")]
    {
        if client_already_installed {
            if let Some(a) = assets
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case("horizon-offline-launcher-slim.exe"))
            {
                return Some(a);
            }
        }
        assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("horizon-offline-launcher.exe"))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_lowercase().ends_with(".exe"))
            })
    }
    #[cfg(not(target_os = "windows"))]
    {
        if client_already_installed {
            if let Some(a) = assets
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case("horizon-offline-launcher-linux-slim"))
            {
                return Some(a);
            }
        }
        assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("horizon-offline-launcher-linux-x86_64"))
            .or_else(|| {
                assets.iter().find(|a| {
                    let low = a.name.to_lowercase();
                    low.contains("linux") && !low.ends_with(".sha256") && !low.ends_with(".sig")
                })
            })
    }
}

/// Checks GitHub Releases for a newer version of Horizon Offline Launcher.
/// Returns `Ok(Some(info))` if a newer version is available, `Ok(None)` if up to date.
pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let agent = build_agent(6, 15);
    let resp = agent
        .get(LATEST_RELEASE_API)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Не удалось проверить обновления: {e}"))?;

    let rel: GhRelease = resp
        .into_json()
        .map_err(|e| format!("Некорректный ответ GitHub API: {e}"))?;

    if rel.draft || rel.prerelease {
        return Ok(None);
    }

    if !is_newer_version(CURRENT_VERSION, &rel.tag_name) {
        return Ok(None);
    }

    let client_installed = client_unpacker::is_client_installed();
    let Some(asset) = pick_platform_asset(&rel.assets, client_installed) else {
        return Ok(None);
    };

    let clean_ver = rel
        .tag_name
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string();

    Ok(Some(UpdateInfo {
        version: clean_ver,
        tag: rel.tag_name.clone(),
        title: if rel.name.trim().is_empty() {
            format!("Horizon Offline Launcher {}", rel.tag_name)
        } else {
            rel.name.trim().to_string()
        },
        notes: format_release_notes(&rel.body),
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        size_bytes: asset.size,
        html_url: rel.html_url,
    }))
}

fn verify_downloaded_binary(path: &Path, expected_size: u64) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("Не удалось проверить файл обновления: {e}"))?;
    let len = meta.len();
    if len < 2 * 1024 * 1024 {
        return Err("Скачанный файл слишком мал или повреждён".to_string());
    }
    if expected_size > 0 && len != expected_size {
        return Err(format!(
            "Размер файла не совпадает ({} из {} байт)",
            len, expected_size
        ));
    }

    let mut file = fs::File::open(path).map_err(|e| format!("Не удалось открыть файл обновления: {e}"))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("Ошибка чтения заголовка: {e}"))?;

    #[cfg(target_os = "windows")]
    if &magic[..2] != b"MZ" {
        return Err("Скачанный файл не является корректным Windows .exe".to_string());
    }

    #[cfg(not(target_os = "windows"))]
    if &magic != b"\x7fELF" {
        return Err("Скачанный файл не является корректным Linux ELF-бинарником".to_string());
    }

    // If the file carries an embedded HORIZON_PAYLOAD footer, verify its length bounds
    if len >= 24 {
        file.seek(SeekFrom::End(-24)).map_err(|e| e.to_string())?;
        let mut footer = [0u8; 24];
        file.read_exact(&mut footer).map_err(|e| e.to_string())?;
        if &footer[8..] == b"HORIZON_PAYLOAD\0" {
            let plen = u64::from_le_bytes(footer[..8].try_into().unwrap());
            if plen + 24 >= len {
                return Err("Повреждён встроенный пакет HORIZON_PAYLOAD в обновлении".to_string());
            }
        }
    }

    Ok(())
}

/// Downloads the update binary with live progress, verifies it, replaces the current executable,
/// and spawns the new version. On success the caller should exit immediately.
pub fn download_and_install_update<F>(
    info: &UpdateInfo,
    abort_flag: Arc<AtomicBool>,
    mut progress_cb: F,
) -> Result<(), String>
where
    F: FnMut(UpdateProgress),
{
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Не удалось определить путь к лаунчеру: {e}"))?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| "Не удалось определить папку лаунчера".to_string())?;

    // Probe write permission before downloading hundreds of megabytes
    let part_path = exe_dir.join(".horizon-update.part");
    {
        let probe = fs::File::create(&part_path).map_err(|e| {
            format!(
                "Нет прав на запись в папку {}: {e}. Переместите лаунчер в папку с правами записи.",
                exe_dir.display()
            )
        })?;
        drop(probe);
    }

    progress_cb(UpdateProgress {
        fraction: 0.0,
        status_text: format!("Подключение к GitHub ({})...", info.asset_name),
        percent_text: "0%".to_string(),
    });

    let agent = build_agent(12, 120);
    let resp = match agent.get(&info.download_url).call() {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_file(&part_path);
            return Err(format!("Ошибка загрузки обновления: {e}"));
        }
    };

    let content_len = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(info.size_bytes);

    let mut reader = resp.into_reader();
    let mut out = fs::File::create(&part_path).map_err(|e| format!("Ошибка создания файла: {e}"))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_ui = Instant::now();

    loop {
        if abort_flag.load(Ordering::SeqCst) {
            drop(out);
            let _ = fs::remove_file(&part_path);
            return Err("Обновление отменено".to_string());
        }

        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(out);
                let _ = fs::remove_file(&part_path);
                return Err(format!("Обрыв загрузки: {e}"));
            }
        };

        if let Err(e) = out.write_all(&buf[..n]) {
            drop(out);
            let _ = fs::remove_file(&part_path);
            return Err(format!("Ошибка записи на диск: {e}"));
        }

        downloaded += n as u64;

        if last_ui.elapsed().as_millis() >= 80 {
            let frac = if content_len > 0 {
                (downloaded as f64 / content_len as f64).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let pct = format!("{:.0}%", (frac * 100.0).floor());
            let status = if content_len > 0 {
                format!(
                    "Скачивание: {:.1} / {:.1} МБ",
                    downloaded as f64 / 1_048_576.0,
                    content_len as f64 / 1_048_576.0
                )
            } else {
                format!("Скачивание: {:.1} МБ", downloaded as f64 / 1_048_576.0)
            };
            progress_cb(UpdateProgress {
                fraction: frac,
                status_text: status,
                percent_text: pct,
            });
            last_ui = Instant::now();
        }
    }

    out.flush().map_err(|e| format!("Ошибка записи буфера: {e}"))?;
    drop(out);

    progress_cb(UpdateProgress {
        fraction: 1.0,
        status_text: "Проверка целостности файла...".to_string(),
        percent_text: "100%".to_string(),
    });

    if let Err(e) = verify_downloaded_binary(&part_path, content_len) {
        let _ = fs::remove_file(&part_path);
        return Err(e);
    }

    progress_cb(UpdateProgress {
        fraction: 1.0,
        status_text: "Установка обновления и перезапуск...".to_string(),
        percent_text: "100%".to_string(),
    });

    replace_self_and_relaunch(&current_exe, &part_path)
}

#[cfg(target_os = "windows")]
fn replace_self_and_relaunch(current_exe: &Path, new_file: &Path) -> Result<(), String> {
    let old_path = old_backup_path(current_exe);
    if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }

    fs::rename(current_exe, &old_path)
        .map_err(|e| format!("Не удалось переименовать текущий .exe: {e}"))?;

    if let Err(e) = fs::rename(new_file, current_exe) {
        // Roll back immediately so the user never ends up with a missing exe
        let _ = fs::rename(&old_path, current_exe);
        return Err(format!("Не удалось заменить .exe: {e}"));
    }

    Command::new(current_exe)
        .spawn()
        .map_err(|e| format!("Обновление установлено, но не удалось перезапустить: {e}"))?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_self_and_relaunch(current_exe: &Path, new_file: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(new_file, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Не удалось выдать права запуска: {e}"))?;

    fs::rename(new_file, current_exe)
        .map_err(|e| format!("Не удалось заменить бинарник: {e}"))?;

    Command::new(current_exe)
        .spawn()
        .map_err(|e| format!("Обновление установлено, но не удалось перезапустить: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        assert!(is_newer_version("2.1.0", "v2.2.0"));
        assert!(is_newer_version("2.1.0", "v2.1.1"));
        assert!(is_newer_version("2.1.0", "3.0.0"));
        assert!(!is_newer_version("2.1.0", "v2.1.0"));
        assert!(!is_newer_version("2.1.0", "v2.0.0"));
        assert!(!is_newer_version("2.1.0", "v1.9.9"));
    }

    #[test]
    fn test_format_release_notes() {
        let raw = "### Что нового в v2.2.0\n- **Автообновление** — само приходит!\n```bash\ncurl ...\n```\n* Исправлены баги";
        let clean = format_release_notes(raw);
        assert!(clean.contains("Что нового в v2.2.0"));
        assert!(clean.contains("• Автообновление — само приходит!"));
        assert!(clean.contains("• Исправлены баги"));
        assert!(!clean.contains("```"));
        assert!(!clean.contains("**"));
    }
}
