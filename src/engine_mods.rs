//! Built-in Horizon 26.3 engine PvP & QoL mods (`horizon-extras.txt` + `options.txt`)
//! and `.hmod` enable/disable registry (`horizon-mods.txt`).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct EngineModSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str, // "PvP" | "HUD" | "QoL"
    pub pvp_preset: bool,
    pub storage: EngineStorage,
}

#[derive(Debug, Clone, Copy)]
pub enum EngineStorage {
    /// Writes `<key>:true` / `<key>:false` into `horizon-extras.txt`, plus optional companion keys when enabling.
    ExtrasBool {
        key: &'static str,
        companions_on: &'static [(&'static str, &'static str)],
    },
    /// Writes `<key>:<on_val>` / `<key>:<off_val>` into `options.txt`.
    OptionsVal {
        key: &'static str,
        on_val: &'static str,
        off_val: &'static str,
    },
}

pub const ENGINE_MODS: &[EngineModSpec] = &[
    EngineModSpec {
        id: "keystrokes",
        name: "Keystrokes + CPS",
        description: "Индикатор нажатий WASD, пробела, ЛКМ/ПКМ и счётчик кликов в секунду (CPS).",
        category: "PvP",
        pvp_preset: true,
        storage: EngineStorage::ExtrasBool {
            key: "keystrokes",
            companions_on: &[("keystrokesCps", "true"), ("keystrokesCorner", "top_left")],
        },
    },
    EngineModSpec {
        id: "fps",
        name: "FPS Counter + 1% Lows",
        description: "Компактный счётчик кадров FPS и просадок 1% Lows поверх экрана.",
        category: "HUD",
        pvp_preset: true,
        storage: EngineStorage::ExtrasBool {
            key: "fps",
            companions_on: &[("fpsLows", "true")],
        },
    },
    EngineModSpec {
        id: "zoom",
        name: "Zoom (Приближение)",
        description: "Плавное кинематографичное приближение камеры (как в OptiFine) без подзорной трубы.",
        category: "PvP",
        pvp_preset: true,
        storage: EngineStorage::ExtrasBool {
            key: "zoom",
            companions_on: &[("zoomSmooth", "true")],
        },
    },
    EngineModSpec {
        id: "dynamic_lights",
        name: "Dynamic Lights",
        description: "Факелы, фонари и светящиеся предметы в руке и на земле освещают пещеры в реальном времени.",
        category: "QoL",
        pvp_preset: true,
        storage: EngineStorage::ExtrasBool {
            key: "dynamicLights",
            companions_on: &[],
        },
    },
    EngineModSpec {
        id: "minimap",
        name: "Minimap (Миникарта)",
        description: "Встроенная круглая вращающаяся миникарта местности в углу экрана.",
        category: "HUD",
        pvp_preset: false,
        storage: EngineStorage::ExtrasBool {
            key: "minimap",
            companions_on: &[
                ("minimapShape", "round"),
                ("minimapRotate", "true"),
                ("minimapCorner", "top_right"),
            ],
        },
    },
    EngineModSpec {
        id: "fullbright",
        name: "Fullbright (Движок)",
        description: "Встроенное ночное зрение движка Horizon — светло в любой пещере без шейдеров.",
        category: "PvP",
        pvp_preset: false,
        storage: EngineStorage::ExtrasBool {
            key: "fullbright",
            companions_on: &[],
        },
    },
    EngineModSpec {
        id: "toggle_sprint",
        name: "Toggle Sprint",
        description: "Бег по одному нажатию Ctrl — не нужно удерживать клавишу во время комбо в PvP.",
        category: "PvP",
        pvp_preset: true,
        storage: EngineStorage::OptionsVal {
            key: "toggleSprint",
            on_val: "true",
            off_val: "false",
        },
    },
    EngineModSpec {
        id: "toggle_sneak",
        name: "Toggle Sneak",
        description: "Переключение приседания (Shift) одним нажатием — удобно для бриджинга и стройки.",
        category: "PvP",
        pvp_preset: false,
        storage: EngineStorage::OptionsVal {
            key: "toggleCrouch",
            on_val: "true",
            off_val: "false",
        },
    },
    EngineModSpec {
        id: "no_hurt_cam",
        name: "No Hurt Cam",
        description: "Убирает тряску камеры при получении урона — прицел остаётся идеально стабильным.",
        category: "PvP",
        pvp_preset: true,
        storage: EngineStorage::OptionsVal {
            key: "damageTiltStrength",
            on_val: "0.0",
            off_val: "1.0",
        },
    },
    EngineModSpec {
        id: "static_fov",
        name: "Static PvP FOV",
        description: "Отключает скачки угла обзора от бега и зелий скорости для идеального аима.",
        category: "PvP",
        pvp_preset: true,
        storage: EngineStorage::OptionsVal {
            key: "fovEffectScale",
            on_val: "0.0",
            off_val: "1.0",
        },
    },
    EngineModSpec {
        id: "no_bobbing",
        name: "No View Bobbing",
        description: "Отключает покачивание камеры и руки при ходьбе — чёткий фокус в бою.",
        category: "PvP",
        pvp_preset: true,
        storage: EngineStorage::OptionsVal {
            key: "bobView",
            on_val: "false",
            off_val: "true",
        },
    },
    EngineModSpec {
        id: "mouse_tweaks",
        name: "Mouse Tweaks",
        description: "Перетаскивание и прокрутка колёсиком мыши для мгновенного лута сундуков.",
        category: "QoL",
        pvp_preset: true,
        storage: EngineStorage::ExtrasBool {
            key: "mouseTweaks",
            companions_on: &[("mouseTweaksWheel", "true")],
        },
    },
    EngineModSpec {
        id: "inv_sorter",
        name: "Inventory Sorter",
        description: "Автоматическая сортировка инвентаря и сундуков одной кнопкой.",
        category: "QoL",
        pvp_preset: true,
        storage: EngineStorage::ExtrasBool {
            key: "sort",
            companions_on: &[],
        },
    },
    EngineModSpec {
        id: "auto_reconnect",
        name: "Auto Reconnect",
        description: "Автоматическое переподключение к серверу при обрыве связи или рестарте.",
        category: "QoL",
        pvp_preset: false,
        storage: EngineStorage::ExtrasBool {
            key: "autoReconnect",
            companions_on: &[],
        },
    },
    EngineModSpec {
        id: "own_nametag",
        name: "Show Own Nametag",
        description: "Показывает собственный никнейм над головой в режиме от третьего лица (F5).",
        category: "HUD",
        pvp_preset: false,
        storage: EngineStorage::ExtrasBool {
            key: "ownNameTag",
            companions_on: &[],
        },
    },
    EngineModSpec {
        id: "contrast_outline",
        name: "High-Contrast Block Outline",
        description: "Яркая контрастная обводка выбранного блока — удобно при быстром бриджинге.",
        category: "PvP",
        pvp_preset: false,
        storage: EngineStorage::OptionsVal {
            key: "highContrastBlockOutline",
            on_val: "true",
            off_val: "false",
        },
    },
    EngineModSpec {
        id: "no_distortion",
        name: "No Screen Distortion",
        description: "Отключает искажение экрана от портала Незера и эффекта тошноты.",
        category: "PvP",
        pvp_preset: false,
        storage: EngineStorage::OptionsVal {
            key: "screenEffectScale",
            on_val: "0.0",
            off_val: "1.0",
        },
    },
    EngineModSpec {
        id: "no_lightning",
        name: "No Lightning Flashes",
        description: "Убирает ослепляющие белые вспышки неба во время грозы.",
        category: "QoL",
        pvp_preset: false,
        storage: EngineStorage::OptionsVal {
            key: "hideLightningFlashes",
            on_val: "true",
            off_val: "false",
        },
    },
    EngineModSpec {
        id: "discord_rpc",
        name: "Discord Rich Presence",
        description: "Показывает статус игры Horizon 26.3 в вашем профиле Discord.",
        category: "QoL",
        pvp_preset: false,
        storage: EngineStorage::ExtrasBool {
            key: "discordPresence",
            companions_on: &[],
        },
    },
    EngineModSpec {
        id: "splitscreen",
        name: "Splitscreen Co-op",
        description: "Режим разделённого экрана для локальной игры вдвоём на одном ПК.",
        category: "QoL",
        pvp_preset: false,
        storage: EngineStorage::ExtrasBool {
            key: "splitscreen",
            companions_on: &[],
        },
    },
];

