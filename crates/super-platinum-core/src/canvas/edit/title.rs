//! Static title metadata comes from the canonical title section, not Document.69.
//! Q extractTitleMetadata/IRZY emit this metadata even when only a checkbox changed.
use scraper::Html;

use super::{CanvasCollabSnapshot, Error, F, RawMessage, Section};

const MAX_TITLE_SOURCE_BYTES: usize = 64 * 1024;
const RICH_TITLE_UNITS: usize = 2000;
const FORM_TITLE_UNITS: usize = 255;

pub(super) struct CanonicalTitle {
    file: String,
    document: String,
    thread: String,
    section: Section,
    form: String,
    rich: RawMessage,
}
impl CanonicalTitle {
    pub(super) fn capture(snapshot: &CanvasCollabSnapshot) -> Result<Self, Error> {
        let mut title = None;
        for section in &snapshot.sections {
            if section.deleted()? == Some(true) {
                continue;
            }
            if section.section_type()? == Some(64) || section.style()? == Some(48) {
                if title.is_some()
                    || section.section_type()? != Some(64)
                    || section.style()? != Some(48)
                {
                    return Err(Error::Unsupported);
                }
                title = Some(section);
            }
        }
        let section = title.ok_or(Error::Unsupported)?;
        if section.document_id()? != Some(snapshot.document_id())
            || section.thread_id()? != Some(snapshot.thread_id())
            || section
                .sequence()?
                .is_none_or(|s| s == 0 || s > i64::MAX as u64)
        {
            return Err(Error::Unsupported);
        }
        let (form, rich) = Self::metadata(section)?;
        Ok(Self {
            file: snapshot.file_id().to_owned(),
            document: snapshot.document_id().to_owned(),
            thread: snapshot.thread_id().to_owned(),
            section: section.clone(),
            form,
            rich,
        })
    }

    /// Pure title projection for inert recovery validation; no snapshot, actor
    /// or write authority is reconstructed.
    pub(super) fn rich_from_section(section: &Section) -> Result<RawMessage, Error> {
        Self::metadata(section).map(|(_, rich)| rich)
    }
    fn metadata(section: &Section) -> Result<(String, RawMessage), Error> {
        if section.section_type()? != Some(64) || section.style()? != Some(48) {
            return Err(Error::Unsupported);
        }
        let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
        // A title can also carry the legacy Content.text sibling; Content.title
        // is authoritative for TITLE_TYPE. Neither sibling is rewritten.
        if content.fields().iter().any(|f| !matches!(f.tag(), 1 | 58)) {
            return Err(Error::Unsupported);
        }
        let title = content.message(58)?.ok_or(Error::Unsupported)?;
        if title.fields().iter().any(|f| !matches!(f.tag(), 1..=4)) {
            return Err(Error::Unsupported);
        }
        title.message(2)?; // header image does not become title text
        title.string(3)?;
        title.messages(4)?;
        let source = title.string(1)?.ok_or(Error::Unsupported)?;
        let normalized = normalize_static_title(source)?;
        let trimmed = normalized.trim_matches(js_trim_space);
        if trimmed.is_empty() {
            // Stock uses the workspace-localized Untitled label. Do not invent it.
            return Err(Error::Unsupported);
        }
        let form = truncate_utf16(trimmed, FORM_TITLE_UNITS)?;
        let rich_text = truncate_utf16(&normalized, RICH_TITLE_UNITS)?;
        let leaf =
            RawMessage::from_fields(&[F::bytes(1, b"text")?, F::bytes(2, rich_text.as_bytes())?])?;
        let group =
            RawMessage::from_fields(&[F::bytes(1, b"rich_text_section")?, F::message(2, &leaf)?])?;
        let rich = RawMessage::from_fields(&[F::bytes(1, b"rich_text")?, F::message(2, &group)?])?;
        Ok((form, rich))
    }

    pub(super) fn revalidate(&self, fresh: &CanvasCollabSnapshot) -> Result<(), Error> {
        let current = Self::capture(fresh)?;
        if self.file != current.file
            || self.document != current.document
            || self.thread != current.thread
            || self.section != current.section
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
    pub(super) fn form_title(&self) -> &str {
        &self.form
    }
    pub(super) fn rich_text(&self) -> &RawMessage {
        &self.rich
    }
}

fn normalize_static_title(source: &str) -> Result<String, Error> {
    if source.len() > MAX_TITLE_SOURCE_BYTES {
        return Err(Error::Unsupported);
    }
    let markup = source.contains(['<', '>', '&']);
    if markup {
        validate_static_markup(source)?;
    }
    // TITLE_TYPE removes ordinary formatting wrappers (8v0o.X0) before export.
    // Decode once: the exporter escapes &<> and IRZY reparses them, then emits
    // literal decoded text, not HTML escapes or the original formatting styles.
    // Retain the accepted plain path's HTML5 CRLF/null preprocessing behavior.
    let parsed = Html::parse_fragment(source);
    if markup && !parsed.errors.is_empty() {
        return Err(Error::Unsupported);
    }
    let text = parsed.root_element().text().collect::<String>();
    let mut normalized = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{00a0}' | '\t' | '\n' | '\r' => normalized.push(' '),
            '\u{200b}' | '\u{feff}' | '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}' => {}
            _ => normalized.push(c),
        }
    }
    Ok(normalized)
}

// A lexical admission check prevents HTML repair from silently accepting
// unmatched formatting, attributes, semantic nodes or unknown source bytes.
// This is intentionally narrower than the original general title renderer.
fn validate_static_markup(mut source: &str) -> Result<(), Error> {
    let mut tags = Vec::new();
    while let Some(at) = source.find(['<', '&']) {
        source = &source[at..];
        if source.starts_with('<') {
            let end = source.find('>').ok_or(Error::Unsupported)?;
            let tag = &source[1..end];
            let name = tag.strip_prefix('/').unwrap_or(tag);
            if !matches!(name, "b" | "i" | "u" | "del" | "code") {
                return Err(Error::Unsupported);
            }
            if tag.starts_with('/') {
                if tags.pop() != Some(name) {
                    return Err(Error::Unsupported);
                }
            } else {
                if tags.len() >= 32 {
                    return Err(Error::Unsupported);
                }
                tags.push(name);
            }
            source = &source[end + 1..];
        } else {
            let end = source.find(';').ok_or(Error::Unsupported)?;
            let entity = &source[1..end];
            let numeric = entity.strip_prefix('#').is_some_and(|digits| {
                let (digits, radix) = digits
                    .strip_prefix(['x', 'X'])
                    .map_or((digits, 10), |hex| (hex, 16));
                !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix))
            });
            if !matches!(entity, "amp" | "lt" | "gt" | "quot" | "nbsp") && !numeric {
                return Err(Error::Unsupported);
            }
            // The HTML parser below validates numeric scalar/range semantics.
            source = &source[end + 1..];
        }
    }
    if !tags.is_empty() {
        return Err(Error::Unsupported);
    }
    Ok(())
}

fn js_trim_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn truncate_utf16(text: &str, limit: usize) -> Result<String, Error> {
    let mut units = 0;
    for (offset, c) in text.char_indices() {
        if units == limit {
            return Ok(text[..offset].to_owned());
        }
        units += c.len_utf16();
        if units > limit {
            // JS slice can produce a lone surrogate. Its protobuf encoder's
            // treatment is not proven; fail closed rather than change the title.
            return Err(Error::Unsupported);
        }
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests;
