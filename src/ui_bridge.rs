use crate::client_unpacker;
use crate::config::LauncherConfig;
use crate::game_process::GameProcess;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// Include compiled slint UI modules
slint::include_modules!();

pub fn setup_ui_bridge(window: &AppWindow) {
    let config = Arc::new(Mutex::new(LauncherConfig::load()));
    let game = Arc::new(GameProcess::new());

    let (game_dir, mods_dir, saves_dir, screenshots_dir) = {
        let cfg = config.lock().unwrap();
        ensure_game_dirs(&cfg)
    };

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
        window.set_is_playing(false);
        window.set_mods_dir_path(mods_dir.display().to_string().into());
    }

    // 2. Scan Mods
    let initial_mods = scan_mods(&mods_dir);
    let mods_model = Rc::new(VecModel::from(initial_mods));
    window.set_mods(ModelRc::from(mods_model.clone()));

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
            if let Some(w) = window_weak.upgrade() {
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
            }
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
                match game_clone.launch(&cfg, None) {
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
        window.on_open_mods_folder(move || { open_folder(&p); });
    }
    {
        let p = saves_dir.clone();
        window.on_open_saves_folder(move || { open_folder(&p); });
    }
    {
        let p = screenshots_dir.clone();
        window.on_open_screenshots_folder(move || { open_folder(&p); });
    }
    {
        let p = game_dir.clone();
        window.on_open_game_folder(move || { open_folder(&p); });
    }

    // 6. Callbacks: Settings & Nickname Save
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
                cfg.save();
            }
        });
    }

    {
        let config_clone = Arc::clone(&config);
        window.on_set_nickname(move |name| {
            let mut cfg = config_clone.lock().unwrap();
            cfg.username = name.to_string();
            cfg.save();
        });
    }

    // 7. Callback: Clear Log
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

    // 8. Background Polling Thread (Logs & Game Process State)
    {
        let window_weak = window.as_weak();
        let game_clone = Arc::clone(&game);
        let log_entries_clone = Arc::clone(&log_entries);

        thread::spawn(move || {
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
                    }
                    let entries_clone = Arc::clone(&log_entries_clone);
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        let current_entries = entries_clone.lock().unwrap().clone();
                        w.set_log_lines(ModelRc::new(VecModel::from(current_entries)));
                        w.set_is_playing(is_running);
                    });
                } else {
                    let _ = window_weak.upgrade_in_event_loop(move |w| {
                        if w.get_is_playing() != is_running {
                            w.set_is_playing(is_running);
                        }
                    });
                }
            }
        });
    }

    // 9. Client Unpacking Flow
    setup_client_unpacking(window);
}

fn setup_client_unpacking(window: &AppWindow) {
    if client_unpacker::is_client_installed() {
        window.set_is_unpacking(false);
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
            if let Some(ext) = p.extension() {
                if ext == "hmod" || ext == "hnmod" || ext == "jar" || ext == "zip" {
                    let size_kb = entry.metadata().map(|m| m.len() / 1024).unwrap_or(0) as i32;
                    let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                    mods.push(ModInfo {
                        name: name.into(),
                        size_kb,
                    });
                }
            }
        }
    }
    mods
}

fn make_log_entry(line: &str) -> LogEntry {
    let lower = line.to_lowercase();
    let color = if lower.contains("err") || lower.contains("ошибка") {
        slint::Color::from_argb_u8(255, 255, 180, 171) // Theme.error
    } else if lower.contains("warn") {
        slint::Color::from_argb_u8(255, 222, 188, 223) // Theme.tertiary
    } else if lower.contains("vulkan") || lower.contains("horizon") {
        slint::Color::from_argb_u8(255, 173, 198, 255) // Theme.primary
    } else {
        slint::Color::from_argb_u8(255, 196, 198, 208) // Theme.on-surface-variant
    };
    LogEntry {
        text: line.into(),
        color,
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