fn parse_kv_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn upsert_kv_file(path: &Path, updates: &[(&str, &str)], default_header: Option<&str>) {
    let mut lines: Vec<String> = if let Ok(content) = fs::read_to_string(path) {
        content.lines().map(|l| l.to_string()).collect()
    } else if let Some(hdr) = default_header {
        hdr.lines().map(|l| l.to_string()).collect()
    } else {
        Vec::new()
    };

    for (key, val) in updates {
        let prefix = format!("{key}:");
        let new_line = format!("{key}:{val}");
        let mut found = false;
        for line in &mut lines {
            if line.trim_start().starts_with(&prefix) {
                *line = new_line.clone();
                found = true;
                break;
            }
        }
        if !found {
            lines.push(new_line);
        }
    }

    let mut out = lines.join("\n");
    out.push('\n');
    let _ = fs::write(path, out);
}

fn is_spec_enabled(
    spec: &EngineModSpec,
    extras: &HashMap<String, String>,
    options: &HashMap<String, String>,
) -> bool {
    match spec.storage {
        EngineStorage::ExtrasBool { key, .. } => extras
            .get(key)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        EngineStorage::OptionsVal { key, on_val, .. } => {
            let Some(cur) = options.get(key) else {
                return false;
            };
            if on_val == "0.0" {
                cur.parse::<f32>().map(|f| f <= 0.01).unwrap_or(false)
            } else {
                cur.eq_ignore_ascii_case(on_val)
            }
        }
    }
}

