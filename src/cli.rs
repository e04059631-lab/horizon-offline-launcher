//! Built-in CLI commands for Horizon Offline Launcher (Linux & Windows).
//!
//! When launched with no arguments, opens the GUI normally.
//! Supports `--help`, `--version`, `--install`, `--uninstall`, `--status`,
//! `--unpack`, `--mods`, and `--play [nick] [--server <ip>]`.

use crate::client_unpacker;
use crate::config::LauncherConfig;
use crate::game_process::{GameProcess, GameState};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(unix)]
const ICON_256_PNG: &[u8] = include_bytes!("../assets/icon-256.png");

pub enum CliAction {
    RunGui,
    Exit(i32),
}

pub fn handle_args() -> CliAction {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return CliAction::RunGui;
    }

    let code = match args[0].as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            0
        }
        "-V" | "--version" | "version" => {
            println!("Horizon Offline Launcher v{APP_VERSION} (Minecraft Horizon 26.3)");
            0
        }
        "--install" | "install" => cmd_install(),
        "--uninstall" | "uninstall" => {
            let purge = args.iter().any(|a| a == "--purge");
            cmd_uninstall(purge)
        }
        "--status" | "status" => cmd_status(),
        "--unpack" | "unpack" => {
            let force = args.iter().any(|a| a == "--force" || a == "-f");
            cmd_unpack(force)
        }
        "--mods" | "mods" => cmd_mods(),
        "--play" | "play" => cmd_play(&args[1..]),
        other => {
            eprintln!("Неизвестная команда: {other}");
            eprintln!("Запустите `horizon-launcher --help` для списка доступных команд.");
            2
        }
    };

    CliAction::Exit(code)
}

fn print_help() {
    println!(
        "Horizon Offline Launcher v{APP_VERSION} — автономный оффлайн-лаунчер для Minecraft Horizon 26.3\n\
         \n\
         Использование:\n\
           horizon-launcher                       Открыть графический интерфейс лаунчера\n\
           horizon-launcher --play [ник]          Запустить игру сразу из терминала\n\
                               [--server <ip>]    (опционально подключиться к серверу)\n\
           horizon-launcher --install             Установить в ~/.local/bin + ярлык в меню (Linux)\n\
           horizon-launcher --uninstall [--purge] Удалить лаунчер и ярлык (--purge удалит и миры)\n\
           horizon-launcher --status              Показать статус клиента, папок и настроек\n\
           horizon-launcher --unpack [--force]    Распаковать встроенный клиент Horizon 26.3\n\
           horizon-launcher --mods                Список установленных шейдеров и PvP-паков\n\
           horizon-launcher --version             Показать версию лаунчера\n\
           horizon-launcher --help                Показать эту справку\n\
         \n\
         Каталог шейдеров и PvP-паков: https://github.com/xwwerz/horizon-mods"
    );
}

fn cmd_status() -> i32 {
    let config = LauncherConfig::load();
    let installed = client_unpacker::is_client_installed();
    let version_dir = config.find_version_dir();
    let game_dir = config.get_game_dir();
    let payload = client_unpacker::find_embedded_payload().ok().flatten();

    println!("Horizon Offline Launcher v{APP_VERSION}");
    println!("  Платформа:        {} ({})", std::env::consts::OS, std::env::consts::ARCH);
    println!(
        "  Клиент 26.3:      {}",
        if installed {
            "установлен и готов"
        } else if version_dir.is_some() {
            "найден в альтернативной папке"
        } else {
            "не распакован (запустите --unpack или откройте лаунчер)"
        }
    );
    if let Some(dir) = version_dir {
        println!("  Папка клиента:    {}", dir.display());
    }
    match payload {
        Some((_, len)) => println!("  Встроенный пакет: да ({:.2} МБ)", len as f64 / 1_048_576.0),
        None => println!("  Встроенный пакет: нет (dev-сборка)"),
    }
    println!("  Папка игры:       {}", game_dir.display());
    println!(
        "  Никнейм:          {}",
        if config.username.trim().is_empty() {
            "(не задан)"
        } else {
            config.username.trim()
        }
    );
    println!("  ОЗУ / дальность:  {} ГБ / {} чанков", config.ram_gb, config.render_distance);
    println!("  Каталог модов:    {}", config.mods_repo_url);
    0
}

