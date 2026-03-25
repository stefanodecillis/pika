mod app;
mod config;
mod editor;
mod events;
mod files;
mod input;
mod lsp;
mod ui;

use std::env;
use std::path::PathBuf;

fn setup_logging() {
    use std::io::Write;
    let log_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pika-lsp.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
    if let Some(file) = file {
        let file = std::sync::Mutex::new(file);
        let logger = Box::leak(Box::new(FileLogger(file)));
        log::set_logger(logger)
            .map(|()| log::set_max_level(log::LevelFilter::Debug))
            .ok();
    }
}

struct FileLogger(std::sync::Mutex<std::fs::File>);

impl log::Log for FileLogger {
    fn enabled(&self, _: &log::Metadata) -> bool { true }
    fn flush(&self) {}
    fn log(&self, record: &log::Record) {
        use std::io::Write;
        if let Ok(mut f) = self.0.lock() {
            let _ = writeln!(f, "[{}] {}: {}", record.level(), record.target(), record.args());
        }
    }
}

fn main() -> anyhow::Result<()> {
    setup_logging();

    // Create and enter a tokio runtime so that async LSP tasks can be spawned
    // from synchronous code throughout the application lifetime.
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    // Determine the root directory: first arg or current directory
    let root_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let root_dir = root_dir.canonicalize().unwrap_or(root_dir);

    let mut app = app::App::new(root_dir)?;
    app.run()?;

    Ok(())
}
