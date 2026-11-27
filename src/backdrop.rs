//! Liquid Glass multi-wallpaper backdrop engine:
//! Supports 4 embedded wallpapers + custom user image (.png/.jpg/.jpeg)
//! with automatic frosted Liquid Glass blur generation and native Windows file picker.

use crate::ui_bridge::{AppWindow, Backdrop};
use image::{imageops, DynamicImage, RgbaImage};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::path::{Path, PathBuf};

static BG_ABSTRACT: &[u8] = include_bytes!("../assets/bg-abstract.jpg");
static BG_AURORA: &[u8] = include_bytes!("../assets/bg-aurora.jpg");
static BG_CHERRY: &[u8] = include_bytes!("../assets/bg-cherry.jpg");
static BG_SUNSET: &[u8] = include_bytes!("../assets/background.jpg");

const BLUR_SIGMA: f32 = 8.5;
const DOWNSCALE: u32 = 4;
const MAX_CUSTOM_DIM: u32 = 1920;

pub fn preset_to_index(preset: &str) -> i32 {
    match preset {
        "abstract" => 0,
        "aurora" => 1,
        "cherry" => 2,
        "sunset" => 3,
        "custom" => 4,
        _ => 0,
    }
}

#[allow(dead_code)]
pub fn index_to_preset(idx: i32) -> &'static str {
    match idx {
        0 => "abstract",
        1 => "aurora",
        2 => "cherry",
        3 => "sunset",
        4 => "custom",
        _ => "abstract",
    }
}

fn embedded_bytes(preset: &str) -> &'static [u8] {
    match preset {
        "aurora" => BG_AURORA,
        "cherry" => BG_CHERRY,
        "sunset" => BG_SUNSET,
        _ => BG_ABSTRACT,
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
    let mut frosted = imageops::fast_blur(&small, BLUR_SIGMA);
    vibrancy(&mut frosted, 1.28);

    let backdrop = window.global::<Backdrop>();
    backdrop.set_sharp(to_slint(&sharp));
    backdrop.set_blurred(to_slint(&frosted));
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
    // Verify image decodes cleanly before replacing the saved custom background
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
    fn test_all_builtin_wallpapers_decode() {
        for preset in ["abstract", "aurora", "cherry", "sunset"] {
            let img = decode_backdrop(preset, None);
            assert!(img.is_some(), "built-in wallpaper '{preset}' failed to decode");
        }
        assert_eq!(preset_to_index("abstract"), 0);
        assert_eq!(preset_to_index("aurora"), 1);
        assert_eq!(preset_to_index("cherry"), 2);
        assert_eq!(preset_to_index("sunset"), 3);
        assert_eq!(preset_to_index("custom"), 4);
        assert_eq!(index_to_preset(2), "cherry");
    }
}
