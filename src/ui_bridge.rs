use crate::client_unpacker;
use crate::config::LauncherConfig;
use crate::game_process::GameProcess;
use crate::mod_catalog::{self, RemoteMod};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// Include compiled slint UI modules
slint::include_modules!();

#[derive(Default)]
struct CatalogState {
    mods: Vec<RemoteMod>,
    downloading_ids: HashSet<String>,
    icons: HashMap<String, (u32, u32, Vec<u8>)>,
    query: String,
}

pub fn setup_ui_bridge(window: &AppWindow) {
    crate::backdrop::apply(window);

    let config = Arc::new(Mutex::new(LauncherConfig::load()));
    let game = Arc::new(GameProcess::new());
    let catalog_state = Arc::new(Mutex::new(CatalogState::default()));

    let (game_dir, mods_dir, saves_dir, screenshots_dir) = {
        let cfg = config.lock().unwrap();
        ensure_game_dirs(&cfg)
    };
    let icon_cache_dir = LauncherConfig::appdata_base_dir()
        .join("cache")
        .join("mod-icons");

    // 1. Initial State Binding
    {
        let cfg = config.lock().unwrap();
        window.set_nickname(cfg.username.clone().into());
        window.set_ram_gb(cfg.ram_gb as i32);
        window.set_render_distance(cfg.render_distance as i32);
        window.set_window_width(cfg.width as i32);
        window.set_window_height(cfg.height as i32);
        window.set_fullscreen(cfg.fullscreen);
        window.set_creative_mode(cfg.creative_mode);
        window.set_dark_mode(cfg.dark_mode);
        window.global::<Theme>().set_dark(cfg.dark_mode);
        window.set_server_address(cfg.last_server.clone().into());
        window.set_mods_repo_url(cfg.mods_repo_url.clone().into());
        window.set_is_playing(false);
        window.set_mods_dir_path(mods_dir.display().to_string().into());
        window.set_client_ready(cfg.find_version_dir().is_some());
        window.set_has_embedded_payload(
            client_unpacker::find_embedded_payload()
                .ok()
                .flatten()
                .is_some(),
        );
        let recent: Vec<SharedString> = cfg
            .recent_nicknames
            .iter()
            .map(|n| SharedString::from(n.as_str()))
            .collect();
        window.set_recent_nicknames(ModelRc::new(VecModel::from(recent)));
    }

    // 2. Scan Local Mods
    update_mods_ui(window, &mods_dir);

    // 3. Initial Logs (Empty by default, showing only real game logs)
    let log_entries = Arc::new(Mutex::new(Vec::<LogEntry>::new()));
    {
        let entries = log_entries.lock().unwrap().clone();
        window.set_log_lines(ModelRc::new(VecModel::from(entries)));
    }

    // 4. Callbacks: Play / Stop
    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);
        let game_clone = Arc::clone(&game);

        window.on_play(move || {
            let server_addr = if let Some(w) = window_weak.upgrade() {
                if w.get_is_unpacking() {
                    return;
                }
                let current_nick = w.get_nickname().to_string();
                if current_nick.trim().is_empty() {
                    w.set_edit_nickname("".into());
                    w.set_play_after_nickname(true);
                    w.set_show_account_dialog(true);
                    return;
                }
                let srv = w.get_server_address().to_string();
                {
                    let mut cfg = config_clone.lock().unwrap();
                    if cfg.last_server != srv {
                        cfg.last_server = srv.clone();
                        cfg.save();
                    }
                }
                srv
            } else {
                String::new()
            };

            let config_clone = Arc::clone(&config_clone);
            let game_clone = Arc::clone(&game_clone);
            let window_weak = window_weak.clone();

            thread::spawn(move || {
                let cfg = { config_clone.lock().unwrap().clone() };
                if cfg.username.trim().is_empty() {
                    let _ = window_weak.upgrade_in_event_loop(|w| {
                        w.set_edit_nickname("".into());
                        w.set_play_after_nickname(true);
                        w.set_show_account_dialog(true);
                    });
                    return;
                }
                let server_opt = if server_addr.trim().is_empty() {
                    None
                } else {
                    Some(server_addr.as_str())
                };
                match game_clone.launch(&cfg, server_opt) {
                    Ok(_) => {
                        let _ = window_weak.upgrade_in_event_loop(|w| {
                            w.set_is_playing(true);
                            w.set_has_status_message(false);
                        });
                    }
                    Err(e) => {
                        let _ = window_weak.upgrade_in_event_loop(move |w| {
                            w.set_is_playing(false);
                            w.set_status_text(format!("Ошибка запуска: {}", e).into());
                            w.set_status_is_error(true);
                            w.set_has_status_message(true);
                        });
                    }
                }
            });
        });
    }

    {
        let window_weak = window.as_weak();
        let game_clone = Arc::clone(&game);

        window.on_stop(move || {
            game_clone.kill();
            let _ = window_weak.upgrade_in_event_loop(|w| {
                w.set_is_playing(false);
                w.set_has_status_message(false);
            });
        });
    }

    // 5. Callbacks: Folders
    {
        let p = mods_dir.clone();
        window.on_open_mods_folder(move || {
            open_folder(&p);
        });
    }
    {
        let p = saves_dir.clone();
        window.on_open_saves_folder(move || {
            open_folder(&p);
        });
    }
    {
        let p = screenshots_dir.clone();
        window.on_open_screenshots_folder(move || {
            open_folder(&p);
        });
    }
    {
        let p = game_dir.clone();
        window.on_open_game_folder(move || {
            open_folder(&p);
        });
    }

    // 6. Callbacks: Local Mods Management (Refresh, Toggle .disabled, Delete)
    {
        let window_weak = window.as_weak();
        let mods_dir_clone = mods_dir.clone();
        let catalog_clone = Arc::clone(&catalog_state);
        window.on_refresh_mods(move || {
            if let Some(w) = window_weak.upgrade() {
                update_mods_ui(&w, &mods_dir_clone);
                rebuild_catalog_ui(&w, &catalog_clone, &mods_dir_clone);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let mods_dir_clone = mods_dir.clone();
        let catalog_clone = Arc::clone(&catalog_state);
        window.on_toggle_mod(move |file_name| {
            let file_name_str = file_name.to_string();
            let current_path = mods_dir_clone.join(&file_name_str);
            if current_path.exists() {
                let target_path = if let Some(stripped) = file_name_str.strip_suffix(".disabled") {
                    mods_dir_clone.join(stripped)
                } else {
                    mods_dir_clone.join(format!("{}.disabled", file_name_str))
                };
                let _ = fs::rename(&current_path, &target_path);
            }
            if let Some(w) = window_weak.upgrade() {
                update_mods_ui(&w, &mods_dir_clone);
                rebuild_catalog_ui(&w, &catalog_clone, &mods_dir_clone);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let mods_dir_clone = mods_dir.clone();
        let catalog_clone = Arc::clone(&catalog_state);
        window.on_delete_mod(move |file_name| {
            let file_name_str = file_name.to_string();
            if let Some(clean) = Path::new(&file_name_str).file_name() {
                let target_path = mods_dir_clone.join(clean);
                if target_path.is_file() {
                    let _ = fs::remove_file(&target_path);
                }
            }
            if let Some(w) = window_weak.upgrade() {
                update_mods_ui(&w, &mods_dir_clone);
                rebuild_catalog_ui(&w, &catalog_clone, &mods_dir_clone);
            }
        });
    }

    // 7. Callbacks: Remote GitHub Mod Catalog (Fetch, Search, Download)
    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);
        let catalog_clone = Arc::clone(&catalog_state);
        let mods_dir_clone = mods_dir.clone();
        let icon_cache_clone = icon_cache_dir.clone();

        window.on_refresh_catalog(move || {
            let repo_url = { config_clone.lock().unwrap().mods_repo_url.clone() };
            spawn_fetch_catalog(
                window_weak.clone(),
                Arc::clone(&catalog_clone),
                mods_dir_clone.clone(),
                icon_cache_clone.clone(),
                repo_url,
            );
        });
    }
    {
        let window_weak = window.as_weak();
        let catalog_clone = Arc::clone(&catalog_state);
        let mods_dir_clone = mods_dir.clone();
        window.on_search_catalog(move |query| {
            {
                let mut state = catalog_clone.lock().unwrap();
                state.query = query.to_string();
            }
            if let Some(w) = window_weak.upgrade() {
                rebuild_catalog_ui(&w, &catalog_clone, &mods_dir_clone);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let catalog_clone = Arc::clone(&catalog_state);
        let mods_dir_clone = mods_dir.clone();

        window.on_download_mod(move |mod_id| {
            let mod_id_str = mod_id.to_string();
            let target_mod = {
                let mut state = catalog_clone.lock().unwrap();
                if state.downloading_ids.contains(&mod_id_str) {
                    return;
                }
                let found = state.mods.iter().find(|m| m.id == mod_id_str).cloned();
                if found.is_some() {
                    state.downloading_ids.insert(mod_id_str.clone());
                }
                found
            };

            let Some(remote_mod) = target_mod else {
                return;
            };

            if let Some(w) = window_weak.upgrade() {
                w.set_catalog_error("".into());
                rebuild_catalog_ui(&w, &catalog_clone, &mods_dir_clone);
            }

            let window_weak_bg = window_weak.clone();
            let catalog_bg = Arc::clone(&catalog_clone);
            let mods_dir_bg = mods_dir_clone.clone();

            thread::spawn(move || {
                let res = mod_catalog::download_mod_file(
                    &mods_dir_bg,
                    &remote_mod.file_name,
                    &remote_mod.download_url,
                );
                {
                    let mut state = catalog_bg.lock().unwrap();
                    state.downloading_ids.remove(&mod_id_str);
                }

                let _ = window_weak_bg.upgrade_in_event_loop(move |w| {
                    match res {
                        Ok(_) => {
                            w.set_catalog_error("".into());
                        }
                        Err(e) => {
                            w.set_catalog_error(e.into());
                        }
                    }
                    update_mods_ui(&w, &mods_dir_bg);
                    rebuild_catalog_ui(&w, &catalog_bg, &mods_dir_bg);
                });
            });
        });
    }

    // 8. Callbacks: Settings & Nickname Save
    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);

        window.on_save_settings(move || {
            if let Some(w) = window_weak.upgrade() {
                let mut cfg = config_clone.lock().unwrap();
                cfg.ram_gb = w.get_ram_gb() as u32;
                cfg.render_distance = w.get_render_distance() as u32;
                cfg.width = w.get_window_width() as u32;
                cfg.height = w.get_window_height() as u32;
                cfg.fullscreen = w.get_fullscreen();
                cfg.creative_mode = w.get_creative_mode();
                cfg.dark_mode = w.get_dark_mode();
                w.global::<Theme>().set_dark(cfg.dark_mode);
                cfg.last_server = w.get_server_address().to_string();
                let repo = w.get_mods_repo_url().to_string();
                if !repo.trim().is_empty() {
                    cfg.mods_repo_url = repo;
                }
                cfg.save();
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);
        window.on_set_nickname(move |name| {
            let trimmed = name.trim().to_string();
            let mut cfg = config_clone.lock().unwrap();
            cfg.username = trimmed.clone();
            if !trimmed.is_empty() {
                cfg.recent_nicknames.retain(|n| n != &trimmed);
                cfg.recent_nicknames.insert(0, trimmed);
                cfg.recent_nicknames.truncate(5);
            }
            cfg.save();
            if let Some(w) = window_weak.upgrade() {
                let recent: Vec<SharedString> = cfg
                    .recent_nicknames
                    .iter()
                    .map(|n| SharedString::from(n.as_str()))
                    .collect();
                w.set_recent_nicknames(ModelRc::new(VecModel::from(recent)));
            }
        });
    }

    // 9. Callback: Clear Log
    {
        let window_weak = window.as_weak();
        let log_entries_clone = Arc::clone(&log_entries);
        window.on_clear_log(move || {
            log_entries_clone.lock().unwrap().clear();
            let _ = window_weak.upgrade_in_event_loop(|w| {
                w.set_log_lines(ModelRc::new(VecModel::from(Vec::new())));
            });
        });
    }

    // 10. Callback: Repair / Re-unpack Client from Embedded Payload
    {
        let window_weak = window.as_weak();
        window.on_repair_client(move || {
            if let Some(w) = window_weak.upgrade() {
                if w.get_is_playing() || w.get_is_unpacking() {
                    return;
                }
                if let Ok(Some((offset, length))) = client_unpacker::find_embedded_payload() {
                    start_unpacking_thread(&w, offset, length);
                } else {
                    w.set_status_text(
                        "Встроенный пакет клиента не найден (режим разработки)".into(),
                    );
                    w.set_status_is_error(true);
                    w.set_has_status_message(true);
                }
            }
        });
    }

    // 11. Background Polling Thread (Logs & Game Process State)
    {
        let window_weak = window.as_weak();
        let game_clone = Arc::clone(&game);
        let log_entries_clone = Arc::clone(&log_entries);

        thread::spawn(move || {
            let mut last_running = false;
            loop {
                thread::sleep(Duration::from_millis(150));

                let new_lines = game_clone.drain_logs();
                let is_running = game_clone.is_running();

                if !new_lines.is_empty() {
                    {
                        let mut entries = log_entries_clone.lock().unwrap();
                        for line in new_lines {
                            entries.push(make_log_entry(&line));
                        }
                        if entries.len() > 1000 {
                            let excess = entries.len() - 1000;
                            entries.drain(0..excess);
                        }
                    }
                    last_running = is_running;
                    let entries_clone = Arc::clone(&log_entries_clone);
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        let current_entries = entries_clone.lock().unwrap().clone();
                        w.set_log_lines(ModelRc::new(VecModel::from(current_entries)));
                        w.set_is_playing(is_running);
                    });
                } else if is_running != last_running {
                    last_running = is_running;
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        w.set_is_playing(is_running);
                    });
                }
            }
        });
    }

    // 12. Initial Background Catalog Fetch
    {
        let initial_url = { config.lock().unwrap().mods_repo_url.clone() };
        spawn_fetch_catalog(
            window.as_weak(),
            Arc::clone(&catalog_state),
            mods_dir.clone(),
            icon_cache_dir,
            initial_url,
        );
    }

    // 13. Client Unpacking Flow
    setup_client_unpacking(window);
}

fn spawn_fetch_catalog(
    window_weak: slint::Weak<AppWindow>,
    catalog_state: Arc<Mutex<CatalogState>>,
    mods_dir: PathBuf,
    icon_cache_dir: PathBuf,
    repo_url: String,
) {
    let _ = window_weak.upgrade_in_event_loop(|w| {
        w.set_catalog_loading(true);
        w.set_catalog_error("".into());
    });

    thread::spawn(move || match mod_catalog::fetch_catalog(&repo_url) {
        Ok((_resolved_url, mods)) => {
            let icon_jobs: Vec<(String, String)> = mods
                .iter()
                .filter(|m| !m.icon_url.is_empty())
                .map(|m| (m.id.clone(), m.icon_url.clone()))
                .collect();

            {
                let mut state = catalog_state.lock().unwrap();
                state.mods = mods;
            }

            let catalog_for_ui = Arc::clone(&catalog_state);
            let mods_dir_for_ui = mods_dir.clone();
            let _ = window_weak.upgrade_in_event_loop(move |w| {
                w.set_catalog_loading(false);
                w.set_catalog_error("".into());
                rebuild_catalog_ui(&w, &catalog_for_ui, &mods_dir_for_ui);
            });

            // Fetch mod icons asynchronously into cache & refresh UI once ready
            if !icon_jobs.is_empty() {
                let mut any_loaded = false;
                for (id, icon_url) in icon_jobs {
                    if let Some(rgba) =
                        mod_catalog::fetch_icon_cached(&icon_cache_dir, &id, &icon_url)
                    {
                        let mut state = catalog_state.lock().unwrap();
                        state.icons.insert(id, rgba);
                        any_loaded = true;
                    }
                }
                if any_loaded {
                    let catalog_for_icons = Arc::clone(&catalog_state);
                    let mods_dir_for_icons = mods_dir.clone();
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        rebuild_catalog_ui(&w, &catalog_for_icons, &mods_dir_for_icons);
                    });
                }
            }
        }
        Err(err) => {
            let _ = window_weak.upgrade_in_event_loop(move |w| {
                w.set_catalog_loading(false);
                w.set_catalog_error(err.into());
            });
        }
    });
}