fn cmd_mods() -> i32 {
    let config = LauncherConfig::load();
    let game_dir = config.get_game_dir();
    let shaders_dir = game_dir.join("horizon-shaders");
    let packs_dir = game_dir.join("resourcepacks");

    println!("Установленные дополнения ({}):", game_dir.display());
    let mut total = 0usize;

    if let Ok(entries) = fs::read_dir(&shaders_dir) {
        let mut list: Vec<(String, bool)> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    return None;
                }
                if name.ends_with(".hshader") {
                    let enabled = config.enabled_shaders.iter().any(|s| s == &name);
                    Some((name, enabled))
                } else if name.ends_with(".hshader.disabled") {
                    Some((name, false))
                } else {
                    None
                }
            })
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        if !list.is_empty() {
            println!("\n  Шейдеры ({}):", list.len());
            for (name, on) in list {
                println!("    [{}] {}", if on { "ВКЛ" } else { "выкл" }, name);
                total += 1;
            }
        }
    }

    if let Ok(entries) = fs::read_dir(&packs_dir) {
        let mut list: Vec<(String, bool)> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".zip") {
                    Some((name, true))
                } else if name.ends_with(".zip.disabled") {
                    Some((name.trim_end_matches(".disabled").to_string(), false))
                } else {
                    None
                }
            })
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        if !list.is_empty() {
            println!("\n  PvP / QoL паки ({}):", list.len());
            for (name, on) in list {
                println!("    [{}] {}", if on { "ВКЛ" } else { "выкл" }, name);
                total += 1;
            }
        }
    }

    if total == 0 {
        println!("  Пока ничего не установлено. Откройте вкладку «Моды» в лаунчере (56 дополнений в каталоге).");
    }
    0
}

fn cmd_unpack(force: bool) -> i32 {
    if !force && client_unpacker::is_client_installed() {
        println!(
            "Клиент Horizon 26.3 уже установлен в {}. Используйте --unpack --force для переустановки.",
            LauncherConfig::standard_version_dir().display()
        );
        return 0;
    }

    let (offset, len) = match client_unpacker::find_embedded_payload() {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!("Ошибка: встроенный пакет HORIZON_PAYLOAD не найден в этом бинарнике.");
            return 1;
        }
        Err(e) => {
            eprintln!("Ошибка проверки встроенного пакета: {e}");
            return 1;
        }
    };

    println!("Распаковка встроенного клиента Horizon 26.3 ({:.2} МБ)...", len as f64 / 1_048_576.0);
    let abort = Arc::new(AtomicBool::new(false));
    let res = client_unpacker::unpack_embedded_payload(offset, len, abort, |p| {
        print!("\r  {} {} {}        ", p.percent_text, p.status, p.time_left);
        let _ = std::io::stdout().flush();
    });
    println!();

    match res {
        Ok(()) => {
            println!("[OK] Клиент распакован в {}", LauncherConfig::standard_version_dir().display());
            0
        }
        Err(e) => {
            eprintln!("Ошибка распаковки: {e}");
            1
        }
    }
}

