//! Liquid Glass backdrop: decodes the embedded wallpaper and produces a single
//! lightweight frosted copy sampled by every glass surface (fast GPU batching).

use crate::ui_bridge::{AppWindow, Backdrop};
use image::{imageops, RgbaImage};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

static WALLPAPER: &[u8] = include_bytes!("../assets/background.jpg");

const BLUR_SIGMA: f32 = 8.5;
const DOWNSCALE: u32 = 4;

pub fn apply(window: &AppWindow) {
    let Ok(decoded) = image::load_from_memory(WALLPAPER) else {
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
    vibrancy(&mut frosted, 1.30);

    let backdrop = window.global::<Backdrop>();
    backdrop.set_sharp(to_slint(&sharp));
    backdrop.set_blurred(to_slint(&frosted));
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
