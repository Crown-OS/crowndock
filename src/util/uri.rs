//! `text/uri-list` (RFC 2483) of `file://` URIs: what file managers, launchers and the dock
//! itself put on a drag.

use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

pub const URI_LIST: &str = "text/uri-list";
const FILE_SCHEME: &str = "file://";
const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

/// The local paths in a URI list, skipping comments and non-`file` URIs.
pub fn file_paths(uri_list: &str) -> impl Iterator<Item = PathBuf> + '_ {
    uri_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(uri_to_path)
}

/// `file:///a%20b` or `file://localhost/a%20b` to `/a b`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix(FILE_SCHEME)?;
    let path = &rest[rest.find('/')?..];
    Some(PathBuf::from(OsString::from_vec(percent_decode(path))))
}

/// A one-entry URI list naming `path`.
pub fn uri_list(path: &Path) -> Vec<u8> {
    let bytes = path.as_os_str().as_bytes();
    let mut list = Vec::with_capacity(FILE_SCHEME.len() + bytes.len() + 2);
    list.extend_from_slice(FILE_SCHEME.as_bytes());
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            list.push(byte);
        } else {
            list.extend_from_slice(&[
                b'%',
                HEX_DIGITS[usize::from(byte >> 4)],
                HEX_DIGITS[usize::from(byte & 0xF)],
            ]);
        }
    }
    list.extend_from_slice(b"\r\n");
    list
}

fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| {
                Some((hex_value(*bytes.get(index + 1)?)? << 4) | hex_value(*bytes.get(index + 2)?)?)
            })
            .flatten();
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    decoded
}

fn hex_value(digit: u8) -> Option<u8> {
    char::from(digit).to_digit(16).map(|value| value as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_round_trip_through_a_uri_list() {
        let path = Path::new("/usr/share/applications/my app%.desktop");
        let list = uri_list(path);
        let text = String::from_utf8_lossy(&list);
        assert_eq!(
            text,
            "file:///usr/share/applications/my%20app%25.desktop\r\n"
        );
        assert_eq!(file_paths(&text).collect::<Vec<_>>(), [path]);
    }

    #[test]
    fn comments_hosts_and_other_schemes_are_handled() {
        let list = "# from crownpad\nfile://localhost/a.desktop\nhttps://example.com/x\n";
        assert_eq!(
            file_paths(list).collect::<Vec<_>>(),
            [Path::new("/a.desktop")]
        );
    }

    #[test]
    fn a_stray_percent_is_kept() {
        assert_eq!(uri_to_path("file:///50%"), Some(PathBuf::from("/50%")));
    }
}
