//! Combines multiple enabled Horizon 26.3 WGSL shader packs (.hshader) into a single
//! composite `.horizon-combined.hshader` archive so players can stack multiple shaders
//! simultaneously (e.g. Motion Blur + FPS Sharpness + Water Reflections + Aurora Sky).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const COMBINED_PACK_NAME: &str = ".horizon-combined.hshader";

struct LoadedPack {
    file_name: String,
    prefix: String,
    display_name: String,
    manifest: toml::Value,
    files: HashMap<String, String>,
}

fn read_pack(path: &Path, idx: usize) -> Option<LoadedPack> {
    let file_name = path.file_name()?.to_string_lossy().to_string();
    let file = fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut files = HashMap::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok()?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            let clean = content.strip_prefix('\u{feff}').unwrap_or(&content).to_string();
            files.insert(name, clean);
        }
    }

    let manifest_raw = files.get("horizon-shaders.toml")?;
    let manifest: toml::Value = toml::from_str(manifest_raw).ok()?;
    let display_name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Shader")
        .to_string();

    Some(LoadedPack {
        file_name,
        prefix: format!("p{idx}_"),
        display_name,
        manifest,
        files,
    })
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Replaces whole-word identifier occurrences in WGSL source, skipping line comments
/// and member field accesses after `.` (e.g. `input.color` will not match `color`).
fn replace_wgsl_idents(src: &str, renames: &HashMap<String, String>) -> String {
    if renames.is_empty() {
        return src.to_string();
    }
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(src.len() + 128);
    let mut i = 0;

    while i < len {
        // Skip single-line comments `// ...`
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            let start = i;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            out.push_str(&src[start..i]);
            continue;
        }

        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let prev_is_dot = i > 0 && bytes[i - 1] == b'.';
            let start = i;
            while i < len && is_ident_char(bytes[i]) {
                i += 1;
            }
            let ident = &src[start..i];
            if !prev_is_dot {
                if let Some(new_name) = renames.get(ident) {
                    out.push_str(new_name);
                    continue;
                }
            }
            out.push_str(ident);
            continue;
        }

        // Advance UTF-8 safely
        let ch = src[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

/// Collects top-level `fn`, `override`, `const`, and `struct` identifier declarations in a WGSL file.
fn collect_wgsl_decls(src: &str) -> Vec<String> {
    let mut decls = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for kw in ["fn ", "override ", "const ", "struct ", "alias ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(kw) {
                let ident: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !ident.is_empty() && !decls.contains(&ident) {
                    decls.push(ident);
                }
            }
        }
    }
    decls
}

/// Namespaces all declarations in a WGSL stage file with `prefix`.
/// If `keep_entry` is Some("post"), keeps `fn post` unrenamed while prefixing helpers/overrides.
fn namespace_stage_wgsl(
    src: &str,
    prefix: &str,
    option_ids: &HashSet<String>,
    keep_entry: Option<&str>,
) -> String {
    let mut renames = HashMap::new();
    for decl in collect_wgsl_decls(src) {
        if keep_entry == Some(decl.as_str()) {
            continue;
        }
        renames.insert(decl.clone(), format!("{prefix}{decl}"));
    }
    for opt_id in option_ids {
        renames.insert(opt_id.clone(), format!("{prefix}{opt_id}"));
    }
    replace_wgsl_idents(src, &renames)
}

/// Synchronizes `pickedShaderPack:` in `horizon-graphics.txt` for any number of enabled `.hshader` packs.
/// - 0 enabled: clears `pickedShaderPack:` and removes `.horizon-combined.hshader`.
/// - 1 enabled: sets `pickedShaderPack:<single_file>` directly.
/// - 2+ enabled: merges all enabled packs into `.horizon-combined.hshader` and activates it.
pub fn sync_combined_shaders(game_dir: &Path, enabled_files: &[String]) -> String {
    let shader_dir = game_dir.join("shaderpacks");
    let combined_path = shader_dir.join(COMBINED_PACK_NAME);

    // Filter to existing real .hshader files in shaderpacks/
    let valid_files: Vec<String> = enabled_files
        .iter()
        .filter(|name| {
            !name.starts_with('.')
                && !name.starts_with("_verify_")
                && shader_dir.join(name).is_file()
        })
        .cloned()
        .collect();

    if valid_files.is_empty() {
        let _ = fs::remove_file(&combined_path);
        return String::new();
    }

    if valid_files.len() == 1 {
        let _ = fs::remove_file(&combined_path);
        return valid_files[0].clone();
    }

    let mut packs = Vec::new();
    for (idx, fname) in valid_files.iter().enumerate() {
        if let Some(p) = read_pack(&shader_dir.join(fname), idx) {
            packs.push(p);
        }
    }

    if packs.is_empty() {
        let _ = fs::remove_file(&combined_path);
        return String::new();
    }
    if packs.len() == 1 {
        let _ = fs::remove_file(&combined_path);
        return packs[0].file_name.clone();
    }

    match build_combined_archive(&combined_path, &packs) {
        Ok(()) => COMBINED_PACK_NAME.to_string(),
        Err(_) => {
            // Safe fallback to the most recently added pack if anything unexpected occurs
            let _ = fs::remove_file(&combined_path);
            valid_files.last().cloned().unwrap_or_default()
        }
    }
}

