use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A log that appends to a per-day text file. The app keeps running even if a write fails.
struct FileLogger {
    directory: PathBuf,
    echo: bool,
    lock: Mutex<()>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let now = chrono::Local::now();
        let line = format!(
            "{} {:5} {}\n",
            now.format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.args()
        );

        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.echo {
            eprint!("{line}");
        }
        let path = self.directory.join(format!("{}.log", now.format("%Y-%m-%d")));
        let _ = std::fs::create_dir_all(&self.directory)
            .and_then(|_| OpenOptions::new().create(true).append(true).open(path))
            .and_then(|mut file| file.write_all(line.as_bytes()));
    }

    fn flush(&self) {}
}

/// Sets up where logs go. If `echo` is true, they are also written to standard error.
pub fn init(directory: &Path, echo: bool) {
    let logger = FileLogger {
        directory: directory.to_owned(),
        echo,
        lock: Mutex::new(()),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}
