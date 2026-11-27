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
        ModInfo { name: "ruby-arsenal.hmod".into(), file_name: "ruby-arsenal.hmod".into(), size_kb: 10, enabled: true, is_shader: false, is_resourcepack: false },
        ModInfo { name: "motion-blur.hshader".into(), file_name: "motion-blur.hshader".into(), size_kb: 2, enabled: true, is_shader: true, is_resourcepack: false },
        ModInfo { name: "low-fire.zip".into(), file_name: "low-fire.zip".into(), size_kb: 40, enabled: true, is_shader: false, is_resourcepack: true },
    ])));
    app.set_active_mods_count(3);
    app.set_total_mods_count(3);
    app.set_engine_mods(ModelRc::new(VecModel::from(vec![
        EngineModInfo { id: "keystrokes".into(), name: "Keystrokes + CPS".into(), description: "Индикатор нажатий WASD, пробела, ЛКМ/ПКМ и счётчик кликов в секунду (CPS).".into(), category: "PvP".into(), enabled: true },
        EngineModInfo { id: "fps".into(), name: "FPS Counter + 1% Lows".into(), description: "Компактный счётчик кадров FPS и просадок 1% Lows поверх экрана.".into(), category: "HUD".into(), enabled: true },
        EngineModInfo { id: "zoom".into(), name: "Zoom (Приближение)".into(), description: "Плавное кинематографичное приближение камеры (как в OptiFine) без подзорной трубы.".into(), category: "PvP".into(), enabled: true },
        EngineModInfo { id: "dynamic_lights".into(), name: "Dynamic Lights".into(), description: "Факелы, фонари и светящиеся предметы в руке и на земле освещают пещеры в реальном времени.".into(), category: "QoL".into(), enabled: true },
        EngineModInfo { id: "minimap".into(), name: "Minimap (Миникарта)".into(), description: "Встроенная круглая вращающаяся миникарта местности в углу экрана.".into(), category: "HUD".into(), enabled: false },
        EngineModInfo { id: "no_hurt_cam".into(), name: "No Hurt Cam".into(), description: "Убирает тряску камеры при получении урона — прицел остаётся идеально стабильным.".into(), category: "PvP".into(), enabled: true },
    ])));
    app.set_active_engine_mods_count(5);
    app.set_catalog_mods(ModelRc::new(VecModel::from(vec![
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
            id: "fps-sharpness".into(),
            name: "FPS Sharpness (CAS)".into(),
            description: "Адаптивная чёткость AMD CAS — убирает «мыло» с текстур вдали без ореолов и практически с 0% нагрузки на видеокарту.".into(),
            version: "1.0.0".into(),
            file_name: "fps-sharpness.hshader".into(),
            size_kb: 1,
            installed: true,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: true,
            is_resourcepack: false,
        },
        CatalogModInfo {
            id: "crystal-underwater".into(),
            name: "Crystal Underwater".into(),
            description: "Кристально чистая видимость под водой без мутного тумана + красивые солнечные каустики на морском дне.".into(),
            version: "1.0.0".into(),
            file_name: "crystal-underwater.hshader".into(),
            size_kb: 1,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: true,
            is_resourcepack: false,
        },
        CatalogModInfo {
            id: "clear-nether-end".into(),
            name: "Clear Nether & End".into(),
            description: "Полностью убирает густой туман в Незере и Энде и слегка подсвечивает тёмные своды для отличного обзора.".into(),
            version: "1.0.0".into(),
            file_name: "clear-nether-end.hshader".into(),
            size_kb: 1,
            installed: false,
            downloading: false,
            has_icon: false,
            icon: Image::default(),
            is_shader: true,
            is_resourcepack: false,
        },
    ])));
    app.set_mods_dir_path("C:\\Users\\wwerz\\AppData\\Roaming\\horizon-offline-game\\horizon-mods".into());
    app.set_recent_nicknames(ModelRc::new(VecModel::from(vec![
        SharedString::from("wwerz"),
        SharedString::from("Steve"),
    ])));
    app.show().unwrap();

    let shots: [(&str, i32, i32, bool); 4] = [
        ("dark_mods", 1, 0, true),
        ("dark_engine_mods", 1, 1, true),
        ("light_mods", 1, 1, false),
        ("dark_settings", 2, 0, true),
    ];

    for (name, tab, subtab, dark) in shots {
        app.set_dark_mode(dark);
        app.global::<Theme>().set_dark(dark);
        app.set_active_tab(tab);
        app.set_left_mods_subtab(subtab);
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