fn rebuild_catalog_ui(
    window: &AppWindow,
    catalog_state: &Arc<Mutex<CatalogState>>,
    mods_dir: &Path,
) {
    let installed_files = installed_mod_base_names(mods_dir);
    let state = catalog_state.lock().unwrap();
    let q = state.query.trim().to_lowercase();

    let items: Vec<CatalogModInfo> = state
        .mods
        .iter()
        .filter(|m| {
            q.is_empty()
                || m.name.to_lowercase().contains(&q)
                || m.description.to_lowercase().contains(&q)
        })
        .map(|m| {
            let installed = installed_files.contains(&m.file_name.to_lowercase());
            let downloading = state.downloading_ids.contains(&m.id);
            let (has_icon, icon) = match state.icons.get(&m.id) {
                Some((w, h, bytes)) => {
                    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(bytes, *w, *h);
                    (true, Image::from_rgba8(buf))
                }
                None => (false, Image::default()),
            };

            CatalogModInfo {
                id: m.id.clone().into(),
                name: m.name.clone().into(),
                description: m.description.clone().into(),
                version: m.version.clone().into(),
                file_name: m.file_name.clone().into(),
                size_kb: m.size_kb,
                installed,
                downloading,
                has_icon,
                icon,
            }
        })
        .collect();

    window.set_catalog_mods(ModelRc::new(VecModel::from(items)));
}

