use crate::client_unpacker;
use crate::config::LauncherConfig;
use crate::engine_mods;
use crate::game_process::GameProcess;
use crate::mod_catalog::{self, RemoteMod};
use crate::shader_combiner;
use crate::updater::{self, UpdateInfo};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
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
    local_icons: HashMap<String, Option<(u32, u32, Vec<u8>)>>,
    icon_cache_dir: PathBuf,
    icon_cache_index: Option<HashMap<String, PathBuf>>,
    catalog_fetch_started: bool,
    query: String,
    filter: i32, // 0 = all, 1 = shaders, 2 = resourcepacks/pvp
}

pub fn setup_ui_bridge(window: &AppWindow) {
    let config = Arc::new(Mutex::new(LauncherConfig::load()));
    {
        let cfg = config.lock().unwrap();
        let custom_p = cfg.custom_backdrop_path.as_deref().map(Path::new);
        crate::backdrop::apply_preset(window, &cfg.backdrop_preset, custom_p);
        crate::backdrop::apply_custom_thumb(window, custom_p);
        window.set_backdrop_index(crate::backdrop::preset_to_index(&cfg.backdrop_preset));
    }
    let game = Arc::new(GameProcess::new());
    let catalog_state = Arc::new(Mutex::new(CatalogState::default()));

    let (game_dir, mods_dir, saves_dir, screenshots_dir) = {
        let cfg = config.lock().unwrap();
        ensure_game_dirs(&cfg)
    };
    let icon_cache_dir = LauncherConfig::appdata_base_dir()
        .join("cache")
        .join("mod-icons");
    {
        catalog_state.lock().unwrap().icon_cache_dir = icon_cache_dir.clone();
    }

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
        window.set_app_version(updater::CURRENT_VERSION.into());
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

    // 2. One-time cleanup of legacy removed PvP/engine mod flags + Initialize enabled shaders
    {
        let mut cfg = config.lock().unwrap();
        if !cfg.legacy_engine_mods_cleaned {
            engine_mods::disable_legacy_engine_mods(&game_dir);
            cfg.legacy_engine_mods_cleaned = true;
            cfg.save();
        }
        let shader_dir = game_dir.join("shaderpacks");
        if cfg.enabled_shaders.is_empty() {
            let picked = read_picked_shader_pack(&game_dir);
            if !picked.is_empty()
                && !picked.starts_with('.')
                && !picked.starts_with("_verify_")
                && shader_dir.join(&picked).is_file()
            {
                cfg.enabled_shaders.push(picked);
                cfg.save();
            }
        } else {
            let before = cfg.enabled_shaders.len();
            cfg.enabled_shaders.retain(|s| {
                !s.starts_with('.')
                    && !s.starts_with("_verify_")
                    && shader_dir.join(s).is_file()
            });
            if cfg.enabled_shaders.len() != before {
                cfg.save();
            }
        }
        let picked = shader_combiner::sync_combined_shaders(&game_dir, &cfg.enabled_shaders);
        write_picked_shader_pack(&game_dir, &picked);
    }
    // Lightweight badge count only on startup (avoid decoding 20+ ZIP pack icons & scanning 1300+ cache files before the window even appears)
    update_mods_badge_counts(window, &game_dir, &mods_dir, &config);

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
            if game_clone.is_busy() {
                return;
            }
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
                // Re-sync combined shader pack right before launch in case a shader was updated on disk (no-op if unchanged)
                let gd = cfg.get_game_dir();
                let picked = shader_combiner::sync_combined_shaders(&gd, &cfg.enabled_shaders);
                write_picked_shader_pack(&gd, &picked);

                match game_clone.launch(&cfg, server_opt) {
                    Ok(_) => {
                        let _ = window_weak.upgrade_in_event_loop(|w| {
                            w.set_is_playing(true);
                            w.set_has_status_message(false);
                            // Auto-minimize launcher so Slint/GPU uses 0% GPU/VRAM while Minecraft launches & runs
                            w.window().set_minimized(true);
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
                w.window().set_minimized(false);
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

    // 6. Callbacks: Local Mods & Shader Packs Management (Refresh, Toggle, Delete)
    {
        let window_weak = window.as_weak();
        let game_dir_clone = game_dir.clone();
        let mods_dir_clone = mods_dir.clone();
        let catalog_clone = Arc::clone(&catalog_state);
        let config_clone = Arc::clone(&config);
        let game_clone = Arc::clone(&game);
        let icon_cache_clone = icon_cache_dir.clone();
        window.on_refresh_mods(move || {
            if let Some(w) = window_weak.upgrade() {
                update_mods_ui(&w, &game_dir_clone, &mods_dir_clone, &config_clone, &catalog_clone);
                rebuild_catalog_ui(&w, &catalog_clone, &game_dir_clone, &mods_dir_clone);
                let should_fetch = {
                    let mut st = catalog_clone.lock().unwrap();
                    if !st.catalog_fetch_started {
                        st.catalog_fetch_started = true;
                        true
                    } else {
                        false
                    }
                };
                if should_fetch {
                    let repo_url = { config_clone.lock().unwrap().mods_repo_url.clone() };
                    spawn_fetch_catalog(
                        window_weak.clone(),
                        Arc::clone(&catalog_clone),
                        Arc::clone(&config_clone),
                        Arc::clone(&game_clone),
                        game_dir_clone.clone(),
                        mods_dir_clone.clone(),
                        icon_cache_clone.clone(),
                        repo_url,
                    );
                }
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let game_dir_clone = game_dir.clone();
        let mods_dir_clone = mods_dir.clone();
        let catalog_clone = Arc::clone(&catalog_state);
        let config_clone = Arc::clone(&config);
        window.on_toggle_mod(move |file_name| {
            let file_name_str = file_name.to_string();
            let rp_path = game_dir_clone.join("resourcepacks").join(&file_name_str);
            if is_shader_file(&file_name_str) {
                let mut cfg = config_clone.lock().unwrap();
                if let Some(pos) = cfg
                    .enabled_shaders
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(&file_name_str))
                {
                    cfg.enabled_shaders.remove(pos);
                } else {
                    cfg.enabled_shaders.push(file_name_str.clone());
                }
                let picked =
                    shader_combiner::sync_combined_shaders(&game_dir_clone, &cfg.enabled_shaders);
                write_picked_shader_pack(&game_dir_clone, &picked);
                cfg.save();
            } else if rp_path.is_file() {
                let currently_on = is_resource_pack_enabled(&game_dir_clone, &file_name_str);
                set_resource_pack_enabled(&game_dir_clone, &file_name_str, !currently_on);
            } else {
                let current_path = mods_dir_clone.join(&file_name_str);
                if current_path.exists() {
                    if let Some(stripped) = file_name_str.strip_suffix(".disabled") {
                        let target_path = mods_dir_clone.join(stripped);
                        let _ = fs::rename(&current_path, &target_path);
                        if is_hmod_file(stripped) {
                            engine_mods::set_hmod_enabled(&game_dir_clone, &target_path, true);
                        }
                    } else if is_hmod_file(&file_name_str) {
                        let currently_on = engine_mods::is_hmod_enabled(&game_dir_clone, &current_path);
                        engine_mods::set_hmod_enabled(&game_dir_clone, &current_path, !currently_on);
                    } else {
                        let target_path = mods_dir_clone.join(format!("{}.disabled", file_name_str));
                        let _ = fs::rename(&current_path, &target_path);
                    }
                }
            }
            if let Some(w) = window_weak.upgrade() {
                update_mods_ui(&w, &game_dir_clone, &mods_dir_clone, &config_clone, &catalog_clone);
                rebuild_catalog_ui(&w, &catalog_clone, &game_dir_clone, &mods_dir_clone);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let game_dir_clone = game_dir.clone();
        let mods_dir_clone = mods_dir.clone();
        let catalog_clone = Arc::clone(&catalog_state);
        let config_clone = Arc::clone(&config);
        window.on_delete_mod(move |file_name| {
            let file_name_str = file_name.to_string();
            if let Some(clean) = Path::new(&file_name_str).file_name() {
                let rp_path = game_dir_clone.join("resourcepacks").join(clean);
                if is_shader_file(&file_name_str) {
                    let target_path = game_dir_clone.join("shaderpacks").join(clean);
                    if target_path.is_file() {
                        let _ = fs::remove_file(&target_path);
                    }
                    let mut cfg = config_clone.lock().unwrap();
                    cfg.enabled_shaders
                        .retain(|s| !s.eq_ignore_ascii_case(&file_name_str));
                    let picked = shader_combiner::sync_combined_shaders(
                        &game_dir_clone,
                        &cfg.enabled_shaders,
                    );
                    write_picked_shader_pack(&game_dir_clone, &picked);
                    cfg.save();
                } else if rp_path.is_file() {
                    let _ = fs::remove_file(&rp_path);
                    set_resource_pack_enabled(&game_dir_clone, &file_name_str, false);
                } else {
                    let target_path = mods_dir_clone.join(clean);
                    if target_path.is_file() {
                        if is_hmod_file(&file_name_str) {
                            engine_mods::remove_hmod_entry(&game_dir_clone, &target_path);
                        }
                        let _ = fs::remove_file(&target_path);
                    }
                    let disabled_path = mods_dir_clone.join(format!("{}.disabled", clean.to_string_lossy()));
                    if disabled_path.is_file() {
                        let _ = fs::remove_file(&disabled_path);
                    }
                }
            }
            if let Some(w) = window_weak.upgrade() {
                update_mods_ui(&w, &game_dir_clone, &mods_dir_clone, &config_clone, &catalog_clone);
                rebuild_catalog_ui(&w, &catalog_clone, &game_dir_clone, &mods_dir_clone);
            }
        });
    }

    // 7. Callbacks: Remote GitHub Mod Catalog (Fetch, Search, Download)
    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);
        let catalog_clone = Arc::clone(&catalog_state);
        let game_clone = Arc::clone(&game);
        let game_dir_clone = game_dir.clone();
        let mods_dir_clone = mods_dir.clone();
        let icon_cache_clone = icon_cache_dir.clone();

        window.on_refresh_catalog(move || {
            {
                catalog_clone.lock().unwrap().catalog_fetch_started = true;
            }
            let repo_url = { config_clone.lock().unwrap().mods_repo_url.clone() };
            spawn_fetch_catalog(
                window_weak.clone(),
                Arc::clone(&catalog_clone),
                Arc::clone(&config_clone),
                Arc::clone(&game_clone),
                game_dir_clone.clone(),
                mods_dir_clone.clone(),
                icon_cache_clone.clone(),
                repo_url,
            );
        });
    }
    {
        let window_weak = window.as_weak();
        let catalog_clone = Arc::clone(&catalog_state);
        let game_dir_clone = game_dir.clone();
        let mods_dir_clone = mods_dir.clone();
        window.on_search_catalog(move |query| {
            {
                let mut state = catalog_clone.lock().unwrap();
                state.query = query.to_string();
            }
            if let Some(w) = window_weak.upgrade() {
                rebuild_catalog_ui(&w, &catalog_clone, &game_dir_clone, &mods_dir_clone);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let catalog_clone = Arc::clone(&catalog_state);
        let game_dir_clone = game_dir.clone();
        let mods_dir_clone = mods_dir.clone();
        window.on_filter_catalog(move |filter_idx| {
            {
                let mut state = catalog_clone.lock().unwrap();
                state.filter = filter_idx;
            }
            if let Some(w) = window_weak.upgrade() {
                rebuild_catalog_ui(&w, &catalog_clone, &game_dir_clone, &mods_dir_clone);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let catalog_clone = Arc::clone(&catalog_state);
        let config_clone = Arc::clone(&config);
        let game_dir_clone = game_dir.clone();
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
                rebuild_catalog_ui(&w, &catalog_clone, &game_dir_clone, &mods_dir_clone);
            }

            let window_weak_bg = window_weak.clone();
            let catalog_bg = Arc::clone(&catalog_clone);
            let config_bg = Arc::clone(&config_clone);
            let game_dir_bg = game_dir_clone.clone();
            let mods_dir_bg = mods_dir_clone.clone();

            thread::spawn(move || {
                let (is_shader, is_rp) = mod_kind_flags(&remote_mod.kind, &remote_mod.file_name);
                let target_dir = if is_shader {
                    game_dir_bg.join("shaderpacks")
                } else if is_rp {
                    game_dir_bg.join("resourcepacks")
                } else {
                    mods_dir_bg.clone()
                };
                let res = mod_catalog::download_mod_file(
                    &target_dir,
                    &remote_mod.file_name,
                    &remote_mod.download_url,
                );
                if let Ok(ref saved_path) = res {
                    if is_shader {
                        let mut cfg = config_bg.lock().unwrap();
                        if !cfg
                            .enabled_shaders
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(&remote_mod.file_name))
                        {
                            cfg.enabled_shaders.push(remote_mod.file_name.clone());
                        }
                        let picked = shader_combiner::sync_combined_shaders(
                            &game_dir_bg,
                            &cfg.enabled_shaders,
                        );
                        write_picked_shader_pack(&game_dir_bg, &picked);
                        cfg.save();
                    } else if is_rp {
                        set_resource_pack_enabled(&game_dir_bg, &remote_mod.file_name, true);
                    } else if is_hmod_file(&remote_mod.file_name) {
                        engine_mods::set_hmod_enabled(&game_dir_bg, saved_path, true);
                    }
                }
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
                    update_mods_ui(&w, &game_dir_bg, &mods_dir_bg, &config_bg, &catalog_bg);
                    rebuild_catalog_ui(&w, &catalog_bg, &game_dir_bg, &mods_dir_bg);
                });
            });
        });
    }

    // 8. Callbacks: Settings, Wallpaper Switcher & Nickname Save
    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);
        window.on_select_backdrop(move |idx| {
            let preset_name = crate::backdrop::index_to_preset(idx).to_string();
            let custom_path = {
                let mut cfg = config_clone.lock().unwrap();
                cfg.backdrop_preset = preset_name.clone();
                cfg.save();
                cfg.custom_backdrop_path.clone()
            };
            if let Some(w) = window_weak.upgrade() {
                w.set_backdrop_index(crate::backdrop::preset_to_index(&preset_name));
                crate::backdrop::apply_preset(
                    &w,
                    &preset_name,
                    custom_path.as_deref().map(Path::new),
                );
            }
        });
    }
    {
        let window_weak = window.as_weak();
        let config_clone = Arc::clone(&config);
        window.on_pick_custom_backdrop(move || {
            let w_weak = window_weak.clone();
            let cfg_arc = Arc::clone(&config_clone);
            thread::spawn(move || {
                let Some(picked) = crate::backdrop::pick_image_file() else {
                    return;
                };
                let dest_dir = LauncherConfig::appdata_base_dir();
                match crate::backdrop::import_custom_backdrop(&picked, &dest_dir) {
                    Ok(saved) => {
                        let saved_str = saved.to_string_lossy().to_string();
                        {
                            let mut cfg = cfg_arc.lock().unwrap();
                            cfg.backdrop_preset = "custom".to_string();
                            cfg.custom_backdrop_path = Some(saved_str);
                            cfg.save();
                        }
                        let _ = w_weak.upgrade_in_event_loop(move |w| {
                            w.set_backdrop_index(4);
                            crate::backdrop::apply_custom_thumb(&w, Some(&saved));
                            crate::backdrop::apply_preset(&w, "custom", Some(&saved));
                        });
                    }
                    Err(err) => {
                        let _ = w_weak.upgrade_in_event_loop(move |w| {
                            w.set_status_text(err.into());
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
                thread::sleep(Duration::from_millis(if last_running { 250 } else { 300 }));

                let new_lines = game_clone.drain_logs();
                let is_running = game_clone.is_running();
                let should_restore = last_running && !is_running;

                if !new_lines.is_empty() {
                    let new_entries: Vec<LogEntry> =
                        new_lines.iter().map(|l| make_log_entry(l)).collect();
                    {
                        let mut entries = log_entries_clone.lock().unwrap();
                        entries.extend(new_entries.iter().cloned());
                        if entries.len() > 1000 {
                            let excess = entries.len() - 1000;
                            entries.drain(0..excess);
                        }
                    }
                    last_running = is_running;
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        let model_rc = w.get_log_lines();
                        if let Some(vec_model) = model_rc.as_any().downcast_ref::<VecModel<LogEntry>>() {
                            for entry in new_entries {
                                vec_model.push(entry);
                            }
                            while vec_model.row_count() > 1000 {
                                vec_model.remove(0);
                            }
                        } else {
                            w.set_log_lines(ModelRc::new(VecModel::from(new_entries)));
                        }
                        w.set_is_playing(is_running);
                        if should_restore {
                            w.window().set_minimized(false);
                        }
                    });
                } else if is_running != last_running {
                    last_running = is_running;
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        w.set_is_playing(is_running);
                        if should_restore {
                            w.window().set_minimized(false);
                        }
                    });
                }
            }
        });
    }

    // 12. Catalog is now fetched lazily when the user opens the Mods tab (zero CPU/RAM/disk contention on game launch!)

    // 13. Client Unpacking Flow
    setup_client_unpacking(window);

    // 14. Auto-Updater (Background check on launch + manual check + in-app download & restart)
    let pending_update: Arc<Mutex<Option<UpdateInfo>>> = Arc::new(Mutex::new(None));
    let update_abort: Arc<Mutex<Option<Arc<AtomicBool>>>> = Arc::new(Mutex::new(None));

    {
        let window_weak = window.as_weak();
        let pending_clone = Arc::clone(&pending_update);
        window.on_check_updates(move || {
            spawn_check_updates(window_weak.clone(), Arc::clone(&pending_clone), true);
        });
    }

    {
        let window_weak = window.as_weak();
        let pending_clone = Arc::clone(&pending_update);
        let abort_slot = Arc::clone(&update_abort);
        window.on_start_update(move || {
            let Some(info) = pending_clone.lock().unwrap().clone() else {
                return;
            };
            let abort_flag = Arc::new(AtomicBool::new(false));
            *abort_slot.lock().unwrap() = Some(Arc::clone(&abort_flag));

            let _ = window_weak.upgrade_in_event_loop(|w| {
                w.set_update_downloading(true);
                w.set_update_error_text("".into());
                w.set_update_progress(0.0);
                w.set_update_percent_text("0%".into());
                w.set_update_status_text("Подготовка к загрузке...".into());
            });

            let w_weak = window_weak.clone();
            let abort_slot_done = Arc::clone(&abort_slot);
            thread::spawn(move || {
                let w_prog = w_weak.clone();
                let res = updater::download_and_install_update(&info, Arc::clone(&abort_flag), move |p| {
                    let _ = w_prog.upgrade_in_event_loop(move |w| {
                        w.set_update_progress(p.fraction);
                        w.set_update_status_text(p.status_text.into());
                        w.set_update_percent_text(p.percent_text.into());
                    });
                });

                *abort_slot_done.lock().unwrap() = None;

                match res {
                    Ok(()) => {
                        // New updated launcher has already been spawned; exit cleanly
                        std::process::exit(0);
                    }
                    Err(err) => {
                        let cancelled = abort_flag.load(std::sync::atomic::Ordering::SeqCst);
                        let _ = w_weak.upgrade_in_event_loop(move |w| {
                            w.set_update_downloading(false);
                            if cancelled {
                                w.set_show_update_dialog(false);
                                w.set_update_error_text("".into());
                            } else {
                                w.set_update_error_text(err.into());
                            }
                        });
                    }
                }
            });
        });
    }

    {
        let window_weak = window.as_weak();
        let abort_slot = Arc::clone(&update_abort);
        window.on_cancel_update(move || {
            if let Some(flag) = abort_slot.lock().unwrap().as_ref() {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            let _ = window_weak.upgrade_in_event_loop(|w| {
                w.set_update_downloading(false);
                w.set_show_update_dialog(false);
                w.set_update_error_text("".into());
            });
        });
    }

    // Automatic silent update check shortly after startup
    spawn_check_updates(window.as_weak(), Arc::clone(&pending_update), false);
}

fn spawn_check_updates(
    window_weak: slint::Weak<AppWindow>,
    pending_update: Arc<Mutex<Option<UpdateInfo>>>,
    manual: bool,
) {
    if manual {
        let _ = window_weak.upgrade_in_event_loop(|w| {
            w.set_update_checking(true);
            w.set_has_status_message(false);
        });
    }

    thread::spawn(move || {
        if !manual {
            thread::sleep(Duration::from_millis(1200));
        }

        match updater::check_for_update() {
            Ok(Some(info)) => {
                // If automatic check, wait until first-run unpacking finishes before showing the sheet
                if !manual {
                    for _ in 0..600 {
                        let unpacking = Arc::new(AtomicBool::new(false));
                        let flag = Arc::clone(&unpacking);
                        let _ = window_weak.upgrade_in_event_loop(move |w| {
                            flag.store(w.get_is_unpacking(), std::sync::atomic::Ordering::SeqCst);
                        });
                        thread::sleep(Duration::from_millis(250));
                        if !unpacking.load(std::sync::atomic::Ordering::SeqCst) {
                            break;
                        }
                    }
                }

                let ver = info.version.clone();
                let notes = info.notes.clone();
                let size_text = updater::format_size_mb(info.size_bytes);
                *pending_update.lock().unwrap() = Some(info);

                let _ = window_weak.upgrade_in_event_loop(move |w| {
                    w.set_update_checking(false);
                    w.set_update_new_version(ver.into());
                    w.set_update_notes(notes.into());
                    w.set_update_size_text(size_text.into());
                    w.set_update_error_text("".into());
                    w.set_update_downloading(false);
                    w.set_update_progress(0.0);
                    w.set_show_update_dialog(true);
                });
            }
            Ok(None) => {
                if manual {
                    let _ = window_weak.upgrade_in_event_loop(|w| {
                        w.set_update_checking(false);
                        w.set_status_text(
                            format!("Установлена последняя версия (v{})", updater::CURRENT_VERSION)
                                .into(),
                        );
                        w.set_status_is_error(false);
                        w.set_has_status_message(true);
                    });
                }
            }
            Err(err) => {
                if manual {
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        w.set_update_checking(false);
                        w.set_status_text(err.into());
                        w.set_status_is_error(true);
                        w.set_has_status_message(true);
                    });
                }
            }
        }
    });
}

fn spawn_fetch_catalog(
    window_weak: slint::Weak<AppWindow>,
    catalog_state: Arc<Mutex<CatalogState>>,
    config: Arc<Mutex<LauncherConfig>>,
    game: Arc<GameProcess>,
    game_dir: PathBuf,
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
            // Prioritize icons for installed mods first, then top catalog items in order
            let installed_files = installed_mod_base_names(&game_dir, &mods_dir);
            let installed_ids: HashSet<String> = mods
                .iter()
                .filter(|m| installed_files.contains(&m.file_name.to_lowercase()))
                .map(|m| m.id.clone())
                .collect();

            let mut icon_jobs: Vec<(String, String)> = mods
                .iter()
                .filter(|m| !m.icon_url.is_empty() && installed_ids.contains(&m.id))
                .map(|m| (m.id.clone(), m.icon_url.clone()))
                .collect();
            icon_jobs.extend(
                mods.iter()
                    .filter(|m| !m.icon_url.is_empty() && !installed_ids.contains(&m.id))
                    .map(|m| (m.id.clone(), m.icon_url.clone())),
            );

            {
                let mut state = catalog_state.lock().unwrap();
                state.mods = mods;
            }

            let catalog_for_ui = Arc::clone(&catalog_state);
            let config_for_ui = Arc::clone(&config);
            let game_dir_for_ui = game_dir.clone();
            let mods_dir_for_ui = mods_dir.clone();
            let _ = window_weak.upgrade_in_event_loop(move |w| {
                w.set_catalog_loading(false);
                w.set_catalog_error("".into());
                update_mods_ui(&w, &game_dir_for_ui, &mods_dir_for_ui, &config_for_ui, &catalog_for_ui);
                rebuild_catalog_ui(&w, &catalog_for_ui, &game_dir_for_ui, &mods_dir_for_ui);
            });

            // Fetch mod icons in 2 gentle worker threads and pause completely while Minecraft is launching/running
            if !icon_jobs.is_empty() {
                let queue = Arc::new(Mutex::new(icon_jobs.into_iter()));
                let installed_ids_arc = Arc::new(installed_ids);
                let mut handles = Vec::new();
                for _ in 0..2 {
                    let q = Arc::clone(&queue);
                    let cache_dir = icon_cache_dir.clone();
                    let state_arc = Arc::clone(&catalog_state);
                    let cfg_arc = Arc::clone(&config);
                    let game_arc = Arc::clone(&game);
                    let inst_arc = Arc::clone(&installed_ids_arc);
                    let w_weak = window_weak.clone();
                    let g_dir = game_dir.clone();
                    let m_dir = mods_dir.clone();
                    handles.push(thread::spawn(move || {
                        let mut batch: Vec<(String, u32, u32, Vec<u8>)> = Vec::with_capacity(12);
                        let mut touched_installed = false;
                        loop {
                            while game_arc.is_busy() {
                                thread::sleep(Duration::from_millis(500));
                            }
                            let next = { q.lock().unwrap().next() };
                            let Some((id, icon_url)) = next else { break };
                            if let Some((iw, ih, bytes)) =
                                mod_catalog::fetch_icon_cached(&cache_dir, &id, &icon_url)
                            {
                                if inst_arc.contains(&id) {
                                    touched_installed = true;
                                }
                                {
                                    let mut state = state_arc.lock().unwrap();
                                    state.icons.insert(id.clone(), (iw, ih, bytes.clone()));
                                }
                                batch.push((id, iw, ih, bytes));
                                if batch.len() >= 10 {
                                    let flush = std::mem::take(&mut batch);
                                    let refresh_my = touched_installed;
                                    touched_installed = false;
                                    let c = Arc::clone(&state_arc);
                                    let cfg = Arc::clone(&cfg_arc);
                                    let gd = g_dir.clone();
                                    let md = m_dir.clone();
                                    let _ = w_weak.upgrade_in_event_loop(move |w| {
                                        apply_catalog_icons_in_place(&w, &flush);
                                        if refresh_my {
                                            update_mods_ui(&w, &gd, &md, &cfg, &c);
                                        }
                                    });
                                }
                            }
                        }
                        if !batch.is_empty() {
                            let refresh_my = touched_installed;
                            let c = Arc::clone(&state_arc);
                            let cfg = Arc::clone(&cfg_arc);
                            let gd = g_dir.clone();
                            let md = m_dir.clone();
                            let _ = w_weak.upgrade_in_event_loop(move |w| {
                                apply_catalog_icons_in_place(&w, &batch);
                                if refresh_my {
                                    update_mods_ui(&w, &gd, &md, &cfg, &c);
                                }
                            });
                        }
                    }));
                }
                for h in handles {
                    let _ = h.join();
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

fn apply_catalog_icons_in_place(window: &AppWindow, batch: &[(String, u32, u32, Vec<u8>)]) {
    let model_rc = window.get_catalog_mods();
    let Some(vec_model) = model_rc.as_any().downcast_ref::<VecModel<CatalogModInfo>>() else {
        return;
    };
    let lookup: HashMap<&str, (u32, u32, &[u8])> = batch
        .iter()
        .map(|(id, w, h, b)| (id.as_str(), (*w, *h, b.as_slice())))
        .collect();
    let len = vec_model.row_count();
    for i in 0..len {
        if let Some(mut row) = vec_model.row_data(i) {
            if let Some(&(w, h, bytes)) = lookup.get(row.id.as_str()) {
                let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(bytes, w, h);
                row.has_icon = true;
                row.icon = Image::from_rgba8(buf);
                vec_model.set_row_data(i, row);
            }
        }
    }
}

fn rebuild_catalog_ui(
    window: &AppWindow,
    catalog_state: &Arc<Mutex<CatalogState>>,
    game_dir: &Path,
    mods_dir: &Path,
) {
    let installed_files = installed_mod_base_names(game_dir, mods_dir);
    let state = catalog_state.lock().unwrap();
    let q = state.query.trim().to_lowercase();

    let items: Vec<CatalogModInfo> = state
        .mods
        .iter()
        .filter(|m| {
            let (is_shader, is_rp) = mod_kind_flags(&m.kind, &m.file_name);
            let cat_ok = match state.filter {
                1 => is_shader,
                2 => is_rp,
                _ => true,
            };
            let search_ok = q.is_empty()
                || m.name.to_lowercase().contains(&q)
                || m.description.to_lowercase().contains(&q);
            cat_ok && search_ok
        })
        .map(|m| {
            let (is_shader, is_rp) = mod_kind_flags(&m.kind, &m.file_name);
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
                is_shader,
                is_resourcepack: is_rp,
            }
        })
        .collect();

    window.set_catalog_mods(ModelRc::new(VecModel::from(items)));
}

fn is_shader_file(file_name: &str) -> bool {
    let base = file_name.strip_suffix(".disabled").unwrap_or(file_name);
    Path::new(base)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("hshader"))
        .unwrap_or(false)
}

fn is_hmod_file(file_name: &str) -> bool {
    let base = file_name.strip_suffix(".disabled").unwrap_or(file_name);
    Path::new(base)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("hmod"))
        .unwrap_or(false)
}

fn read_picked_shader_pack(game_dir: &Path) -> String {
    let gfx_path = game_dir.join("horizon-graphics.txt");
    let Ok(content) = fs::read_to_string(&gfx_path) else {
        return String::new();
    };
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("pickedShaderPack:") {
            return val.trim().to_string();
        }
    }
    String::new()
}

fn write_picked_shader_pack(game_dir: &Path, pack_name: &str) {
    let gfx_path = game_dir.join("horizon-graphics.txt");
    let target_line = format!("pickedShaderPack:{pack_name}");
    if let Ok(content) = fs::read_to_string(&gfx_path) {
        let mut found = false;
        let mut lines: Vec<String> = content
            .lines()
            .map(|l| {
                if l.starts_with("pickedShaderPack:") {
                    found = true;
                    target_line.clone()
                } else {
                    l.to_string()
                }
            })
            .collect();
        if !found {
            lines.push(target_line);
        }
        let mut out = lines.join("\n");
        out.push('\n');
        let _ = fs::write(&gfx_path, out);
    } else {
        let _ = fs::write(&gfx_path, format!("{target_line}\n"));
    }
}

fn mod_kind_flags(kind_str: &str, file_name: &str) -> (bool, bool) {
    let is_shader = kind_str.eq_ignore_ascii_case("shader") || is_shader_file(file_name);
    let base = file_name.strip_suffix(".disabled").unwrap_or(file_name);
    let is_zip = Path::new(base)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false);
    let is_rp = !is_shader && (kind_str.eq_ignore_ascii_case("resourcepack") || is_zip);
    (is_shader, is_rp)
}

fn read_enabled_resource_packs(game_dir: &Path) -> Vec<String> {
    let opt_path = game_dir.join("options.txt");
    let Ok(content) = fs::read_to_string(&opt_path) else {
        return vec!["vanilla".to_string()];
    };
    for line in content.lines() {
        if let Some(raw_json) = line.strip_prefix("resourcePacks:") {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(raw_json.trim()) {
                return list;
            }
        }
    }
    vec!["vanilla".to_string()]
}

fn is_resource_pack_enabled(game_dir: &Path, file_name: &str) -> bool {
    let entry = format!("file/{file_name}");
    read_enabled_resource_packs(game_dir)
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&entry) || p.eq_ignore_ascii_case(file_name))
}

fn set_resource_pack_enabled(game_dir: &Path, file_name: &str, enable: bool) {
    let mut packs = read_enabled_resource_packs(game_dir);
    let entry = format!("file/{file_name}");
    packs.retain(|p| !p.eq_ignore_ascii_case(&entry) && !p.eq_ignore_ascii_case(file_name));
    if !packs.iter().any(|p| p == "vanilla") {
        packs.insert(0, "vanilla".to_string());
    }
    if enable {
        packs.push(entry);
    }

    let serialized = serde_json::to_string(&packs).unwrap_or_else(|_| "[\"vanilla\"]".to_string());
    let target_line = format!("resourcePacks:{serialized}");
    let opt_path = game_dir.join("options.txt");

    if let Ok(content) = fs::read_to_string(&opt_path) {
        let mut found = false;
        let mut lines: Vec<String> = content
            .lines()
            .map(|l| {
                if l.starts_with("resourcePacks:") {
                    found = true;
                    target_line.clone()
                } else {
                    l.to_string()
                }
            })
            .collect();
        if !found {
            lines.push(target_line);
        }
        let mut out = lines.join("\n");
        out.push('\n');
        let _ = fs::write(&opt_path, out);
    } else {
        let _ = fs::write(&opt_path, format!("version:5023\nlang:ru_ru\n{target_line}\n"));
    }
}

fn installed_mod_base_names(game_dir: &Path, mods_dir: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    for dir in [
        mods_dir.to_path_buf(),
        game_dir.join("shaderpacks"),
        game_dir.join("resourcepacks"),
    ] {
        if let Ok(entries) = fs::read_dir(&dir) {
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
                if file_name.starts_with('.') || file_name.starts_with("_verify_") {
                    continue;
                }
                let base = file_name.strip_suffix(".disabled").unwrap_or(&file_name);
                set.insert(base.to_lowercase());
            }
        }
    }
    set
}

