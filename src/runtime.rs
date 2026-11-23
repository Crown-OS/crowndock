use std::{os::unix::process::CommandExt, process::Stdio};

use anyhow::Result;

pub enum TaskType {
    Launch,
}

pub struct Task {
    task_type: TaskType,
}

pub struct TaskQueue {
    queue: Vec<Task>,
}

impl TaskQueue {
    pub fn queue(&self, closure: Box<dyn Fn() -> Result<()>>) {}

    fn spawn(&self, argv: &[String]) {
        let Some((program, args)) = argv.split_first() else {
            return;
        };

        let mut command = std::process::Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Safety: `setsid` is async-signal-safe and the only call between fork
        // and exec.
        unsafe {
            command.pre_exec(|| {
                let _ = rustix::process::setsid();
                Ok(())
            });
        }

        match command.spawn() {
            Ok(child) => tracing::debug!(program, pid = child.id(), "spawned"),
            Err(err) => tracing::warn!(%err, program, "failed to spawn"),
        }
    }
}