/// Returns the live enabled/disabled status of all 20 built-in Horizon 26.3 engine mods.
pub fn load_engine_mod_states(game_dir: &Path) -> Vec<(&'static EngineModSpec, bool)> {
    let extras = parse_kv_file(&game_dir.join("horizon-extras.txt"));
    let options = parse_kv_file(&game_dir.join("options.txt"));
    ENGINE_MODS
        .iter()
        .map(|spec| (spec, is_spec_enabled(spec, &extras, &options)))
        .collect()
}

/// Toggles a single built-in engine mod by its ID.
pub fn toggle_engine_mod(game_dir: &Path, mod_id: &str) {
    let Some(spec) = ENGINE_MODS.iter().find(|m| m.id == mod_id) else {
        return;
    };
    let extras_path = game_dir.join("horizon-extras.txt");
    let options_path = game_dir.join("options.txt");
    let extras = parse_kv_file(&extras_path);
    let options = parse_kv_file(&options_path);
    let next_on = !is_spec_enabled(spec, &extras, &options);
    apply_single_spec(game_dir, spec, next_on);
}

fn apply_single_spec(game_dir: &Path, spec: &EngineModSpec, enable: bool) {
    match spec.storage {
        EngineStorage::ExtrasBool { key, companions_on } => {
            let mut updates: Vec<(&str, &str)> = vec![(key, if enable { "true" } else { "false" })];
            if enable {
                updates.extend_from_slice(companions_on);
            }
            upsert_kv_file(&game_dir.join("horizon-extras.txt"), &updates, None);
        }
        EngineStorage::OptionsVal {
            key,
            on_val,
            off_val,
        } => {
            let val = if enable { on_val } else { off_val };
            upsert_kv_file(
                &game_dir.join("options.txt"),
                &[(key, val)],
                Some("version:5023\nlang:ru_ru"),
            );
        }
    }
}