/// Fast startup-only count of installed/enabled mods without reading ZIPs or decoding any icons.
fn update_mods_badge_counts(
    window: &AppWindow,
    game_dir: &Path,
    mods_dir: &Path,
    config: &Arc<Mutex<LauncherConfig>>,
) {
    let enabled_shaders = { config.lock().unwrap().enabled_shaders.clone() };
    let enabled_rps = read_enabled_resource_packs(game_dir);
    let mut active = 0i32;
    let mut total = 0i32;

    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() { continue; }
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with("_verify_") { continue; }
            let base = name.strip_suffix(".disabled").unwrap_or(&name);
            let ext = Path::new(base).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if matches!(ext.as_str(), "hmod" | "hnmod" | "jar" | "zip") {
                total += 1;
                if !name.ends_with(".disabled") { active += 1; }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(game_dir.join("shaderpacks")) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() { continue; }
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with("_verify_") { continue; }
            if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("hshader")).unwrap_or(false) {
                total += 1;
                if enabled_shaders.iter().any(|s| s.eq_ignore_ascii_case(&name)) { active += 1; }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(game_dir.join("resourcepacks")) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() { continue; }
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with("_verify_") { continue; }
            if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false) {
                total += 1;
                let rp_key = format!("file/{name}");
                if enabled_rps.iter().any(|item| item.eq_ignore_ascii_case(&rp_key) || item.eq_ignore_ascii_case(&name)) {
                    active += 1;
                }
            }
        }
    }
    window.set_active_mods_count(active);
    window.set_total_mods_count(total);
}

