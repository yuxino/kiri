//! Persistent diagnostics for release builds that do not own a console.

use std::panic;

#[cfg(windows)]
use std::fs::{File, OpenOptions};
#[cfg(any(windows, test))]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
const WINDOWS_LOG_MAX_BYTES: u64 = 4 * 1024 * 1024;

pub fn init() {
    #[cfg(windows)]
    init_windows_logger();

    #[cfg(not(windows))]
    {
        let _ = env_logger::try_init();
    }

    install_panic_hook();
}

fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_string());
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("non-string panic payload");
        log::error!(
            "[panic] thread={} location={} payload={}\n{}",
            std::thread::current().name().unwrap_or("unnamed"),
            location,
            payload,
            std::backtrace::Backtrace::force_capture()
        );
        previous(panic_info);
    }));
}

#[cfg(windows)]
fn init_windows_logger() {
    use std::io::{self, Write};

    struct FlushFile(File);

    impl Write for FlushFile {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let written = self.0.write(bytes)?;
            self.0.flush()?;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("RUST_LOG", "warn,kiri_lib=info"),
    );
    builder.format_timestamp_millis();

    let mut log_error = None;
    let log_path = match open_windows_log_file() {
        Ok((file, path)) => {
            builder.target(env_logger::Target::Pipe(Box::new(FlushFile(file))));
            Some(path)
        }
        Err(error) => {
            log_error = Some(error.to_string());
            None
        }
    };
    let _ = builder.try_init();
    match log_path {
        Some(path) => log::info!(
            "[diagnostics] persistent log ready path={} max_bytes={}",
            path.display(),
            WINDOWS_LOG_MAX_BYTES
        ),
        None => log::error!(
            "[diagnostics] persistent log unavailable: {}",
            log_error.as_deref().unwrap_or("unknown error")
        ),
    }
}

#[cfg(windows)]
fn open_windows_log_file() -> std::io::Result<(File, PathBuf)> {
    let directory = dirs::data_local_dir()
        .ok_or_else(|| std::io::Error::other("Windows local data directory is unavailable"))?
        .join("io.yuxino.kiri")
        .join("logs");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("kiri.log");
    rotate_log_if_oversized(&path, WINDOWS_LOG_MAX_BYTES)?;
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    Ok((file, path))
}

#[cfg(any(windows, test))]
fn rotate_log_if_oversized(path: &Path, max_bytes: u64) -> std::io::Result<()> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() <= max_bytes {
        return Ok(());
    }

    let previous = path.with_extension("previous.log");
    if previous.exists() {
        std::fs::remove_file(&previous)?;
    }
    std::fs::rename(path, previous)
}

#[cfg(test)]
mod tests {
    use super::rotate_log_if_oversized;

    #[test]
    fn persistent_log_keeps_one_previous_bounded_file() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("kiri.log");
        let previous = directory.path().join("kiri.previous.log");
        std::fs::write(&previous, b"old previous").unwrap();
        std::fs::write(&current, b"new oversized log").unwrap();

        rotate_log_if_oversized(&current, 4).unwrap();

        assert!(!current.exists());
        assert_eq!(std::fs::read(previous).unwrap(), b"new oversized log");
    }

    #[test]
    fn persistent_log_does_not_rotate_at_the_limit() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("kiri.log");
        std::fs::write(&current, b"1234").unwrap();

        rotate_log_if_oversized(&current, 4).unwrap();

        assert_eq!(std::fs::read(current).unwrap(), b"1234");
        assert!(!directory.path().join("kiri.previous.log").exists());
    }
}
