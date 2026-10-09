use std::path::Path;

use anyhow::{Context, Result};

const SECTION: &str = "Desktop Entry";
/// `Exec=` field codes that stand alone and expand to files, URLs or launcher data the dock
/// never passes.
const FIELD_CODES: [&str; 7] = ["%f", "%F", "%u", "%U", "%i", "%c", "%k"];

/// The parts of a `.desktop` file the dock uses.
pub struct DesktopEntry {
    pub icon: Option<String>,
    exec: Option<String>,
    startup_wm_class: Option<String>,
}

impl DesktopEntry {
    pub fn read(path: &Path) -> Result<Self> {
        let entry = freedesktop_entry_parser::parse_entry(path)
            .with_context(|| format!("read {}", path.display()))?;
        let section = entry.section(SECTION);
        Ok(Self {
            icon: section.attr("Icon").map(str::to_owned),
            exec: section.attr("Exec").map(str::to_owned),
            startup_wm_class: section.attr("StartupWMClass").map(str::to_owned),
        })
    }

    /// The program and its arguments, field codes removed.
    pub fn command(&self) -> Option<Vec<String>> {
        let arguments: Vec<String> = split_exec(self.exec.as_deref()?)
            .into_iter()
            .filter(|argument| !FIELD_CODES.contains(&argument.as_str()))
            .map(|argument| argument.replace("%%", "%"))
            .collect();
        (!arguments.is_empty()).then_some(arguments)
    }
}

/// The app_id the windows of the application `desktop_file` describes are expected to set: its
/// `StartupWMClass`, or else the file's name without `.desktop`.
pub fn app_id(desktop_file: &Path) -> Option<String> {
    DesktopEntry::read(desktop_file)
        .ok()
        .and_then(|entry| entry.startup_wm_class)
        .or_else(|| Some(desktop_file.file_stem()?.to_str()?.to_owned()))
}

/// Splits an `Exec=` value into arguments, honouring double quotes and backslash escapes.
fn split_exec(exec: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut escaped, mut started) = (false, false, false);
    for character in exec.chars() {
        match character {
            _ if escaped => {
                current.push(character);
                escaped = false;
            }
            '\\' => escaped = true,
            '"' => {
                quoted = !quoted;
                started = true;
            }
            space if space.is_whitespace() && !quoted => {
                if started || !current.is_empty() {
                    arguments.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            other => current.push(other),
        }
    }
    if started || !current.is_empty() {
        arguments.push(current);
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(exec: &str) -> Option<Vec<String>> {
        DesktopEntry {
            icon: None,
            exec: Some(exec.to_owned()),
            startup_wm_class: None,
        }
        .command()
    }

    #[test]
    fn field_codes_are_dropped_and_quotes_kept_together() {
        assert_eq!(
            command(r#"env "MY VAR=1" /opt/app\ dir/app --name=%% %U"#),
            Some(vec![
                "env".to_owned(),
                "MY VAR=1".to_owned(),
                "/opt/app dir/app".to_owned(),
                "--name=%".to_owned(),
            ])
        );
    }

    #[test]
    fn the_app_id_is_the_startup_wm_class_when_there_is_one() {
        let directory =
            std::env::temp_dir().join(format!("crowndock-app-id-{}", std::process::id()));
        let classed = directory.join("org.example.Classed.desktop");
        let unclassed = directory.join("org.gnome.Nautilus.desktop");
        let written = std::fs::create_dir_all(&directory)
            .and_then(|()| {
                std::fs::write(
                    &classed,
                    "[Desktop Entry]\nName=Classed\nExec=classed\nStartupWMClass=classed-window\n",
                )
            })
            .and_then(|()| {
                std::fs::write(&unclassed, "[Desktop Entry]\nName=Files\nExec=nautilus\n")
            });
        if let Err(error) = written {
            panic!("write the test entries: {error}");
        }
        let ids = (app_id(&classed), app_id(&unclassed));
        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(ids.0.as_deref(), Some("classed-window"));
        assert_eq!(ids.1.as_deref(), Some("org.gnome.Nautilus"));
    }

    #[test]
    fn an_unreadable_entry_is_known_by_its_file_name() {
        assert_eq!(
            app_id(Path::new("/nonexistent/org.example.Gone.desktop")).as_deref(),
            Some("org.example.Gone")
        );
    }

    #[test]
    fn an_exec_of_only_field_codes_runs_nothing() {
        assert_eq!(command("%F"), None);
        assert_eq!(command(r#""""#), Some(vec![String::new()]));
    }
}
