use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::desktop_entry::DesktopEntry;

/// Starts the application `desktop_file` describes, off the UI thread. `setsid --fork` puts it in
/// a session of its own and returns at once, so the app outlives the dock and is never left
/// behind as the dock's zombie.
pub fn launch(desktop_file: PathBuf) {
    let launcher = std::thread::Builder::new()
        .name("crowndock-launch".into())
        .spawn(move || {
            let command = DesktopEntry::read(&desktop_file)
                .map(|entry| entry.command().map(|argv| (argv, entry.gpu)));
            let Ok(Some((argv, gpu))) = command else {
                log::warn!("{} has nothing to run", desktop_file.display());
                return;
            };
            let started = Command::new("setsid")
                .arg("--fork")
                .args(&argv)
                .envs(gpu.environment().iter().map(|(key, value)| (key, value)))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Err(error) = started {
                log::warn!("launch {}: {error}", desktop_file.display());
            }
        });
    if let Err(error) = launcher {
        log::warn!("could not start the launcher thread: {error}");
    }
}