fn update_mods_ui(
    window: &AppWindow,
    game_dir: &Path,
    mods_dir: &Path,
    config: &Arc<Mutex<LauncherConfig>>,
    catalog_state: &Arc<Mutex<CatalogState>>,
) {
    let enabled_shaders = { config.lock().unwrap().enabled_shaders.clone() };
    let mut state = catalog_state.lock().unwrap();
    let mods = scan_mods(game_dir, mods_dir, &enabled_shaders, &mut state);
    drop(state);
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
    let _ = fs::create_dir_all(game_dir.join("shaderpacks"));
    let _ = fs::create_dir_all(game_dir.join("resourcepacks"));
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

fn decode_icon_bytes(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img.resize_to_fill(64, 64, image::imageops::FilterType::Triangle);
    let rgba = resized.to_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

fn extract_zip_pack_icon(zip_path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    use std::io::Read;
    let file = fs::File::open(zip_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    // Look for top-level pack.png (or icon.png) inside the resourcepack archive
    for candidate in ["pack.png", "icon.png"] {
        if let Ok(mut entry) = archive.by_name(candidate) {
            if entry.size() <= 2 * 1024 * 1024 {
                let mut buf = Vec::new();
                if entry.read_to_end(&mut buf).is_ok() {
                    if let Some(rgba) = decode_icon_bytes(&buf) {
                        return Some(rgba);
                    }
                }
            }
        }
    }
    None
}

fn ensure_icon_cache_index(state: &mut CatalogState) {
    if state.icon_cache_index.is_some() {
        return;
    }
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(&state.icon_cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if let Some(stem_part) = lower.strip_suffix(".img") {
                // Support both `<safe_stem>_<8hex>.img` and `<safe_stem>.img`
                let key = if let Some((prefix, hash)) = stem_part.rsplit_once('_') {
                    if hash.len() == 8 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                        prefix.to_string()
                    } else {
                        stem_part.to_string()
                    }
                } else {
                    stem_part.to_string()
                };
                map.entry(key).or_insert_with(|| entry.path());
            }
        }
    }
    state.icon_cache_index = Some(map);
}

fn load_cached_icon_by_stem(state: &mut CatalogState, stem: &str) -> Option<(u32, u32, Vec<u8>)> {
    ensure_icon_cache_index(state);
    let safe_stem: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase();
    let path = state.icon_cache_index.as_ref()?.get(&safe_stem)?.clone();
    let bytes = fs::read(path).ok()?;
    decode_icon_bytes(&bytes)
}

fn resolve_installed_icon_and_name(
    state: &mut CatalogState,
    full_path: &Path,
    base_name: &str,
) -> (bool, Image, Option<String>) {
    // 1. Match against live online catalog (gets both official icon & pretty display name)
    let matched = state
        .mods
        .iter()
        .find(|m| m.file_name.eq_ignore_ascii_case(base_name))
        .map(|m| (m.id.clone(), m.name.clone()));

    if let Some((ref mod_id, ref pretty_name)) = matched {
        if let Some((w, h, bytes)) = state.icons.get(mod_id) {
            let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(bytes, *w, *h);
            return (true, Image::from_rgba8(buf), Some(pretty_name.clone()));
        }
    }

    let pretty_opt = matched.map(|(_, n)| n);

    // 2. Fallback: cached local extraction (`pack.png` inside `.zip` OR cached disk icon in `cache/mod-icons`)
    let key = base_name.to_lowercase();
    if !state.local_icons.contains_key(&key) {
        let ext = full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let extracted = if ext == "zip" {
            extract_zip_pack_icon(full_path)
        } else {
            None
        }
        .or_else(|| {
            let stem = Path::new(base_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if stem.is_empty() {
                None
            } else {
                load_cached_icon_by_stem(state, stem)
            }
        });
        state.local_icons.insert(key.clone(), extracted);
    }

    if let Some(Some((w, h, bytes))) = state.local_icons.get(&key) {
        let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(bytes, *w, *h);
        return (true, Image::from_rgba8(buf), pretty_opt);
    }

    (false, Image::default(), pretty_opt)
}

fn scan_mods(
    game_dir: &Path,
    mods_dir: &Path,
    enabled_shaders: &[String],
    catalog_state: &mut CatalogState,
) -> Vec<ModInfo> {
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
            if file_name.starts_with('.') || file_name.starts_with("_verify_") {
                continue;
            }

            let (enabled, base_name) = if let Some(stripped) = file_name.strip_suffix(".disabled") {
                (false, stripped.to_string())
            } else if is_hmod_file(&file_name) {
                (engine_mods::is_hmod_enabled(game_dir, &p), file_name.clone())
            } else {
                (true, file_name.clone())
            };

            let ext = Path::new(&base_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "hmod" || ext == "hnmod" || ext == "jar" || ext == "zip" {
                let raw_len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let size_kb = if raw_len == 0 { 0 } else { raw_len.div_ceil(1024) as i32 };
                let (has_icon, icon, pretty) =
                    resolve_installed_icon_and_name(catalog_state, &p, &base_name);
                mods.push(ModInfo {
                    name: pretty.unwrap_or(base_name).into(),
                    file_name: file_name.into(),
                    size_kb,
                    enabled,
                    is_shader: false,
                    is_resourcepack: false,
                    has_icon,
                    icon,
                });
            }
        }
    }

    // Scan Horizon native shader packs (.hshader) in <game_dir>/shaderpacks/
    let shader_dir = game_dir.join("shaderpacks");
    if let Ok(entries) = fs::read_dir(&shader_dir) {
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
            if file_name.starts_with('.') || file_name.starts_with("_verify_") {
                continue;
            }

            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "hshader" {
                let raw_len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let size_kb = if raw_len == 0 { 0 } else { raw_len.div_ceil(1024) as i32 };
                let enabled = enabled_shaders
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&file_name));
                let (has_icon, icon, pretty) =
                    resolve_installed_icon_and_name(catalog_state, &p, &file_name);
                mods.push(ModInfo {
                    name: pretty.unwrap_or_else(|| file_name.clone()).into(),
                    file_name: file_name.into(),
                    size_kb,
                    enabled,
                    is_shader: true,
                    is_resourcepack: false,
                    has_icon,
                    icon,
                });
            }
        }
    }

    // Scan client-side PvP / QoL resourcepacks (.zip) in <game_dir>/resourcepacks/
    let rp_dir = game_dir.join("resourcepacks");
    let enabled_rps = read_enabled_resource_packs(game_dir);
    if let Ok(entries) = fs::read_dir(&rp_dir) {
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
            if file_name.starts_with('.') || file_name.starts_with("_verify_") {
                continue;
            }

            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "zip" {
                let raw_len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let size_kb = if raw_len == 0 { 0 } else { raw_len.div_ceil(1024) as i32 };
                let rp_key = format!("file/{file_name}");
                let enabled = enabled_rps
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(&rp_key) || item.eq_ignore_ascii_case(&file_name));
                let (has_icon, icon, pretty) =
                    resolve_installed_icon_and_name(catalog_state, &p, &file_name);
                mods.push(ModInfo {
                    name: pretty.unwrap_or_else(|| file_name.clone()).into(),
                    file_name: file_name.into(),
                    size_kb,
                    enabled,
                    is_shader: false,
                    is_resourcepack: true,
                    has_icon,
                    icon,
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
