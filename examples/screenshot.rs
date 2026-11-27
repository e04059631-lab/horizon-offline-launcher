//! Dev tool: renders the redesigned launcher UI headlessly at standard (1100x720) and narrow (940x640) widths.

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

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    window.set_size(slint::PhysicalSize::new(1100, 720));

    let app = AppWindow::new().unwrap();
    backdrop::apply_preset(&app, "blue", None);
    app.set_backdrop_index(0);
    app.set_nickname("wwerz".into());
    app.set_client_ready(true);
    app.set_has_embedded_payload(true);
    app.set_mods_repo_url("https://raw.githubusercontent.com/xwwerz/horizon-mods/main/catalog.json".into());
    app.set_mods(ModelRc::new(VecModel::from(vec![
        ModInfo { name: "Motion Blur".into(), file_name: "motion-blur.hshader".into(), size_kb: 2, enabled: true, is_shader: true, is_resourcepack: false, has_icon: false, icon: Image::default() },
        ModInfo { name: "Horizon Ultra".into(), file_name: "horizon-ultra.hshader".into(), size_kb: 3, enabled: true, is_shader: true, is_resourcepack: false, has_icon: false, icon: Image::default() },
        ModInfo { name: "Low On Fire".into(), file_name: "low-on-fire.zip".into(), size_kb: 42, enabled: true, is_shader: false, is_resourcepack: true, has_icon: false, icon: Image::default() },
    ])));
    app.set_active_mods_count(3);
    app.set_total_mods_count(3);
    app.set_catalog_mods(ModelRc::new(VecModel::from(vec![
        CatalogModInfo {
            id: "motion-blur".into(),
            name: "Motion Blur".into(),
            description: "Плавное размытие в движении для Horizon 26.3 с адаптацией под 144 Гц и настройками силы и качества в игре.".into(),
            version: "1.0.0".into(),
            file_name: "motion-blur.hshader".into(),
            size_kb: 2,
            installed: true,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: true,
            is_resourcepack: false,
        },
        CatalogModInfo {
            id: "horizon-ultra".into(),
            name: "Horizon Ultra".into(),
            description: "Флагманский комбо-шейдер: мягкие тени, зеркальная вода, цветной 3D-свет, атмосферное небо, Bloom и Motion Blur.".into(),
            version: "1.0.0".into(),
            file_name: "horizon-ultra.hshader".into(),
            size_kb: 3,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: true,
            is_resourcepack: false,
        },
        CatalogModInfo {
            id: "better-bedwars-wool".into(),
            name: "Better BedWars Wool".into(),
            description: "Гладкая сочная шерсть всех 16 цветов с чёткой рамкой граней — идеально для бриджинга и BedWars.".into(),
            version: "1.0.0".into(),
            file_name: "better-bedwars-wool.zip".into(),
            size_kb: 12,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: false,
            is_resourcepack: true,
        },
        CatalogModInfo {
            id: "fps-sharpness".into(),
            name: "FPS Sharpness (CAS)".into(),
            description: "Адаптивная чёткость AMD CAS — убирает «мыло» с текстур вдали без ореолов и практически с 0% нагрузки на видеокарту.".into(),
            version: "1.0.0".into(),
            file_name: "fps-sharpness.hshader".into(),
            size_kb: 1,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: true,
            is_resourcepack: false,
        },
    ])));
    app.set_recent_nicknames(ModelRc::new(VecModel::from(vec![
        SharedString::from("wwerz"),
        SharedString::from("Steve"),
    ])));
    app.show().unwrap();

    let shots: [(&str, u32, u32, i32, i32, bool, &str, bool); 2] = [
        ("v7_settings", 1100, 720, 2, 0, true, "blue", false),
        ("v7_update_dialog", 1100, 720, 0, 0, true, "blue", true),
    ];

    for (name, w, h, tab, mods_view, dark, bg, show_upd) in shots {
        window.set_size(slint::PhysicalSize::new(w, h));
        backdrop::apply_preset(&app, bg, None);
        app.set_backdrop_index(backdrop::preset_to_index(bg));
        app.set_dark_mode(dark);
        app.global::<Theme>().set_dark(dark);
        app.set_active_tab(tab);
        app.set_mods_view(mods_view);
        app.set_app_version("2.1.0".into());
        app.set_update_new_version("2.2.0".into());
        app.set_update_size_text("14.8 МБ".into());
        app.set_update_notes(
            "Что нового в v2.2.0\n\n• Автообновление прямо в лаунчере с прогресс-баром и автоматическим перезапуском.\n• Нативная поддержка Linux (x86_64) и установка одной командой.\n• Улучшена скорость загрузки каталога шейдеров и PvP-паков."
                .into(),
        );
        app.set_show_update_dialog(show_upd);
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
