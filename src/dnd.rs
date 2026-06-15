//! Drag-and-drop wire-protocol helpers: MIME selection, URI parsing, and the
//! in-flight read buffer used while the compositor streams the drop payload.

use std::path::PathBuf;

use calloop::RegistrationToken;
use smithay_client_toolkit::data_device_manager::data_offer::DragOffer;

pub const URI_LIST_MIME: &str = "text/uri-list";

/// MIME types we know how to consume, in preference order.
const ACCEPTED_MIMES: &[&str] = &[
    URI_LIST_MIME,
    "text/x-uri",
    "text/x-moz-url",
    "application/x-moz-file",
    "text/plain;charset=utf-8",
    "text/plain",
    "STRING",
    "UTF8_STRING",
];

/// Pick the most preferred MIME we support from an offered list.
pub fn pick_uri_mime(offered: &[String]) -> Option<String> {
    ACCEPTED_MIMES
        .iter()
        .find_map(|want| offered.iter().find(|m| m.as_str() == *want).cloned())
}

/// An in-progress drop being streamed from the compositor.
pub struct PendingDrop {
    pub offer: DragOffer,
    pub buffer: Vec<u8>,
    pub token: Option<RegistrationToken>,
}

impl PendingDrop {
    pub fn new(offer: DragOffer) -> Self {
        Self {
            offer,
            buffer: Vec::new(),
            token: None,
        }
    }
}

/// Iterator over parsed `.desktop` paths from a `text/uri-list` payload. Lines
/// that aren't `file://` URIs to existing `.desktop` files are silently skipped.
pub fn parse_uri_list(data: &[u8]) -> impl Iterator<Item = PathBuf> + '_ {
    let text = std::str::from_utf8(data).unwrap_or("");
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(parse_uri)
}

/// Parse a single `file://` URI or absolute path. Returns `None` for any other
/// scheme.
pub fn parse_uri(raw: &str) -> Option<PathBuf> {
    if let Some(rest) = raw.strip_prefix("file://") {
        let path_part = match rest.find('/') {
            Some(0) => rest,
            Some(idx) => &rest[idx..],
            None => return None,
        };
        Some(PathBuf::from(percent_decode(path_part)))
    } else if raw.starts_with('/') {
        Some(PathBuf::from(raw))
    } else {
        None
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h << 4) | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_file_uri() {
        assert_eq!(
            parse_uri("file:///tmp/x.desktop"),
            Some(PathBuf::from("/tmp/x.desktop"))
        );
    }

    #[test]
    fn parses_percent_encoded() {
        assert_eq!(
            parse_uri("file:///tmp/My%20App.desktop"),
            Some(PathBuf::from("/tmp/My App.desktop"))
        );
    }

    #[test]
    fn parses_bare_path() {
        assert_eq!(parse_uri("/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
    }

    #[test]
    fn rejects_https() {
        assert_eq!(parse_uri("https://example.com/x.desktop"), None);
    }

    #[test]
    fn skips_comments_and_blanks() {
        let data = b"# comment\n\nfile:///a.desktop\nfile:///b.desktop\n";
        let paths: Vec<_> = parse_uri_list(data).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("/a.desktop"), PathBuf::from("/b.desktop")]
        );
    }

    #[test]
    fn picks_uri_list_over_text() {
        let offered = vec!["text/plain".into(), "text/uri-list".into()];
        assert_eq!(pick_uri_mime(&offered).as_deref(), Some("text/uri-list"));
    }
}
