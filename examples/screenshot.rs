//! Dev tool: renders the launcher UI headlessly into PNG files (light & dark).
//! Usage: cargo run --example screenshot -- <out_dir>

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Image, ModelRc, Rgb8Pixel, SharedString, VecModel};
use std::rc::Rc;

mod ui_bridge {
    slint::include_modules!();
}
#[path = "../src/backdrop.rs"]
mod backdrop;

use ui_bridge::*;

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let (w, h) = (1100u32, 720u32);

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    window.set_size(slint::PhysicalSize::new(w, h));

    let app = AppWindow::new().unwrap();
    backdrop::apply(&app);
    app.set_nickname("wwerz".into());
    app.set_client_ready(true);
    app.set_has_embedded_payload(true);
    app.set_mods_repo_url("https://github.com/wwerz1/horizon-mods".into());
    app.set_mods(ModelRc::new(VecModel::from(vec![
        ModInfo { name: "motion-blur.hshader".into(), file_name: "motion-blur.hshader".into(), size_kb: 2, enabled: true, is_shader: true },
        ModInfo { name: "sodium-horizon.hmod".into(), file_name: "sodium-horizon.hmod".into(), size_kb: 812, enabled: true, is_shader: false },
    ])));
    app.set_active_mods_count(1);
    app.set_total_mods_count(2);
    app.set_catalog_mods(ModelRc::new(VecModel::from(vec![
        CatalogModInfo {
            id: "sodium-horizon".into(),
            name: "Sodium Horizon".into(),
            description: "Повышает FPS и оптимизирует рендеринг чанков для Horizon 26.3.".into(),
            version: "1.0.0".into(),
            file_name: "sodium-horizon.hmod".into(),
            size_kb: 812,
            installed: true,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
        },
        CatalogModInfo {
            id: "horizon-minimap".into(),
            name: "Horizon Minimap".into(),
            description: "Удобная мини-карта с метками, координатами и картой мира.".into(),
            version: "2.1.0".into(),
            file_name: "horizon-minimap.hmod".into(),
            size_kb: 1540,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
        },
        CatalogModInfo {
            id: "dynamic-lights".into(),
            name: "Dynamic Lights".into(),
            description: "Факел в руке и брошенные источники света освещают пещеры в реальном времени.".into(),
            version: "1.4.2".into(),
            file_name: "dynamic-lights.hmod".into(),
            size_kb: 390,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
        },
    ])));
    app.set_mods_dir_path("C:\\Users\\wwerz\\AppData\\Roaming\\horizon-offline-game\\horizon-mods".into());
    app.set_recent_nicknames(ModelRc::new(VecModel::from(vec![
        SharedString::from("wwerz"),
        SharedString::from("Steve"),
    ])));
    app.show().unwrap();

    let shots: [(&str, i32, bool); 3] = [
        ("dark_mods", 1, true),
        ("light_mods", 1, false),
        ("dark_settings", 2, true),
    ];

    for (name, tab, dark) in shots {
        app.set_dark_mode(dark);
        app.global::<Theme>().set_dark(dark);
        app.set_active_tab(tab);
        slint::platform::update_timers_and_animations();
        window.request_redraw();

        let mut buf = vec![Rgb8Pixel::default(); (w * h) as usize];
        window.draw_if_needed(|r| {
            r.render(&mut buf, w as usize);
        });
        let bytes: Vec<u8> = buf.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        let img = image::RgbImage::from_raw(w, h, bytes).unwrap();
        let path = format!("{out_dir}/shot_{name}.png");
        img.save(&path).unwrap();
        println!("saved {path}");
    }
}
