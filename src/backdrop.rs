//! Liquid Glass multi-wallpaper backdrop engine with automatic adaptive accent extraction:
//! Supports 4 embedded real-photography wallpapers + custom user image (.png/.jpg/.jpeg),
//! computes frosted Liquid Glass blur, and extracts a harmonious UI accent palette from every image.

use crate::ui_bridge::{AppWindow, Backdrop, Theme};
use image::{imageops, DynamicImage, RgbaImage};
use slint::{Color, ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::path::{Path, PathBuf};

static BG_BLUE: &[u8] = include_bytes!("../assets/bg-blue.jpg");
static BG_AURORA: &[u8] = include_bytes!("../assets/bg-aurora.jpg");
static BG_DUSK: &[u8] = include_bytes!("../assets/bg-dusk.jpg");
static BG_SUNSET: &[u8] = include_bytes!("../assets/bg-sunset.jpg");

const BLUR_SIGMA: f32 = 8.5;
const DOWNSCALE: u32 = 4;
const MAX_CUSTOM_DIM: u32 = 1920;

#[derive(Debug, Clone, Copy)]
pub struct AccentPalette {
    pub accent: Color,
    pub hover: Color,
    pub pressed: Color,
    pub light: Color,
    pub label_dark: Color,
    pub label_light: Color,
    pub selection: Color,
}

pub fn preset_to_index(preset: &str) -> i32 {
    match preset {
        "blue" | "abstract" => 0,
        "aurora" => 1,
        "dusk" | "cherry" => 2,
        "sunset" => 3,
        "custom" => 4,
        _ => 0,
    }
}

#[allow(dead_code)]
pub fn index_to_preset(idx: i32) -> &'static str {
    match idx {
        0 => "blue",
        1 => "aurora",
        2 => "dusk",
        3 => "sunset",
        4 => "custom",
        _ => "blue",
    }
}

fn embedded_bytes(preset: &str) -> &'static [u8] {
    match preset {
        "aurora" => BG_AURORA,
        "dusk" | "cherry" => BG_DUSK,
        "sunset" => BG_SUNSET,
        _ => BG_BLUE,
    }
}

fn decode_backdrop(preset: &str, custom_path: Option<&Path>) -> Option<DynamicImage> {
    if preset == "custom" {
        if let Some(p) = custom_path {
            if let Ok(img) = image::open(p) {
                if img.width() > MAX_CUSTOM_DIM || img.height() > MAX_CUSTOM_DIM {
                    return Some(img.resize(
                        MAX_CUSTOM_DIM,
                        MAX_CUSTOM_DIM,
                        imageops::FilterType::Triangle,
                    ));
                }
                return Some(img);
            }
        }
    }
    image::load_from_memory(embedded_bytes(preset)).ok()
}

pub fn apply_preset(window: &AppWindow, preset: &str, custom_path: Option<&Path>) {
    let Some(decoded) = decode_backdrop(preset, custom_path) else {
        return;
    };
    let sharp = decoded.to_rgba8();

    let (w, h) = sharp.dimensions();
    let small = imageops::resize(
        &sharp,
        (w / DOWNSCALE).max(1),
        (h / DOWNSCALE).max(1),
        imageops::FilterType::Triangle,
    );

    let palette = extract_accent_palette(&small);
    apply_accent_palette(window, palette);

    let mut frosted = imageops::fast_blur(&small, BLUR_SIGMA);
    vibrancy(&mut frosted, 1.28);

    let backdrop = window.global::<Backdrop>();
    backdrop.set_sharp(to_slint(&sharp));
    backdrop.set_blurred(to_slint(&frosted));
}

fn apply_accent_palette(window: &AppWindow, p: AccentPalette) {
    let theme = window.global::<Theme>();
    theme.set_accent(p.accent);
    theme.set_accent_hover(p.hover);
    theme.set_accent_pressed(p.pressed);
    theme.set_accent_light(p.light);
    theme.set_accent_label_dark(p.label_dark);
    theme.set_accent_label_light(p.label_light);
    theme.set_accent_selection(p.selection);
}