fn build_combined_archive(out_path: &Path, packs: &[LoadedPack]) -> Result<(), String> {
    let mut shadow_res: Option<i64> = None;
    let mut shadow_dist: Option<f64> = None;
    let mut needs_reflection = false;
    let mut disable_fog = false;

    let mut merged_lights: Vec<toml::Value> = Vec::new();
    let mut merged_options: Vec<toml::Value> = Vec::new();

    let mut sky_chunks: Vec<(String, String)> = Vec::new();
    let mut mat_chunks: Vec<(String, String)> = Vec::new();
    let mut weather_chunks: Vec<(String, String)> = Vec::new();
    let mut post_files: Vec<(String, String)> = Vec::new();

    for pack in packs {
        let m = &pack.manifest;

        // Collect option IDs for this pack
        let mut opt_ids = HashSet::new();
        if let Some(opts) = m.get("options").and_then(|v| v.as_array()) {
            for opt in opts {
                if let Some(id) = opt.get("id").and_then(|v| v.as_str()) {
                    opt_ids.insert(id.to_string());
                }
            }
        }

        // Merge [shadow]
        if let Some(sh) = m.get("shadow").and_then(|v| v.as_table()) {
            if let Some(r) = sh.get("resolution").and_then(|v| v.as_integer()) {
                shadow_res = Some(shadow_res.map_or(r, |cur| cur.max(r)));
            }
            if let Some(d) = sh.get("distance").and_then(|v| v.as_float()) {
                shadow_dist = Some(shadow_dist.map_or(d, |cur| cur.max(d)));
            } else if let Some(d) = sh.get("distance").and_then(|v| v.as_integer()) {
                let df = d as f64;
                shadow_dist = Some(shadow_dist.map_or(df, |cur| cur.max(df)));
            }
        }

        // Merge [reflection]
        if m.get("reflection").is_some() {
            needs_reflection = true;
        }

        // Merge [[lights]] (cap at 32)
        if let Some(lights) = m.get("lights").and_then(|v| v.as_array()) {
            for l in lights {
                if merged_lights.len() < 32 {
                    merged_lights.push(l.clone());
                }
            }
        }

        // Track which option IDs actually appear in emitted stages
        let mut used_wgsl_concat = String::new();

        // [sky]
        if let Some(sky_file) = m
            .get("sky")
            .and_then(|v| v.get("file"))
            .and_then(|v| v.as_str())
        {
            if let Some(src) = pack.files.get(sky_file) {
                let ns = namespace_stage_wgsl(src, &pack.prefix, &opt_ids, None);
                used_wgsl_concat.push_str(&ns);
                sky_chunks.push((format!("{}sky", pack.prefix), ns));
            }
        }

        // [material]
        if let Some(mat_tbl) = m.get("material").and_then(|v| v.as_table()) {
            if mat_tbl.get("fog").and_then(|v| v.as_bool()) == Some(false) {
                disable_fog = true;
            }
            if let Some(mat_file) = mat_tbl.get("file").and_then(|v| v.as_str()) {
                if let Some(src) = pack.files.get(mat_file) {
                    let ns = namespace_stage_wgsl(src, &pack.prefix, &opt_ids, None);
                    used_wgsl_concat.push_str(&ns);
                    mat_chunks.push((format!("{}material", pack.prefix), ns));
                }
            }
        }

        // [weather]
        if let Some(w_file) = m
            .get("weather")
            .and_then(|v| v.get("file"))
            .and_then(|v| v.as_str())
        {
            if let Some(src) = pack.files.get(w_file) {
                let ns = namespace_stage_wgsl(src, &pack.prefix, &opt_ids, None);
                used_wgsl_concat.push_str(&ns);
                weather_chunks.push((format!("{}weather", pack.prefix), ns));
            }
        }

        // [[post]] — each post stage runs sequentially as its own native [[post]] pass!
        if let Some(posts) = m.get("post").and_then(|v| v.as_array()) {
            for post_tbl in posts {
                if post_files.len() >= 12 {
                    break;
                }
                if let Some(p_file) = post_tbl.get("file").and_then(|v| v.as_str()) {
                    if let Some(src) = pack.files.get(p_file) {
                        let ns = namespace_stage_wgsl(src, &pack.prefix, &opt_ids, Some("post"));
                        used_wgsl_concat.push_str(&ns);
                        let pass_name = format!("post_{}.wgsl", post_files.len());
                        post_files.push((pass_name, ns));
                    }
                }
            }
        }

        // Namespace [[options]] for this pack (only include options declared in emitted WGSL)
        if let Some(opts) = m.get("options").and_then(|v| v.as_array()) {
            for opt in opts {
                if let Some(mut tbl) = opt.as_table().cloned() {
                    if let Some(orig_id) = tbl
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                    {
                        let new_id = format!("{}{orig_id}", pack.prefix);
                        if used_wgsl_concat.contains(&format!("override {new_id}")) {
                            let orig_name = tbl
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&orig_id)
                                .to_string();
                            tbl.insert("id".to_string(), toml::Value::String(new_id));
                            tbl.insert(
                                "name".to_string(),
                                toml::Value::String(format!("{}: {orig_name}", pack.display_name)),
                            );
                            merged_options.push(toml::Value::Table(tbl));
                        }
                    }
                }
            }
        }
    }

    // Build combined TOML manifest
    let mut root = toml::map::Map::new();
    root.insert("schema".into(), toml::Value::Integer(1));
    root.insert("api".into(), toml::Value::String("0.1".into()));
    root.insert("id".into(), toml::Value::String("horizon_combined".into()));
    root.insert(
        "name".into(),
        toml::Value::String(format!("Combined ({} Shaders)", packs.len())),
    );
    root.insert("version".into(), toml::Value::String("1.0.0".into()));

    if !merged_options.is_empty() {
        root.insert("options".into(), toml::Value::Array(merged_options));
    }
    if let (Some(res), Some(dist)) = (shadow_res, shadow_dist) {
        let mut sh = toml::map::Map::new();
        sh.insert("resolution".into(), toml::Value::Integer(res));
        sh.insert("distance".into(), toml::Value::Float(dist));
        root.insert("shadow".into(), toml::Value::Table(sh));
    }
    if needs_reflection && !mat_chunks.is_empty() {
        root.insert("reflection".into(), toml::Value::Table(toml::map::Map::new()));
    }
    if !merged_lights.is_empty() && !mat_chunks.is_empty() {
        root.insert("lights".into(), toml::Value::Array(merged_lights));
    }

    let mut out_files: Vec<(String, String)> = Vec::new();

    // Compose sky.wgsl
    if !sky_chunks.is_empty() {
        let mut tbl = toml::map::Map::new();
        tbl.insert("file".into(), toml::Value::String("sky.wgsl".into()));
        root.insert("sky".into(), toml::Value::Table(tbl));

        let mut code = String::new();
        for (_, chunk) in &sky_chunks {
            code.push_str(chunk);
            code.push('\n');
        }
        code.push_str("fn sky(input: SkyInput) -> vec4<f32> {\n");
        code.push_str("    var cur = input.color;\n");
        for (fn_name, _) in &sky_chunks {
            code.push_str(&format!("    cur = {fn_name}(SkyInput(input.direction, cur));\n"));
        }
        code.push_str("    return cur;\n}\n");
        out_files.push(("sky.wgsl".into(), code));
    }

    // Compose material.wgsl
    if !mat_chunks.is_empty() {
        let mut tbl = toml::map::Map::new();
        tbl.insert("file".into(), toml::Value::String("material.wgsl".into()));
        if disable_fog {
            tbl.insert("fog".into(), toml::Value::Boolean(false));
        }
        root.insert("material".into(), toml::Value::Table(tbl));

        let mut code = String::new();
        for (_, chunk) in &mat_chunks {
            code.push_str(chunk);
            code.push('\n');
        }
        code.push_str("fn material(input: MaterialInput) -> Surface {\n");
        code.push_str("    let h_base_tex = max(input.albedo.rgb * input.tint.rgb, vec3<f32>(0.004));\n");
        code.push_str("    var cur_in = input;\n");
        code.push_str(&format!("    var cur = {}(cur_in);\n", mat_chunks[0].0));
        for (fn_name, _) in mat_chunks.iter().skip(1) {
            code.push_str("    cur_in.lightmap = clamp(cur.color.rgb / h_base_tex, vec3<f32>(0.0), vec3<f32>(4.0));\n");
            code.push_str(&format!("    cur = {fn_name}(cur_in);\n"));
        }
        code.push_str("    return cur;\n}\n");
        out_files.push(("material.wgsl".into(), code));
    }

    // Compose weather.wgsl
    if !weather_chunks.is_empty() {
        let mut tbl = toml::map::Map::new();
        tbl.insert("file".into(), toml::Value::String("weather.wgsl".into()));
        root.insert("weather".into(), toml::Value::Table(tbl));

        let mut code = String::new();
        for (_, chunk) in &weather_chunks {
            code.push_str(chunk);
            code.push('\n');
        }
        code.push_str("fn weather(input: WeatherInput) -> vec4<f32> {\n");
        code.push_str("    var cur_in = input;\n");
        for (fn_name, _) in &weather_chunks {
            code.push_str(&format!("    cur_in.color = {fn_name}(cur_in);\n"));
        }
        code.push_str("    return cur_in.color;\n}\n");
        out_files.push(("weather.wgsl".into(), code));
    }

    // Add sequential [[post]] passes
    if !post_files.is_empty() {
        let mut post_arr = Vec::new();
        for (fname, code) in post_files {
            let mut tbl = toml::map::Map::new();
            tbl.insert("file".into(), toml::Value::String(fname.clone()));
            post_arr.push(toml::Value::Table(tbl));
            out_files.push((fname, code));
        }
        root.insert("post".into(), toml::Value::Array(post_arr));
    }

    let manifest_toml =
        toml::to_string_pretty(&toml::Value::Table(root)).map_err(|e| e.to_string())?;

    // Build zip archive in memory first so we skip touching disk when content is unchanged
    let mut zip_bytes = Vec::with_capacity(4096);
    {
        let cursor = std::io::Cursor::new(&mut zip_bytes);
        let mut zip = ZipWriter::new(cursor);
        // Fixed timestamp so identical inputs produce byte-for-byte identical zips
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default());

        zip.start_file("horizon-shaders.toml", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(manifest_toml.as_bytes())
            .map_err(|e| e.to_string())?;

        for (fname, content) in out_files {
            zip.start_file(&fname, opts).map_err(|e| e.to_string())?;
            zip.write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }

    if let Ok(existing) = fs::read(out_path) {
        if existing == zip_bytes {
            return Ok(());
        }
    }

    let tmp_path = out_path.with_extension("hshader.tmp");
    fs::write(&tmp_path, &zip_bytes).map_err(|e| e.to_string())?;
    if out_path.exists() {
        let _ = fs::remove_file(out_path);
    }
    fs::rename(&tmp_path, out_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_real_shaders() {
        let mods_dir = Path::new(r"C:\Users\wwerz\projects\horizon-mods\mods");
        if !mods_dir.exists() {
            return;
        }
        let tmp_game = std::env::temp_dir().join("horizon_combiner_test");
        let sh_dir = tmp_game.join("shaderpacks");
        let _ = fs::remove_dir_all(&tmp_game);
        fs::create_dir_all(&sh_dir).unwrap();

        let names = [
            "motion-blur.hshader",
            "fps-sharpness.hshader",
            "anime-cel-shading.hshader",
            "aurora-starry-sky.hshader",
            "water-reflections.hshader",
        ];
        for n in names {
            fs::copy(mods_dir.join(n), sh_dir.join(n)).unwrap();
        }
        let enabled: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        let picked = sync_combined_shaders(&tmp_game, &enabled);
        assert_eq!(picked, COMBINED_PACK_NAME);
        assert!(sh_dir.join(COMBINED_PACK_NAME).exists());

        // Copy out for Horizon GPU validation if temp inspect path requested
        if let Ok(out_copy) = std::env::var("HORIZON_TEST_COMBINED_OUT") {
            let _ = fs::copy(sh_dir.join(COMBINED_PACK_NAME), out_copy);
        }
        let _ = fs::remove_dir_all(&tmp_game);
    }
}