/// One-click competitive PvP preset: enables the core 10 PvP & QoL engine mods cleanly.
pub fn apply_pvp_preset(game_dir: &Path) {
    let mut extras_updates: Vec<(&str, &str)> = Vec::new();
    let mut options_updates: Vec<(&str, &str)> = Vec::new();
    for spec in ENGINE_MODS {
        if !spec.pvp_preset {
            continue;
        }
        match spec.storage {
            EngineStorage::ExtrasBool { key, companions_on } => {
                extras_updates.push((key, "true"));
                extras_updates.extend_from_slice(companions_on);
            }
            EngineStorage::OptionsVal { key, on_val, .. } => {
                options_updates.push((key, on_val));
            }
        }
    }
    if !extras_updates.is_empty() {
        upsert_kv_file(&game_dir.join("horizon-extras.txt"), &extras_updates, None);
    }
    if !options_updates.is_empty() {
        upsert_kv_file(
            &game_dir.join("options.txt"),
            &options_updates,
            Some("version:5023\nlang:ru_ru"),
        );
    }
}

// =========================================================================
// Horizon 26.3 `.hmod` registry (`<game_dir>/horizon-mods.txt`)
// =========================================================================

/// Reads `id = "..."` from `horizon-mod.toml` inside a `.hmod` archive.
pub fn read_hmod_manifest_id(hmod_path: &Path) -> Option<String> {
    let file = fs::File::open(hmod_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("horizon-mod.toml").ok()?;
    let mut toml_str = String::new();
    entry.read_to_string(&mut toml_str).ok()?;
    let value: toml::Value = toml::from_str(&toml_str).ok()?;
    value
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Checks if a `.hmod` is enabled in `horizon-mods.txt` (Horizon defaults to enabled if not explicitly `false`).
pub fn is_hmod_enabled(game_dir: &Path, hmod_path: &Path) -> bool {
    let Some(mod_id) = read_hmod_manifest_id(hmod_path) else {
        return true;
    };
    let map = parse_kv_file(&game_dir.join("horizon-mods.txt"));
    let key = format!("mod.{mod_id}");
    map.get(&key)
        .map(|v| !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

/// Explicitly writes `mod.<id>:true` or `mod.<id>:false` into `<game_dir>/horizon-mods.txt`.
pub fn set_hmod_enabled(game_dir: &Path, hmod_path: &Path, enabled: bool) {
    let Some(mod_id) = read_hmod_manifest_id(hmod_path) else {
        return;
    };
    let key = format!("mod.{mod_id}");
    let val = if enabled { "true" } else { "false" };
    upsert_kv_file(&game_dir.join("horizon-mods.txt"), &[(&key, val)], None);
}

/// Removes `mod.<id>` line from `<game_dir>/horizon-mods.txt` when deleting a `.hmod`.
pub fn remove_hmod_entry(game_dir: &Path, hmod_path: &Path) {
    let Some(mod_id) = read_hmod_manifest_id(hmod_path) else {
        return;
    };
    let path = game_dir.join("horizon-mods.txt");
    let Ok(content) = fs::read_to_string(&path) else {
        return;
    };
    let prefix = format!("mod.{mod_id}:");
    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim_start().starts_with(&prefix))
        .collect();
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    let _ = fs::write(&path, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_mods_toggle_and_pvp_preset() {
        let dir = std::env::temp_dir().join("horizon_test_engine_mods");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        assert_eq!(ENGINE_MODS.len(), 20);
        let init = load_engine_mod_states(&dir);
        assert!(init.iter().all(|(_, on)| !on));

        toggle_engine_mod(&dir, "keystrokes");
        let extras_txt = fs::read_to_string(dir.join("horizon-extras.txt")).unwrap();
        assert!(extras_txt.contains("keystrokes:true"));
        assert!(extras_txt.contains("keystrokesCps:true"));

        apply_pvp_preset(&dir);
        let after = load_engine_mod_states(&dir);
        let on_count = after.iter().filter(|(_, on)| *on).count();
        assert_eq!(on_count, 10, "PvP preset should enable 10 core PvP/QoL engine mods");

        let opts_txt = fs::read_to_string(dir.join("options.txt")).unwrap();
        assert!(opts_txt.contains("toggleSprint:true"));
        assert!(opts_txt.contains("damageTiltStrength:0.0"));
        assert!(opts_txt.contains("fovEffectScale:0.0"));
        assert!(opts_txt.contains("bobView:false"));

        let _ = fs::remove_dir_all(&dir);
    }
}
