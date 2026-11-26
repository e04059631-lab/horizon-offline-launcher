#![cfg_attr(windows, windows_subsystem = "windows")]

mod backdrop;
mod client_unpacker;
mod config;
mod game_process;
mod mod_catalog;
mod ui_bridge;

use slint::ComponentHandle;
use ui_bridge::{setup_ui_bridge, AppWindow};

#[cfg(windows)]
mod win_timer {
    #[link(name = "winmm")]
    extern "system" {
        fn timeBeginPeriod(uPeriod: u32) -> u32;
        fn timeEndPeriod(uPeriod: u32) -> u32;
    }

    pub struct HighResTimerGuard;

    impl HighResTimerGuard {
        pub fn enable() -> Self {
            unsafe {
                timeBeginPeriod(1);
            }
            Self
        }
    }

    impl Drop for HighResTimerGuard {
        fn drop(&mut self) {
            unsafe {
                timeEndPeriod(1);
            }
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(windows)]
    let _high_res_timer = win_timer::HighResTimerGuard::enable();

    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {:?}\n", info);
        if let Some(base) = dirs::data_dir() {
            let log_dir = base.join("horizon-offline-game").join("logs");
            let _ = std::fs::create_dir_all(&log_dir);
            let _ = std::fs::write(log_dir.join("panic.txt"), &msg);
        }
    }));

    let app = AppWindow::new()?;
    setup_ui_bridge(&app);

    let res = app.run();
    client_unpacker::cleanup_tmp();
    res
}
