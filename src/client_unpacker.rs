use crate::config::LauncherConfig;
use flate2::read::GzDecoder;
use std::fs;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tar::Archive;

const MAGIC: &[u8; 16] = b"HORIZON_PAYLOAD\0";
const FOOTER_SIZE: u64 = 24; // 8 bytes length (u64 LE) + 16 bytes magic

#[derive(Debug, Clone)]
pub struct UnpackProgress {
    pub progress: f32,
    pub status: String,
    pub percent_text: String,
    pub time_left: String,
}

pub struct CountingReader<R> {
    inner: R,
    bytes_read: Arc<AtomicU64>,
}

impl<R: Read> CountingReader<R> {
    pub fn new(inner: R, bytes_read: Arc<AtomicU64>) -> Self {
        Self { inner, bytes_read }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// Checks if the client is already installed and intact in AppData
pub fn is_client_installed() -> bool {
    let client_dir = LauncherConfig::standard_version_dir();
    let exe = client_dir.join(LauncherConfig::client_exe_name());
    let res = client_dir.join(".local").join("horizon-client").join("26.3").join("horizon-resources.json");

    if exe.exists() && res.exists() {
        if let Ok(meta) = exe.metadata() {
            if meta.len() > 10_000_000 {
                return true;
            }
        }
    }
    false
}

/// Inspects current executable tail for embedded payload
pub fn find_embedded_payload() -> std::io::Result<Option<(u64, u64)>> {
    let exe_path = std::env::current_exe()?;
    let mut file = fs::File::open(&exe_path)?;
    let file_len = file.metadata()?.len();

    if file_len < FOOTER_SIZE {
        return Ok(None);
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;

    let mut footer = [0u8; 24];
    file.read_exact(&mut footer)?;

    let (len_bytes, magic_bytes) = footer.split_at(8);
    if magic_bytes != MAGIC {
        return Ok(None);
    }

    let payload_len = u64::from_le_bytes(len_bytes.try_into().unwrap());
    if payload_len + FOOTER_SIZE > file_len {
        return Ok(None);
    }

    let payload_offset = file_len - FOOTER_SIZE - payload_len;
    Ok(Some((payload_offset, payload_len)))
}

/// Performs unpacking with progress reporting and cancellation support
pub fn unpack_embedded_payload<F>(
    offset: u64,
    length: u64,
    abort_flag: Arc<AtomicBool>,
    mut progress_cb: F,
) -> Result<(), String>
where
    F: FnMut(UnpackProgress),
{
    let tmp_dir = LauncherConfig::standard_version_tmp_dir();
    let final_dir = LauncherConfig::standard_version_dir();

    // Clean any leftover tmp dir from previously interrupted attempt
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    if let Some(parent) = tmp_dir.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("Не удалось создать временную папку {}: {e}", tmp_dir.display()))?;

    let exe_path = std::env::current_exe().map_err(|e| format!("Не удалось определить путь к .exe: {e}"))?;
    let mut file = fs::File::open(&exe_path).map_err(|e| format!("Не удалось открыть .exe: {e}"))?;
    file.seek(SeekFrom::Start(offset)).map_err(|e| format!("Ошибка позиционирования в файле: {e}"))?;

    let limited_reader = file.take(length);
    let bytes_read = Arc::new(AtomicU64::new(0));
    let counting_reader = CountingReader::new(limited_reader, Arc::clone(&bytes_read));
    let buf_reader = BufReader::with_capacity(128 * 1024, counting_reader);
    let decoder = GzDecoder::new(buf_reader);
    let mut archive = Archive::new(decoder);

    let start_time = Instant::now();
    let mut last_ui_update = Instant::now();
    let mut file_count: usize = 0;

    let entries = archive.entries().map_err(|e| format!("Ошибка чтения архива клиента: {e}"))?;

    for entry_res in entries {
        if abort_flag.load(Ordering::SeqCst) {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err("Распаковка отменена пользователем".to_string());
        }

        let mut entry = entry_res.map_err(|e| format!("Ошибка распаковки файла: {e}"))?;
        let path = entry.path().map_err(|e| format!("Некорректный путь в архиве: {e}"))?.to_path_buf();
        
        // Strip leading prefix if any
        let clean_path = path.strip_prefix("game").unwrap_or(&path);
        let dest = tmp_dir.join(clean_path);

        if let Some(parent) = dest.parent() {
            if !parent.exists() {
                let _ = fs::create_dir_all(parent);
            }
        }

        entry.unpack(&dest).map_err(|e| format!("Ошибка извлечения файла {}: {e}", dest.display()))?;
        file_count += 1;

        if last_ui_update.elapsed().as_millis() >= 40 {
            let frac = (file_count as f32 / 28032.0).clamp(0.0, 1.0);
            let elapsed = start_time.elapsed().as_secs_f32();
            let eta_secs = if frac > 0.02 && elapsed > 0.5 {
                let total_est = elapsed / frac;
                (total_est - elapsed).max(0.0)
            } else {
                0.0
            };

            let time_left = if eta_secs > 0.5 {
                if eta_secs >= 60.0 {
                    format!("~{:.0} мин", (eta_secs / 60.0).ceil())
                } else {
                    format!("~{:.0} сек", eta_secs.ceil())
                }
            } else {
                "".to_string()
            };

            let percent_text = format!("{:.0}%", (frac * 100.0).floor());
            let status = format!("Распаковка файлов ({}/28032)...", file_count);

            progress_cb(UnpackProgress {
                progress: frac,
                status,
                percent_text,
                time_left,
            });

            last_ui_update = Instant::now();
        }
    }

    // Verify Integrity
    let exe_check = tmp_dir.join(LauncherConfig::client_exe_name());
    let res_check = tmp_dir.join(".local").join("horizon-client").join("26.3").join("horizon-resources.json");

    if !exe_check.exists() || !res_check.exists() || file_count < 20_000 {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(format!(
            "Ошибка проверки целостности: извлечено {} файлов, основные компоненты не найдены",
            file_count
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&exe_check, fs::Permissions::from_mode(0o755));
    }

    // Final UI update 100%
    progress_cb(UnpackProgress {
        progress: 1.0,
        status: "Завершение подготовки...".to_string(),
        percent_text: "100%".to_string(),
        time_left: "".to_string(),
    });

    // Atomic swap to final_dir
    if final_dir.exists() {
        let _ = fs::remove_dir_all(&final_dir);
    }
    fs::rename(&tmp_dir, &final_dir).map_err(|e| format!("Не удалось переименовать папку клиента: {e}"))?;

    // Ensure options.txt exists with Russian language default in appdata base dir
    let game_dir = LauncherConfig::appdata_base_dir();
    let options_path = game_dir.join("options.txt");
    if !options_path.exists() {
        let default_options = "version:5023\nlang:ru_ru\nforceUnicodeFont:false\n";
        let _ = fs::write(&options_path, default_options);
    } else if let Ok(content) = fs::read_to_string(&options_path) {
        if !content.contains("lang:") {
            let mut updated = content;
            updated.push_str("\nlang:ru_ru\n");
            let _ = fs::write(&options_path, updated);
        } else if content.contains("lang:en_us") {
            let updated = content.replace("lang:en_us", "lang:ru_ru");
            let _ = fs::write(&options_path, updated);
        }
    }

    Ok(())
}

/// Cleanup tmp directory if aborted on process exit
pub fn cleanup_tmp() {
    let tmp_dir = LauncherConfig::standard_version_tmp_dir();
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