fn installed_mod_base_names(mods_dir: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let file_name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let base = file_name.strip_suffix(".disabled").unwrap_or(&file_name);
            set.insert(base.to_lowercase());
        }
    }
    set
}

fn update_mods_ui(window: &AppWindow, mods_dir: &Path) {
    let mods = scan_mods(mods_dir);
    let active_count = mods.iter().filter(|m| m.enabled).count() as i32;
    let total_count = mods.len() as i32;
    window.set_mods(ModelRc::new(VecModel::from(mods)));
    window.set_active_mods_count(active_count);
    window.set_total_mods_count(total_count);
}

fn setup_client_unpacking(window: &AppWindow) {
    if client_unpacker::is_client_installed() {
        window.set_is_unpacking(false);
        window.set_client_ready(true);
        return;
    }

    if let Ok(Some((offset, length))) = client_unpacker::find_embedded_payload() {
        start_unpacking_thread(window, offset, length);

        let window_weak = window.as_weak();
        window.on_retry_unpack(move || {
            if let Some(w) = window_weak.upgrade() {
                w.set_unpack_has_error(false);
                w.set_unpack_error_text("".into());
                start_unpacking_thread(&w, offset, length);
            }
        });
    } else {
        // Dev fallback: no embedded payload, run directly
        window.set_is_unpacking(false);
    }
}

