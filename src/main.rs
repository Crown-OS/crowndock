mod cli;
mod config;
mod desktop;
mod dnd;
mod dock;
mod error;
mod icon;
mod item;
mod launcher;
mod render;
mod source;
mod theme;
mod watcher;
mod wayland;

use crate::error::Result;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    if let Some(exit) = cli::dispatch() {
        std::process::exit(exit);
    }

    let _ = config::ensure_dirs();
    let cfg = config::Config::load();
    wayland::run(cfg)
}
