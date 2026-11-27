//! Horizon 26.3 `.hmod` enable/disable registry (`<game_dir>/horizon-mods.txt`).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

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

fn upsert_kv_file(path: &Path, updates: &[(&str, &str)]) {
    let mut lines: Vec<String> = if let Ok(content) = fs::read_to_string(path) {
        content.lines().map(|l| l.to_string()).collect()
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
    upsert_kv_file(&game_dir.join("horizon-mods.txt"), &[(&key, val)]);
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
    use std::io::Write;

    #[test]
    fn test_hmod_registry_toggle_and_remove() {
        let dir = std::env::temp_dir().join("horizon_test_hmod_reg");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let hmod_path = dir.join("sample.hmod");
        {
            let f = fs::File::create(&hmod_path).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("horizon-mod.toml", opts).unwrap();
            z.write_all(b"id = \"ruby_arsenal\"\nname = \"Ruby Arsenal\"\n").unwrap();
            z.finish().unwrap();
        }

        assert_eq!(read_hmod_manifest_id(&hmod_path).as_deref(), Some("ruby_arsenal"));
        assert!(is_hmod_enabled(&dir, &hmod_path));

        set_hmod_enabled(&dir, &hmod_path, false);
        assert!(!is_hmod_enabled(&dir, &hmod_path));

        set_hmod_enabled(&dir, &hmod_path, true);
        assert!(is_hmod_enabled(&dir, &hmod_path));

        remove_hmod_entry(&dir, &hmod_path);
        let txt = fs::read_to_string(dir.join("horizon-mods.txt")).unwrap();
        assert!(!txt.contains("ruby_arsenal"));

        let _ = fs::remove_dir_all(&dir);
    }
}