/// Extracts a clean, high-contrast adaptive UI accent palette from any wallpaper image.
pub fn extract_accent_palette(img: &RgbaImage) -> AccentPalette {
    let mut bin_weight = [0.0f64; 36];
    let mut bin_sin = [0.0f64; 36];
    let mut bin_cos = [0.0f64; 36];
    let mut bin_sat = [0.0f64; 36];
    let mut total_weight = 0.0f64;

    for p in img.pixels() {
        let [r8, g8, b8, a8] = p.0;
        if a8 < 128 {
            continue;
        }
        let r = r8 as f64 / 255.0;
        let g = g8 as f64 / 255.0;
        let b = b8 as f64 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        if max < 0.14 || delta < 0.04 {
            continue;
        }
        let sat = delta / max;
        if sat < 0.15 || (max > 0.96 && sat < 0.25) {
            continue;
        }

        let mut hue = if (max - r).abs() < 1e-9 {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < 1e-9 {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        if hue < 0.0 {
            hue += 360.0;
        }

        // Favor rich, clean mid-tone colors over noisy shadows or blown highlights
        let mid_v = (1.0 - (max - 0.54).abs() * 1.35).max(0.08);
        let w = sat * sat * mid_v;
        let idx = ((hue / 10.0).floor() as usize) % 36;
        let rad = hue.to_radians();

        bin_weight[idx] += w;
        bin_sin[idx] += rad.sin() * w;
        bin_cos[idx] += rad.cos() * w;
        bin_sat[idx] += sat * w;
        total_weight += w;
    }

    let (hue_deg, sat_ui) = if total_weight < 8.0 {
        // Nearly monochrome wallpaper -> clean Apple Sapphire Blue fallback
        (212.0, 0.80)
    } else {
        let mut best_bin = 0usize;
        let mut best_score = -1.0f64;
        for i in 0..36 {
            let prev = (i + 35) % 36;
            let next = (i + 1) % 36;
            let smoothed = 0.25 * bin_weight[prev] + 0.50 * bin_weight[i] + 0.25 * bin_weight[next];
            if smoothed > best_score {
                best_score = smoothed;
                best_bin = i;
            }
        }

        let prev = (best_bin + 35) % 36;
        let next = (best_bin + 1) % 36;
        let mut s_sum = 0.0;
        let mut c_sum = 0.0;
        let mut sat_sum = 0.0;
        let mut w_sum = 0.0;
        for b_idx in [prev, best_bin, next] {
            s_sum += bin_sin[b_idx];
            c_sum += bin_cos[b_idx];
            sat_sum += bin_sat[b_idx];
            w_sum += bin_weight[b_idx];
        }
        let mut mean_hue = s_sum.atan2(c_sum).to_degrees();
        if mean_hue < 0.0 {
            mean_hue += 360.0;
        }
        let mean_sat = (sat_sum / w_sum.max(1e-6)).clamp(0.68, 0.86);
        (mean_hue, mean_sat)
    };

    // Perceptually balanced base lightness so white text on primary buttons always reads crisply
    let l_base = if (45.0..170.0).contains(&hue_deg) {
        0.41 // gold / lime / emerald / bright teal -> rich deeper tone for crisp white text
    } else if (170.0..205.0).contains(&hue_deg) {
        0.46 // cyan-sky
    } else if (205.0..265.0).contains(&hue_deg) {
        0.54 // sapphire / cobalt / indigo
    } else if (265.0..335.0).contains(&hue_deg) {
        0.56 // violet / orchid / rose
    } else {
        0.52 // warm coral / amber / crimson
    };

    AccentPalette {
        accent: hsl_to_color(hue_deg, sat_ui, l_base, 242),
        hover: hsl_to_color(hue_deg, (sat_ui + 0.04).min(0.94), (l_base + 0.06).min(0.68), 250),
        pressed: hsl_to_color(hue_deg, sat_ui, (l_base - 0.08).max(0.28), 245),
        light: hsl_to_color(hue_deg, (sat_ui + 0.04).min(0.95), (l_base + 0.17).min(0.78), 255),
        label_dark: hsl_to_color(hue_deg, (sat_ui + 0.06).min(0.96), 0.72, 255),
        label_light: hsl_to_color(hue_deg, (sat_ui + 0.06).min(0.94), 0.38, 255),
        selection: hsl_to_color(hue_deg, sat_ui, l_base, 85),
    }
}

fn hsl_to_color(h: f64, s: f64, l: f64, alpha: u8) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h % 360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if hp < 1.0 {
        (c, x, 0.0)
    } else if hp < 2.0 {
        (x, c, 0.0)
    } else if hp < 3.0 {
        (0.0, c, x)
    } else if hp < 4.0 {
        (0.0, x, c)
    } else if hp < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    let r = ((r1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = ((g1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = ((b1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_argb_u8(alpha, r, g, b)
}

#[allow(dead_code)]
pub fn apply_custom_thumb(window: &AppWindow, custom_path: Option<&Path>) {
    let backdrop = window.global::<Backdrop>();
    if let Some(p) = custom_path {
        if let Ok(img) = image::open(p) {
            let thumb = img
                .resize_to_fill(240, 135, imageops::FilterType::Triangle)
                .to_rgba8();
            backdrop.set_custom_thumb(to_slint(&thumb));
            backdrop.set_has_custom_thumb(true);
            return;
        }
    }
    backdrop.set_has_custom_thumb(false);
}

fn vibrancy(img: &mut RgbaImage, saturation: f32) {
    for p in img.pixels_mut() {
        let [r, g, b, a] = p.0;
        let (r, g, b) = (r as f32, g as f32, b as f32);
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        let adjust = |c: f32| (lum + (c - lum) * saturation).clamp(0.0, 255.0) as u8;
        p.0 = [adjust(r), adjust(g), adjust(b), a];
    }
}

fn to_slint(img: &RgbaImage) -> Image {
    let buffer =
        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height());
    Image::from_rgba8(buffer)
}

/// Validates and copies a user-chosen wallpaper into `dest_dir/custom_backdrop.<ext>`.
#[allow(dead_code)]
pub fn import_custom_backdrop(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    image::open(src).map_err(|e| format!("Не удалось открыть изображение: {e}"))?;
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .filter(|s| matches!(s.as_str(), "png" | "jpg" | "jpeg"))
        .unwrap_or_else(|| "jpg".to_string());
    let _ = std::fs::create_dir_all(dest_dir);
    let dest = dest_dir.join(format!("custom_backdrop.{ext}"));
    std::fs::copy(src, &dest).map_err(|e| format!("Не удалось скопировать фон: {e}"))?;
    Ok(dest)
}

/// Opens a native OS image file picker dialog (PNG / JPG).
#[cfg(windows)]
#[allow(dead_code)]
pub fn pick_image_file() -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct OpenFileNameW {
        lStructSize: u32,
        hwndOwner: *mut std::ffi::c_void,
        hInstance: *mut std::ffi::c_void,
        lpstrFilter: *const u16,
        lpstrCustomFilter: *mut u16,
        nMaxCustFilter: u32,
        nFilterIndex: u32,
        lpstrFile: *mut u16,
        nMaxFile: u32,
        lpstrFileTitle: *mut u16,
        nMaxFileTitle: u32,
        lpstrInitialDir: *const u16,
        lpstrTitle: *const u16,
        Flags: u32,
        nFileOffset: u16,
        nFileExtension: u16,
        lpstrDefExt: *const u16,
        lCustData: isize,
        lpfnHook: *mut std::ffi::c_void,
        lpTemplateName: *const u16,
        pvReserved: *mut std::ffi::c_void,
        dwReserved: u32,
        FlagsEx: u32,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(lpofn: *mut OpenFileNameW) -> i32;
    }

    const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
    const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
    const OFN_NOCHANGEDIR: u32 = 0x0000_0008;

    let filter: Vec<u16> = "Изображения (*.png;*.jpg;*.jpeg)\0*.png;*.jpg;*.jpeg\0Все файлы (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "Выбери фон для Horizon Launcher\0"
        .encode_utf16()
        .collect();
    let mut file_buf = vec![0u16; 4096];

    let mut ofn = OpenFileNameW {
        lStructSize: std::mem::size_of::<OpenFileNameW>() as u32,
        hwndOwner: std::ptr::null_mut(),
        hInstance: std::ptr::null_mut(),
        lpstrFilter: filter.as_ptr(),
        lpstrCustomFilter: std::ptr::null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: file_buf.as_mut_ptr(),
        nMaxFile: file_buf.len() as u32,
        lpstrFileTitle: std::ptr::null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: std::ptr::null(),
        lpstrTitle: title.as_ptr(),
        Flags: OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: std::ptr::null(),
        lCustData: 0,
        lpfnHook: std::ptr::null_mut(),
        lpTemplateName: std::ptr::null(),
        pvReserved: std::ptr::null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };

    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None;
    }
    let len = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
    if len == 0 {
        return None;
    }
    Some(PathBuf::from(OsString::from_wide(&file_buf[..len])))
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn pick_image_file() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_builtin_wallpapers_and_adaptive_accents() {
        for preset in ["blue", "aurora", "dusk", "sunset"] {
            let img = decode_backdrop(preset, None)
                .unwrap_or_else(|| panic!("built-in wallpaper '{preset}' failed to decode"));
            let small = img.resize(320, 180, imageops::FilterType::Triangle).to_rgba8();
            let pal = extract_accent_palette(&small);
            println!(
                "Wallpaper '{preset}' -> accent rgb({}, {}, {})",
                pal.accent.red(),
                pal.accent.green(),
                pal.accent.blue()
            );
            if preset == "blue" {
                assert!(
                    pal.accent.blue() > pal.accent.red() + 40,
                    "blue wallpaper should extract a blue accent, got {:?}",
                    pal.accent
                );
            }
            if preset == "aurora" {
                assert!(
                    pal.accent.green() > pal.accent.red() + 20,
                    "aurora wallpaper should extract a green/teal accent, got {:?}",
                    pal.accent
                );
            }
        }
    }
}