fn start_unpacking_thread(window: &AppWindow, offset: u64, length: u64) {
    window.set_is_unpacking(true);
    window.set_unpack_progress(0.0);
    window.set_unpack_status("Подготовка к распаковке...".into());
    window.set_unpack_percent_text("0%".into());
    window.set_unpack_time_left("".into());
    window.set_unpack_has_error(false);

    let window_weak = window.as_weak();
    let abort_flag = Arc::new(AtomicBool::new(false));

    thread::spawn(move || {
        let window_weak_cb = window_weak.clone();
        let res = client_unpacker::unpack_embedded_payload(
            offset,
            length,
            abort_flag,
            move |p| {
                let _ = window_weak_cb.upgrade_in_event_loop(move |w| {
                    w.set_unpack_progress(p.progress);
                    w.set_unpack_status(p.status.into());
                    w.set_unpack_percent_text(p.percent_text.into());
                    w.set_unpack_time_left(p.time_left.into());
                });
            },
        );

        match res {
            Ok(()) => {
                let _ = window_weak.upgrade_in_event_loop(|w| {
                    w.set_is_unpacking(false);
                    w.set_client_ready(true);
                });
            }
            Err(e) => {
                let _ = window_weak.upgrade_in_event_loop(move |w| {
                    w.set_unpack_has_error(true);
                    w.set_unpack_error_text(e.into());
                });
            }
        }
    });
}

