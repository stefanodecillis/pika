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

fn main() -> anyhow::Result<()> {
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
