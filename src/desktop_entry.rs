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
}

impl DesktopEntry {
    pub fn read(path: &Path) -> Result<Self> {
        let entry = freedesktop_entry_parser::parse_entry(path)
            .with_context(|| format!("read {}", path.display()))?;
        let section = entry.section(SECTION);
        Ok(Self {
            icon: section.attr("Icon").map(str::to_owned),
            exec: section.attr("Exec").map(str::to_owned),
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
    fn an_exec_of_only_field_codes_runs_nothing() {
        assert_eq!(command("%F"), None);
        assert_eq!(command(r#""""#), Some(vec![String::new()]));
    }
}
