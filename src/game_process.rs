use crate::config::LauncherConfig;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, PartialEq)]
pub enum GameState {
    NotRunning,
    Starting,
    Running { pid: u32 },
    Exited { code: i32 },
    Failed(String),
}

pub struct GameProcess {
    pub state: Arc<Mutex<GameState>>,
    pub log_sender: Sender<String>,
    pub log_receiver: Arc<Mutex<Receiver<String>>>,
    pub child: Arc<Mutex<Option<Child>>>,
}

impl GameProcess {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            state: Arc::new(Mutex::new(GameState::NotRunning)),
            log_sender: tx,
            log_receiver: Arc::new(Mutex::new(rx)),
            child: Arc::new(Mutex::new(None)),
        }
    }

    pub fn current_state(&self) -> GameState {
        self.state.lock().unwrap().clone()
    }

    pub fn drain_logs(&self) -> Vec<String> {
        let rx = self.log_receiver.lock().unwrap();
        let mut lines = Vec::new();
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }
        lines
    }

    pub fn is_running(&self) -> bool {
        matches!(self.current_state(), GameState::Running { .. })
    }

    pub fn is_busy(&self) -> bool {
        matches!(
            self.current_state(),
            GameState::Starting | GameState::Running { .. }
        )
    }

    pub fn kill(&self) {
        let mut child_guard = self.child.lock().unwrap();
        if let Some(mut child) = child_guard.take() {
            #[cfg(target_os = "windows")]
            {
                let _ = child.kill();
                let _ = child.wait();
            }
            #[cfg(not(target_os = "windows"))]
            {
                let pid = child.id();
                let _ = Command::new("kill").arg("-15").arg(pid.to_string()).status();
                let mut exited = false;
                for _ in 0..30 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if let Ok(Some(_)) = child.try_wait() {
                        exited = true;
                        break;
                    }
                }
                if !exited {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        *self.state.lock().unwrap() = GameState::NotRunning;
    }

    pub fn launch(&self, config: &LauncherConfig, direct_server: Option<&str>) -> Result<(), String> {
        let version_dir = config
            .find_version_dir()
            .ok_or_else(|| "Не найдена папка с установленной версией Horizon 26.3!".to_string())?;

        let exe_name = LauncherConfig::client_exe_name();
        let exe_path = version_dir.join(exe_name);
        if !exe_path.exists() {
            return Err(format!("Файл {} не найден в {}", exe_name, version_dir.display()));
        }

        // Locate resources
        let res_dir = version_dir.join(".local").join("horizon-client").join("26.3");
        if !res_dir.join("horizon-resources.json").exists() {
            return Err("Ресурсы игры не подготовлены! Файл horizon-resources.json отсутствует.".to_string());
        }

        let game_dir = config.get_game_dir();
        let _ = std::fs::create_dir_all(&game_dir);

        let mut cmd = Command::new(&exe_path);
        cmd.arg("--resources").arg(&res_dir);
        cmd.arg("--game-dir").arg(&game_dir);
        cmd.arg("--home").arg(&game_dir);

        // Offline username
        if config.username.trim().is_empty() {
            return Err("Никнейм не указан! Пожалуйста, укажите никнейм перед запуском игры.".to_string());
        }
        let name = config.username.trim();
        cmd.arg("--name").arg(name);

        if let Some(srv) = direct_server {
            if !srv.trim().is_empty() {
                cmd.arg("--server").arg(srv.trim());
            }
        }

        // Graphics and gameplay
        cmd.arg("--render-distance").arg(config.render_distance.to_string());
        if config.fullscreen {
            cmd.arg("--fullscreen");
        } else {
            cmd.arg("--width").arg(config.width.to_string());
            cmd.arg("--height").arg(config.height.to_string());
        }

        if config.creative_mode {
            cmd.arg("--creative");
        }

        // Working dir
        cmd.current_dir(&game_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        *self.state.lock().unwrap() = GameState::Starting;
        let tx = self.log_sender.clone();
        tx.send(format!("Запуск: {} с ником '{}'...", exe_path.file_name().unwrap_or_default().to_string_lossy(), name))
            .unwrap_or_default();
        tx.send(format!("Директория игры: {}", game_dir.display())).unwrap_or_default();
        tx.send(format!("Ресурсы: {}", res_dir.display())).unwrap_or_default();

        let mut child = cmd.spawn().map_err(|e| format!("Ошибка запуска процесса: {e}"))?;
        let pid = child.id();
        *self.state.lock().unwrap() = GameState::Running { pid };
        tx.send(format!("Процесс запущен успешно [PID: {pid}]")).unwrap_or_default();

        let state_clone = Arc::clone(&self.state);
        let child_arc = Arc::clone(&self.child);

        // Take pipes for background log streaming
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        *child_arc.lock().unwrap() = Some(child);

        // Thread to read stdout
        if let Some(stdout) = stdout {
            let tx_out = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().flatten() {
                    let _ = tx_out.send(line);
                }
            });
        }

        // Thread to read stderr
        if let Some(stderr) = stderr {
            let tx_err = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    let _ = tx_err.send(format!("[LOG] {line}"));
                }
            });
        }

        // Waiter thread
        thread::spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_millis(500));
                let mut guard = child_arc.lock().unwrap();
                if let Some(ref mut c) = *guard {
                    match c.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code().unwrap_or(0);
                            let _ = tx.send(format!("Игра завершена с кодом: {code}"));
                            *state_clone.lock().unwrap() = GameState::Exited { code };
                            *guard = None;
                            break;
                        }
                        Ok(None) => {} // still running
                        Err(e) => {
                            let _ = tx.send(format!("Ошибка ожидания процесса: {e}"));
                            *state_clone.lock().unwrap() = GameState::Failed(e.to_string());
                            *guard = None;
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        });

        Ok(())
    }
}
