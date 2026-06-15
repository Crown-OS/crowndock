//! CLI subcommand dispatch. Returns `Some(exit_code)` if a subcommand handled
//! the run, `None` to fall through to the dock GUI.

use std::fs;
use std::path::PathBuf;

use crate::config;

pub fn dispatch() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next()?;
    match cmd.as_str() {
        "pin" => Some(pin(args.next())),
        "unpin" => Some(unpin(args.next())),
        "list" => Some(list()),
        "help" | "-h" | "--help" => {
            print_help();
            Some(0)
        }
        _ => None,
    }
}

fn print_help() {
    println!(
        "crowndock — a Wayland dock\n\n\
         USAGE:\n  \
           crowndock [OUTPUT]               Run the dock (optionally on a named output)\n  \
           crowndock pin <path/to.desktop>  Copy a .desktop file into the pinned folder\n  \
           crowndock unpin <name>           Remove <name>.desktop from the pinned folder\n  \
           crowndock list                   List currently pinned applications\n  \
           crowndock help                   Show this message\n"
    );
}

fn pin(path: Option<String>) -> i32 {
    let Some(src) = path.map(PathBuf::from) else {
        eprintln!("crowndock pin: expected a path to a .desktop file");
        return 2;
    };
    if !src.exists() {
        eprintln!("crowndock pin: {} does not exist", src.display());
        return 1;
    }
    if src.extension().and_then(|e| e.to_str()) != Some("desktop") {
        eprintln!("crowndock pin: {} is not a .desktop file", src.display());
        return 1;
    }

    if let Err(err) = config::ensure_dirs() {
        eprintln!("crowndock pin: failed to prepare config dirs: {err}");
        return 1;
    }
    let Some(file_name) = src.file_name() else {
        eprintln!("crowndock pin: {} has no file name", src.display());
        return 1;
    };
    let dest = config::pinned_dir().join(file_name);
    if let Err(err) = fs::copy(&src, &dest) {
        eprintln!(
            "crowndock pin: copy {} -> {} failed: {err}",
            src.display(),
            dest.display()
        );
        return 1;
    }
    println!("pinned {}", dest.display());
    0
}

fn unpin(name: Option<String>) -> i32 {
    let Some(mut file) = name else {
        eprintln!("crowndock unpin: expected a .desktop file name");
        return 2;
    };
    if !file.ends_with(".desktop") {
        file.push_str(".desktop");
    }
    let target = config::pinned_dir().join(&file);
    if !target.exists() {
        eprintln!("crowndock unpin: {} is not pinned", target.display());
        return 1;
    }
    if let Err(err) = fs::remove_file(&target) {
        eprintln!("crowndock unpin: remove {} failed: {err}", target.display());
        return 1;
    }
    println!("unpinned {}", target.display());
    0
}

fn list() -> i32 {
    let dir = config::pinned_dir();
    let Ok(read) = fs::read_dir(&dir) else {
        eprintln!("crowndock list: cannot read {}", dir.display());
        return 1;
    };
    let mut any = false;
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
            println!("{}", path.display());
            any = true;
        }
    }
    if !any {
        println!("(no pinned apps)");
    }
    0
}