fn ensure_game_dirs(config: &LauncherConfig) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let game_dir = config.get_game_dir();
    let mods_dir = game_dir.join("horizon-mods");
    let saves_dir = game_dir.join("saves");
    let screenshots_dir = game_dir.join("screenshots");

    let _ = fs::create_dir_all(&game_dir);
    let _ = fs::create_dir_all(&mods_dir);
    let _ = fs::create_dir_all(&saves_dir);
    let _ = fs::create_dir_all(&screenshots_dir);

    // Ensure options.txt exists with Russian language default
    let options_path = game_dir.join("options.txt");
    if !options_path.exists() {
        let default_options = "version:5023\nlang:ru_ru\nforceUnicodeFont:false\n";
        let _ = fs::write(&options_path, default_options);
    } else if let Ok(content) = fs::read_to_string(&options_path) {
        if !content.contains("lang:") {
            let mut updated = content;
            updated.push_str("\nlang:ru_ru\n");
            let _ = fs::write(&options_path, updated);
        } else if content.contains("lang:en_us") {
            let updated = content.replace("lang:en_us", "lang:ru_ru");
            let _ = fs::write(&options_path, updated);
        }
    }

    (game_dir, mods_dir, saves_dir, screenshots_dir)
}

fn scan_mods(mods_dir: &Path) -> Vec<ModInfo> {
    let mut mods = Vec::new();
    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let file_name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let (enabled, base_name) = if let Some(stripped) = file_name.strip_suffix(".disabled") {
                (false, stripped.to_string())
            } else {
                (true, file_name.clone())
            };

            let ext = Path::new(&base_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "hmod" || ext == "hnmod" || ext == "jar" || ext == "zip" {
                let size_kb = entry.metadata().map(|m| m.len() / 1024).unwrap_or(0) as i32;
                mods.push(ModInfo {
                    name: base_name.into(),
                    file_name: file_name.into(),
                    size_kb,
                    enabled,
                });
            }
        }
    }
    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    mods
}

fn make_log_entry(line: &str) -> LogEntry {
    let lower = line.to_lowercase();
    let (level, color) =
        if lower.contains("err") || lower.contains("ошибка") || lower.contains("fatal") {
            (2, slint::Color::from_argb_u8(255, 200, 16, 30)) // iOS red (dark variant)
        } else if lower.contains("warn") {
            (1, slint::Color::from_argb_u8(255, 178, 80, 0)) // iOS orange (dark variant)
        } else if lower.contains("vulkan") || lower.contains("horizon") || lower.contains("запуск")
        {
            (0, slint::Color::from_argb_u8(255, 0, 88, 204)) // iOS blue (dark variant)
        } else {
            (0, slint::Color::from_argb_u8(220, 28, 28, 30)) // label
        };
    LogEntry {
        text: line.into(),
        color,
        level,
    }
}

fn open_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        if Command::new("xdg-open").arg(path).spawn().is_err() {
            let _ = Command::new("gio").arg("open").arg(path).spawn();
        }
    }
}
