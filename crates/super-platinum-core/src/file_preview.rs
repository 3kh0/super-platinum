//! Bounded, inert text previews. Original bytes remain available for download.
pub const MAX_PREVIEW_BYTES: usize = 256 * 1024;
pub const MAX_PREVIEW_LINES: usize = 2_000;

pub fn is_text(mime: &str) -> bool {
    let mime = mime.split(';').next().unwrap_or(mime).trim();
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-javascript"
                | "application/yaml"
                | "application/x-yaml"
                | "application/toml"
                | "application/sql"
        )
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
}

/// Slack may label a freshly uploaded plain-text file as generic binary. Only
/// known text extensions opt into decoding; the byte decoder still rejects binary.
pub fn mime_for_file(file: &crate::slack::models::File) -> &str {
    let mime = file
        .mimetype
        .as_deref()
        .unwrap_or("application/octet-stream");
    if matches!(mime, "" | "application/octet-stream") {
        let extension = file
            .name
            .as_deref()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, ext)| ext.to_ascii_lowercase());
        if extension.as_deref() == Some("pdf") {
            return "application/pdf";
        }
        if extension.as_deref().is_some_and(|ext| {
            matches!(
                ext,
                "txt"
                    | "log"
                    | "md"
                    | "markdown"
                    | "rs"
                    | "py"
                    | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "json"
                    | "jsonl"
                    | "xml"
                    | "html"
                    | "htm"
                    | "css"
                    | "scss"
                    | "yaml"
                    | "yml"
                    | "toml"
                    | "ini"
                    | "cfg"
                    | "conf"
                    | "csv"
                    | "tsv"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "sql"
                    | "rb"
                    | "go"
                    | "c"
                    | "h"
                    | "cpp"
                    | "hpp"
                    | "java"
                    | "kt"
                    | "swift"
                    | "diff"
                    | "patch"
            )
        }) {
            return "text/plain";
        }
    }
    mime
}

#[derive(Debug, PartialEq, Eq)]
pub struct TextPreview {
    pub text: String,
    pub truncated: bool,
    pub lines: usize,
}

pub fn decode(bytes: &[u8]) -> Result<TextPreview, &'static str> {
    let end = bytes.len().min(MAX_PREVIEW_BYTES);
    let mut truncated = end < bytes.len();
    let prefix = &bytes[..end];
    let text = if prefix.starts_with(&[0xff, 0xfe]) || prefix.starts_with(&[0xfe, 0xff]) {
        let little = prefix[0] == 0xff;
        let mut units = prefix[2..]
            .chunks_exact(2)
            .map(|p| {
                if little {
                    u16::from_le_bytes([p[0], p[1]])
                } else {
                    u16::from_be_bytes([p[0], p[1]])
                }
            })
            .collect::<Vec<_>>();
        if !truncated && (prefix.len() - 2) % 2 != 0 {
            return Err("This text encoding could not be read.");
        }
        if truncated && units.last().is_some_and(|u| (0xd800..=0xdbff).contains(u)) {
            units.pop();
        }
        String::from_utf16(&units).map_err(|_| "This text encoding could not be read.")?
    } else {
        let prefix = prefix.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(prefix);
        let valid = match std::str::from_utf8(prefix) {
            Ok(s) => s,
            Err(e) if truncated && e.error_len().is_none() => {
                std::str::from_utf8(&prefix[..e.valid_up_to()]).unwrap()
            }
            Err(_) => return Err("This text encoding could not be read."),
        };
        valid.to_owned()
    };
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))
    {
        return Err("This file contains binary data and cannot be shown as text.");
    }
    let mut text = text.replace("\r\n", "\n").replace('\r', "\n");
    if let Some((cut, _)) = text.match_indices('\n').nth(MAX_PREVIEW_LINES - 1) {
        if cut + 1 < text.len() {
            text.truncate(cut);
            truncated = true;
        }
    }
    let lines = if text.is_empty() {
        0
    } else {
        text.lines().count()
    };
    Ok(TextPreview {
        text,
        truncated,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_stays_literal_and_unicode_encodings_are_preserved() {
        let content = "<script>alert('x')</script>\r\n🦊 日本語";
        assert_eq!(
            decode(content.as_bytes()).unwrap().text,
            content.replace("\r\n", "\n")
        );
        for little in [true, false] {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(decode(&bytes).unwrap().text, content.replace("\r\n", "\n"));
        }
        assert!(decode(b"hello\0binary").is_err());
        assert!(decode(&[0xff, 0xfe, 0]).is_err());
        assert!(decode(&[0xff, 0x00]).is_err());
        assert_eq!(decode(b"").unwrap().lines, 0);
    }
    #[test]
    fn generic_mime_uses_only_known_text_filenames() {
        let mut file = crate::slack::models::File {
            name: Some("notes.TXT".into()),
            mimetype: Some("application/octet-stream".into()),
            ..Default::default()
        };
        assert_eq!(mime_for_file(&file), "text/plain");
        file.name = Some("brief.PDF".into());
        assert_eq!(mime_for_file(&file), "application/pdf");
        file.name = Some("archive.zip".into());
        assert_eq!(mime_for_file(&file), "application/octet-stream");
        file.name = Some("notes.txt".into());
        file.mimetype = Some("image/png".into());
        assert_eq!(mime_for_file(&file), "image/png");
    }
    #[test]
    fn preview_limits_do_not_split_unicode_or_render_unbounded_lines() {
        let mut text = "a".repeat(MAX_PREVIEW_BYTES - 1);
        text.push('🦊');
        let preview = decode(text.as_bytes()).unwrap();
        assert!(preview.truncated);
        assert_eq!(preview.text.len(), MAX_PREVIEW_BYTES - 1);
        let preview = decode("line\n".repeat(3_000).as_bytes()).unwrap();
        assert!(preview.truncated);
        assert_eq!(preview.lines, MAX_PREVIEW_LINES);
        assert!(is_text("text/html; charset=utf-8"));
        assert!(is_text("application/problem+json"));
        assert!(!is_text("application/pdf"));
        assert!(!is_text("application/octet-stream"));
    }
}
