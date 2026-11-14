#![cfg_attr(windows, windows_subsystem = "windows")]

mod client_unpacker;
mod config;
mod game_process;
mod ui_bridge;

use slint::ComponentHandle;
use ui_bridge::{setup_ui_bridge, AppWindow};

fn main() -> Result<(), slint::PlatformError> {
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
