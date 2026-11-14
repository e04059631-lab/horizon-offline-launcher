use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub username: String,
    pub ram_gb: u32,
    pub render_distance: u32,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub creative_mode: bool,
    pub custom_game_dir: Option<String>,
    pub custom_version_dir: Option<String>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            ram_gb: 4,
            render_distance: 12,
            width: 1280,
            height: 720,
            fullscreen: false,
            creative_mode: false,
            custom_game_dir: None,
            custom_version_dir: None,
        }
    }
}

impl LauncherConfig {
    pub const fn client_exe_name() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "horizon-client.exe"
        }
        #[cfg(not(target_os = "windows"))]
        {
            "horizon-client"
        }
    }

    pub fn appdata_base_dir() -> PathBuf {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("horizon-offline-game");
        let _ = fs::create_dir_all(&base);
        base
    }

    pub fn config_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            Self::appdata_base_dir()
        }
        #[cfg(not(target_os = "windows"))]
        {
            let base = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("horizon-offline-game");
            let _ = fs::create_dir_all(&base);
            base
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("launcher_config.json")
    }

    pub fn standard_version_dir() -> PathBuf {
        Self::appdata_base_dir().join("client").join("26.3")
    }

    pub fn standard_version_tmp_dir() -> PathBuf {
        Self::appdata_base_dir().join("client").join("26.3.tmp")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut config) = serde_json::from_str::<LauncherConfig>(&content) {
                    if config.username == "Player" {
                        config.username = String::new();
                    }
                    return config;
                }
            }
        }
        let default_config = Self::default();
        default_config.save();
        default_config
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    /// Resolves the working game directory (saves, options, screenshots, mods)
    pub fn get_game_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.custom_game_dir {
            let p = PathBuf::from(dir);
            if !dir.trim().is_empty() {
                let _ = fs::create_dir_all(&p);
                return p;
            }
        }
        Self::appdata_base_dir()
    }

    /// Finds the installed or portable Horizon 26.3 version
    pub fn find_version_dir(&self) -> Option<PathBuf> {
        let exe_name = Self::client_exe_name();

        // 1. Check custom path if set
        if let Some(ref custom) = self.custom_version_dir {
            let p = PathBuf::from(custom);
            if p.join(exe_name).exists() {
                return Some(p);
            }
        }

        // 2. Check standard unpack location: %APPDATA%\horizon-offline-game\client\26.3 (Win) or ~/.local/share/... (Linux)
        let standard = Self::standard_version_dir();
        if standard.join(exe_name).exists() {
            return Some(standard);
        }

        // 3. Check portable './game' directory next to launcher executable (dev fallback)
        if let Ok(exe_dir) = std::env::current_exe().map(|p| p.parent().unwrap_or(Path::new(".")).to_path_buf()) {
            let portable_game = exe_dir.join("game");
            if portable_game.join(exe_name).exists() {
                return Some(portable_game);
            }
        }

        // 4. Check AppData Roaming standard Horizon installation (legacy fallback)
        if let Some(roaming) = dirs::data_dir() {
            let versions_dir = roaming.join("horizon-launcher").join("versions");
            if versions_dir.exists() {
                if let Ok(entries) = fs::read_dir(versions_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() && path.join(exe_name).exists() {
                            return Some(path);
                        }
                    }
                }
            }
        }

        None
    }
}
