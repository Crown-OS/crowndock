use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::desktop::DesktopEntry;
use crate::item::Action;

/// Activate an item by dispatching its [`Action`].
pub fn activate(action: &Action) {
    match action {
        Action::LaunchDesktop(entry) => launch_desktop(entry),
        Action::Command {
            program,
            args,
            terminal,
        } => spawn(program, args, *terminal, program),
    }
}

fn launch_desktop(entry: &DesktopEntry) {
    let argv = entry.launch_argv();
    let Some((program, args)) = argv.split_first() else {
        log::warn!("desktop entry {} has empty Exec", entry.name);
        return;
    };
    log::debug!("launching {}", entry.name);
    spawn(program, args, entry.terminal, &entry.name);
}

fn spawn<S: AsRef<str>>(program: &str, args: &[S], terminal: bool, label: &str) {
    let (program, args): (String, Vec<String>) = if terminal {
        let term = std::env::var("TERMINAL").unwrap_or_else(|_| "xterm".to_string());
        let mut a = vec!["-e".to_string(), program.to_string()];
        a.extend(args.iter().map(|s| s.as_ref().to_string()));
        (term, a)
    } else {
        (
            program.to_string(),
            args.iter().map(|s| s.as_ref().to_string()).collect(),
        )
    };

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Detach from the dock so closing the dock doesn't kill spawned apps.
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    if let Err(err) = cmd.spawn() {
        log::warn!("failed to launch {label}: {err}");
    }
}