fn parse_play_args(args: &[String]) -> (Option<String>, Option<String>) {
    let mut nick: Option<String> = None;
    let mut server: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--server" | "-s" => {
                if let Some(val) = args.get(i + 1) {
                    server = Some(val.clone());
                    i += 2;
                    continue;
                }
            }
            arg if !arg.starts_with('-') && nick.is_none() => {
                nick = Some(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    (nick, server)
}

fn cmd_play(args: &[String]) -> i32 {
    let mut config = LauncherConfig::load();
    let (nick_arg, server_arg) = parse_play_args(args);

    if let Some(n) = nick_arg {
        config.username = n;
        config.save();
    }
    if config.username.trim().is_empty() {
        eprintln!("Укажите никнейм: horizon-launcher --play ТвойНик");
        return 1;
    }

    if config.find_version_dir().is_none() {
        let code = cmd_unpack(false);
        if code != 0 {
            return code;
        }
    }

    let server = server_arg.as_deref().or_else(|| {
        if config.last_server.trim().is_empty() {
            None
        } else {
            Some(config.last_server.trim())
        }
    });

    let proc = GameProcess::new();
    if let Err(e) = proc.launch(&config, server) {
        eprintln!("Ошибка запуска: {e}");
        return 1;
    }

    loop {
        for line in proc.drain_logs() {
            println!("{line}");
        }
        match proc.current_state() {
            GameState::Exited { code } => {
                for line in proc.drain_logs() {
                    println!("{line}");
                }
                return code;
            }
            GameState::Failed(err) => {
                eprintln!("Сбой процесса: {err}");
                return 1;
            }
            GameState::NotRunning => return 0,
            _ => std::thread::sleep(Duration::from_millis(120)),
        }
    }
}

#[cfg(unix)]
fn cmd_install() -> i32 {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let Some(home) = dirs::home_dir() else {
        eprintln!("Не удалось определить домашнюю папку (HOME).");
        return 1;
    };

    let bin_dir = home.join(".local").join("bin");
    let target_bin = bin_dir.join("horizon-launcher");
    let icon_dir = home
        .join(".local")
        .join("share")
        .join("icons")
        .join("hicolor")
        .join("256x256")
        .join("apps");
    let icon_path = icon_dir.join("horizon-launcher.png");
    let apps_dir = home.join(".local").join("share").join("applications");
    let desktop_path = apps_dir.join("horizon-launcher.desktop");

    for dir in [&bin_dir, &icon_dir, &apps_dir] {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("Не удалось создать папку {}: {e}", dir.display());
            return 1;
        }
    }

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Не удалось определить путь к текущему файлу: {e}");
            return 1;
        }
    };

    let same_file = fs::canonicalize(&current_exe).ok() == fs::canonicalize(&target_bin).ok();
    if !same_file {
        let tmp_bin = target_bin.with_extension("tmp");
        if let Err(e) = fs::copy(&current_exe, &tmp_bin) {
            eprintln!("Не удалось скопировать бинарник в {}: {e}", tmp_bin.display());
            return 1;
        }
        let _ = fs::set_permissions(&tmp_bin, fs::Permissions::from_mode(0o755));
        if let Err(e) = fs::rename(&tmp_bin, &target_bin) {
            let _ = fs::remove_file(&tmp_bin);
            eprintln!("Не удалось установить {}: {e}", target_bin.display());
            return 1;
        }
    } else {
        let _ = fs::set_permissions(&target_bin, fs::Permissions::from_mode(0o755));
    }

    if let Err(e) = fs::write(&icon_path, ICON_256_PNG) {
        eprintln!("Предупреждение: не удалось сохранить иконку {}: {e}", icon_path.display());
    }

    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Name=Horizon Offline Launcher\n\
         Comment=Автономный оффлайн-лаунчер для Minecraft Horizon 26.3\n\
         Exec={}\n\
         Icon=horizon-launcher\n\
         Terminal=false\n\
         Type=Application\n\
         Categories=Game;\n\
         Keywords=minecraft;horizon;launcher;game;\n\
         StartupWMClass=horizon-offline-launcher\n",
        target_bin.display()
    );
    if let Err(e) = fs::write(&desktop_path, desktop_entry) {
        eprintln!("Не удалось создать ярлык {}: {e}", desktop_path.display());
        return 1;
    }

    let _ = Command::new("update-desktop-database").arg(&apps_dir).status();
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(home.join(".local/share/icons/hicolor"))
        .status();

    println!("[OK] Установлено:");
    println!("  Команда: {}", target_bin.display());
    println!("  Ярлык:   {}", desktop_path.display());
    println!("  Иконка:  {}", icon_path.display());
    0
}

#[cfg(not(unix))]
fn cmd_install() -> i32 {
    println!("Команда --install предназначена для Linux (~/.local/bin и ярлык .desktop).");
    println!("На Windows просто запускайте horizon-offline-launcher.exe напрямую.");
    0
}

#[cfg(unix)]
fn cmd_uninstall(purge: bool) -> i32 {
    let Some(home) = dirs::home_dir() else {
        eprintln!("Не удалось определить HOME.");
        return 1;
    };

    let targets: [PathBuf; 3] = [
        home.join(".local/bin/horizon-launcher"),
        home.join(".local/share/applications/horizon-launcher.desktop"),
        home.join(".local/share/icons/hicolor/256x256/apps/horizon-launcher.png"),
    ];
    for p in &targets {
        if p.exists() {
            let _ = fs::remove_file(p);
        }
    }

    if purge {
        let data_dir = LauncherConfig::appdata_base_dir();
        let cfg_dir = LauncherConfig::config_dir();
        let _ = fs::remove_dir_all(&data_dir);
        let _ = fs::remove_dir_all(&cfg_dir);
        println!("[OK] Horizon Offline Launcher и все данные игры полностью удалены.");
    } else {
        println!("[OK] Horizon Offline Launcher и ярлык удалены.");
        println!(
            "  Миры и настройки сохранены в {} (используйте --uninstall --purge для полного удаления).",
            LauncherConfig::appdata_base_dir().display()
        );
    }
    0
}

#[cfg(not(unix))]
fn cmd_uninstall(_purge: bool) -> i32 {
    println!("Команда --uninstall предназначена для Linux.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_play_args() {
        let args = vec![
            "wwerz".to_string(),
            "--server".to_string(),
            "play.example.net".to_string(),
        ];
        let (nick, srv) = parse_play_args(&args);
        assert_eq!(nick.as_deref(), Some("wwerz"));
        assert_eq!(srv.as_deref(), Some("play.example.net"));

        let args2 = vec!["-s".to_string(), "127.0.0.1".to_string(), "Steve".to_string()];
        let (nick2, srv2) = parse_play_args(&args2);
        assert_eq!(nick2.as_deref(), Some("Steve"));
        assert_eq!(srv2.as_deref(), Some("127.0.0.1"));
    }
}
