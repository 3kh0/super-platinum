//! A fresh-field preview projection, NOT a canonical File or action capability.
//! Retained Source proof: file-image-fresh-dominance-static-20261031-01.
//! Omitted gates refuse; no empty-store/cold/predecessor assumption is made.
use serde_json::{Value, json};
use std::io::Cursor;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const UNAVAILABLE: &str = "This image is not available to preview.";

pub struct Projection {
    url: String,
    mime: &'static str,
}
impl Projection {
    /// The original runner must still own this singleton response. Parsing is
    /// classification only: IP, custody, events and publication stay separate.
    pub fn from_fresh(raw: &Value, id: &str) -> Result<Self, &'static str> {
        let file = raw
            .get("file")
            .filter(|v| v.is_object())
            .ok_or(UNAVAILABLE)?;
        if raw.get("ok") != Some(&json!(true))
            || raw.get("error").is_some()
            || file.get("id").and_then(Value::as_str) != Some(id)
            || file.get("mode") != Some(&json!("hosted"))
            || file
                .get("is_deleted")
                .is_some_and(|v| !v.is_null() && v != &json!(false))
            || ["isMigratedFromLegacyPost", "docsId"]
                .iter()
                .any(|key| file.get(*key).is_some())
        {
            return Err(UNAVAILABLE);
        }
        // Each is fresh, literal and explicit. Unlike a generic File, absence
        // cannot clear an inherited access/storage/routing restriction.
        for (key, expected) in [
            ("file_access", Value::Null),
            ("is_hidden_by_limit", json!(false)),
            ("is_tombstoned", json!(false)),
            ("restriction_type", json!(0)),
            ("is_external", json!(false)),
            ("external_type", Value::Null),
            ("app_id", Value::Null),
        ] {
            if file.get(key) != Some(&expected) {
                return Err(UNAVAILABLE);
            }
        }
        // Incoming nonexistent fails the Source gate before knownness is
        // computed. Do not use a later timestamp to erase that denial.
        if file
            .get("isNonExistent")
            .is_some_and(|v| v != &json!(false) && !v.is_null())
        {
            return Err(UNAVAILABLE);
        }
        if file
            .get("isUnknown")
            .is_some_and(|v| v != &json!(false) && !v.is_null())
        {
            return Err(UNAVAILABLE);
        }
        let timestamp = ["updated", "created", "timestamp"]
            .into_iter()
            .any(|key| file.get(key).and_then(Value::as_u64).is_some_and(|v| v > 0));
        let known = file.get("isUnknown") == Some(&json!(false))
            && file.get("isNonExistent") == Some(&json!(false));
        if !timestamp && !known {
            return Err(UNAVAILABLE);
        }
        let mime = match (
            file.get("mimetype").and_then(Value::as_str),
            file.get("filetype").and_then(Value::as_str),
        ) {
            (Some("image/png"), Some("png")) => "image/png",
            (Some("image/jpeg"), Some("jpg" | "jpeg")) => "image/jpeg",
            _ => return Err(UNAVAILABLE),
        };
        let url = file
            .get("thumb_360")
            .and_then(Value::as_str)
            .ok_or(UNAVAILABLE)?;
        let parsed = url::Url::parse(url).map_err(|_| UNAVAILABLE)?;
        if parsed.scheme() != "https"
            || parsed.port_or_known_default() != Some(443)
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
            || !parsed
                .host_str()
                .is_some_and(|h| h == "files.slack.com" || h.ends_with(".files.slack.com"))
        {
            return Err(UNAVAILABLE);
        }
        Ok(Self {
            url: parsed.into(),
            mime,
        })
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn mime(&self) -> &'static str {
        self.mime
    }

    /// Decode on a worker, then re-encode an inert static PNG. No HTML/SVG,
    /// animation, EXIF, original URL or action metadata reaches the renderer.
    pub fn rasterize(&self, bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
        if bytes.len() > MAX_BYTES {
            return Err(UNAVAILABLE);
        }
        let format = match self.mime {
            "image/png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => {
                let mut rest = &bytes[8..];
                while !rest.is_empty() {
                    if rest.len() < 12 {
                        return Err(UNAVAILABLE);
                    }
                    let n = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
                    let end = n
                        .checked_add(12)
                        .filter(|n| *n <= rest.len())
                        .ok_or(UNAVAILABLE)?;
                    if &rest[4..8] == b"acTL" {
                        return Err(UNAVAILABLE);
                    }
                    rest = &rest[end..];
                }
                image::ImageFormat::Png
            }
            "image/jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => image::ImageFormat::Jpeg,
            _ => return Err(UNAVAILABLE),
        };
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|_| UNAVAILABLE)?;
        let mut out = Cursor::new(Vec::new());
        image
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|_| UNAVAILABLE)?;
        let bytes = out.into_inner();
        if bytes.len() > MAX_BYTES {
            return Err(UNAVAILABLE);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests;
