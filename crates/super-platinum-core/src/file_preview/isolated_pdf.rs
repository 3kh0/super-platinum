//! New, no-predecessor PDF content data; never a File or preview-ready receipt.
//! Source: isolated-search-pdf-source-contract-static-20261101-01. Original
//! occurrence, policy/IP, known denies, events, bytes and decoded publication
//! remain separately owned by the native caller.
use serde_json::Value;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Defensive native limit, checked against actual SDK decode by the caller.
pub const MAX_PAGES: usize = 200;
const UNAVAILABLE: &str = "This PDF is not available to preview.";

pub struct Projection {
    url: String,
}
impl Projection {
    pub fn from_fresh(raw: &Value, id: &str) -> Result<Self, &'static str> {
        let file = raw
            .get("file")
            .filter(|v| v.is_object())
            .ok_or(UNAVAILABLE)?;
        if !valid_id(id)
            || raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some()
            || raw.get("files").is_some()
            || file.get("id").and_then(Value::as_str) != Some(id)
            || file.get("mode").and_then(Value::as_str) != Some("hosted")
            || file.get("filetype").and_then(Value::as_str) != Some("pdf")
            || file.get("mimetype").and_then(Value::as_str) != Some("application/pdf")
            || file.get("is_external") != Some(&Value::Bool(false))
            || file.get("is_hidden_by_limit") != Some(&Value::Bool(false))
            || file.get("restriction_type").and_then(Value::as_u64) != Some(0)
            || !matches!(file.get("file_access"), Some(Value::Null))
                && file.get("file_access").and_then(Value::as_str) != Some("visible")
            || ["isMigratedFromLegacyPost", "docsId"]
                .iter()
                .any(|k| file.get(*k).is_some())
        {
            return Err(UNAVAILABLE);
        }
        // Ordinary20Mt success constructs is_deleted:false. Fresh tombstone
        // normalization uses !!incoming.is_tombstoned. Missing/null/false can
        // cover those exact new-object results; no prior deny is cleared here.
        for key in [
            "is_deleted",
            "is_tombstoned",
            "is_tombstone",
            "isUnknown",
            "isNonExistent",
            "deleted",
            "hidden",
            "is_hidden",
            "is_dlp_tombstoned",
            "is_dsa_restricted",
        ] {
            if file
                .get(key)
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
            {
                return Err(UNAVAILABLE);
            }
        }
        // These are excluded routing/content alternatives, not missing-policy
        // defaults. This projection is new and never merged over a warm File.
        for key in ["converted_pdf", "app_id", "external_id", "external_type"] {
            if file
                .get(key)
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
            {
                return Err(UNAVAILABLE);
            }
        }
        let mut timestamp = false;
        for key in ["updated", "created", "timestamp"] {
            if let Some(value) = file.get(key).filter(|v| !v.is_null()) {
                timestamp |= value.as_u64().ok_or(UNAVAILABLE)? > 0;
            }
        }
        let known = file.get("isUnknown") == Some(&Value::Bool(false))
            && file.get("isNonExistent") == Some(&Value::Bool(false));
        if !timestamp && !known {
            return Err(UNAVAILABLE);
        }
        let url = file
            .get("url_private_download")
            .and_then(Value::as_str)
            .ok_or(UNAVAILABLE)?;
        if !valid_content_url(url) {
            return Err(UNAVAILABLE);
        }
        Ok(Self { url: url.into() })
    }
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Size/header/end-marker precheck only. This deliberately does NOT parse
    /// objects, count pages, validate xrefs, decode PDF, or make bytes ready.
    /// The native decoder must still enforce MAX_PAGES and its other bounds.
    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<(), &'static str> {
        if bytes.len() > MAX_BYTES || bytes.len() < 16 {
            return Err(UNAVAILABLE);
        }
        let version = &bytes[..8];
        let supported = version.starts_with(b"%PDF-1.") && (b'0'..=b'7').contains(&version[7])
            || version == b"%PDF-2.0";
        if !supported || !matches!(bytes[8], b'\r' | b'\n') {
            return Err(UNAVAILABLE);
        }
        let end = bytes
            .iter()
            .rposition(|&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c))
            .map(|i| i + 1)
            .ok_or(UNAVAILABLE)?;
        if !bytes[..end].ends_with(b"%%EOF") {
            return Err(UNAVAILABLE);
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    (9..=15).contains(&id.len())
        && id.starts_with('F')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
pub(crate) fn valid_content_url(input: &str) -> bool {
    if input.is_empty()
        || input.len() > 16 * 1024
        || !input.is_ascii()
        || input.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let Ok(url) = url::Url::parse(input) else {
        return false;
    };
    let allowed = url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url
            .host_str()
            .is_some_and(|h| h == "files.slack.com" || h.ends_with(".files.slack.com"));
    // Core protocol corpus only. No production/dependency/native build gets
    // a loopback projection or an alternate host from this branch.
    #[cfg(test)]
    let allowed = allowed || url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    allowed
        && url.as_str() == input
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

#[cfg(test)]
mod tests;
