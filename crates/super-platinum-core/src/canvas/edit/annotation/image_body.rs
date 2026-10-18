//! Exact ordinary ImageContent metadata plus a renderer-observed export branch.
//! Export evidence is inert. It contains no pixels, URL, request, or replay key.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The original native ImageContent export has four observable outcomes. The
/// three empty outcomes produce the same snippet but retain distinct evidence.
/// A caller must observe its original loaded image; a placeholder is not one.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageExport {
    Exported,
    NoImage,
    EmptyData,
    ExportError,
}

pub(super) struct ImageBody {
    pub(super) alt: String,
    file: Option<String>,
}

/// Read-only original-image renderer mapping. Matching a source URL permits
/// only local native observation, never an annotation request or file fetch.
pub struct ImageAnnotationSeed {
    section: String,
    alt: String,
    resource_prefix: String,
    thread: String,
    hashes: Vec<String>,
    files: Vec<String>,
}
impl ImageAnnotationSeed {
    pub fn section(&self) -> &str {
        &self.section
    }
    pub fn alt(&self) -> &str {
        &self.alt
    }
    pub fn has_image_source(&self) -> bool {
        !self.hashes.is_empty() || !self.files.is_empty()
    }
    pub fn matches_rendered_url(&self, source: &str) -> bool {
        // Exact retained serverResourcePrefix + /-/blob path. Do not treat an
        // unrelated URL with the same alt text/section marker as this image.
        let Ok(url) = url::Url::parse(source) else {
            return false;
        };
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return false;
        }
        let mut seen = std::collections::BTreeSet::new();
        if url.query_pairs().any(|(key, _)| {
            !matches!(key.as_ref(), "s" | "name" | "oauth_token" | "size_name")
                || !seen.insert(key.into_owned())
        }) {
            return false;
        }
        let mut without_query = url.clone();
        without_query.set_query(None);
        self.hashes.iter().chain(&self.files).any(|identity| {
            // blobUrl chooses the path from the client environment, not the
            // identifier's field. Both paths retain the same canonical value.
            ["blob", "collab-slack-blob"].iter().any(|path| {
                without_query.as_str()
                    == format!(
                        "{}/-/{path}/{}/{identity}",
                        self.resource_prefix, self.thread
                    )
            })
        })
    }
}

pub fn image_seed(snapshot: &CanvasCollabSnapshot, id: &str) -> Result<ImageAnnotationSeed, Error> {
    comment_permission(snapshot)?;
    let section = snapshot.section(id).ok_or(Error::Changed)?;
    if section.section_type()? != Some(2) {
        return Err(Error::Unsupported);
    }
    super::admitted_section_shape(snapshot, section)?;
    let body = ImageBody::metadata(section)?;
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    let image = content.message(2)?.ok_or(Error::Unsupported)?;
    let prefixes = snapshot
        .init
        .options()
        .message(8)?
        .ok_or(Error::Unsupported)?;
    let prefix = prefixes.string(2)?.ok_or(Error::Unsupported)?;
    let parsed = url::Url::parse(prefix).map_err(|_| Error::Unsupported)?;
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || prefix.ends_with('/')
    {
        return Err(Error::Unsupported);
    }
    let mut originals = image.message(3)?.into_iter().collect::<Vec<_>>();
    originals.extend(image.messages(4)?);
    originals.extend(image.messages(5)?);
    let mut hashes = Vec::new();
    let mut files = Vec::new();
    for original in originals {
        for (tag, out) in [(1, &mut hashes), (5, &mut files)] {
            if let Some(value) = original.string(tag)?.filter(|s| !s.is_empty()) {
                if value.len() > 256
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                {
                    return Err(Error::Unsupported);
                }
                if !out.iter().any(|s| s == value) {
                    out.push(value.to_owned());
                }
            }
        }
    }
    Ok(ImageAnnotationSeed {
        section: id.into(),
        alt: body.alt,
        resource_prefix: prefix.into(),
        thread: snapshot.thread_id().into(),
        hashes,
        files,
    })
}

/// Shape of the loaded ordinary section, also used for inert sibling topology.
/// Deleted/read-only siblings still occupy positions; authoring checks those
/// properties separately in shape(). No caption/body/unknown field is rewritten.
pub(super) fn inventory_shape(section: &Section) -> Result<(), Error> {
    inventory_shape_with_layout(section, false)
}
pub(super) fn inventory_shape_with_layout(section: &Section, layout: bool) -> Result<(), Error> {
    if section.section_type()? != Some(2)
        || section.style()? != Some(11)
        || section.raw().uint(11)?.unwrap_or(0) != 0
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || (!layout && section.raw().boolean(29)? == Some(true))
        || section.raw().boolean(36)? == Some(true)
        || section.raw().has(15)
        || section.crdt()?.is_some()
        || section
            .sequence()?
            .is_none_or(|n| n == 0 || n > i64::MAX as u64)
    {
        return Err(Error::Unsupported);
    }
    let position = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
    if section.raw().string(21)?.is_some_and(|p| p != position) {
        return Err(Error::Unsupported);
    }
    ImageBody::metadata(section).map(|_| ())
}

pub(super) fn shape(section: &Section) -> Result<(), Error> {
    shape_with_layout(section, false)
}
pub(super) fn shape_with_layout(section: &Section, layout: bool) -> Result<(), Error> {
    inventory_shape_with_layout(section, layout)?;
    if section.deleted()? == Some(true) || section.read_only()? == Some(true) {
        return Err(Error::Unsupported);
    }
    Ok(())
}

impl ImageBody {
    pub(super) fn metadata(section: &Section) -> Result<Self, Error> {
        let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
        if content.fields().iter().any(|f| f.tag() != 2) {
            return Err(Error::Unsupported);
        }
        let image = content.message(2)?.ok_or(Error::Unsupported)?;
        // Actual altTextOrFilename: decoration beats description; a present
        // empty description must not fall through to filename.
        let alt = if image.boolean(16)? == Some(true) {
            ""
        } else {
            image.string(15)?.or(image.string(7)?).unwrap_or("")
        };
        let file = effective_original(&image)?
            .map(|p| p.string(5).map(|s| s.map(str::to_owned)))
            .transpose()?
            .flatten()
            .filter(|s| !s.is_empty());
        if alt.len() > 16_384
            || file
                .as_ref()
                .is_some_and(|id| id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(Error::Unsupported);
        }
        Ok(Self {
            alt: alt.into(),
            file,
        })
    }

    pub(super) fn rich(&self, export: ImageExport) -> Value {
        let elements = if export == ImageExport::Exported {
            // Actual qa+r.htmlEscape escapes only &, < and >. ImageContent
            // places it inside a single-quoted attribute, then IRZY reads the
            // HTML-parsed alt. Preserve that boundary (CR/NUL normalization
            // and apostrophes included) without changing the canonical value.
            let escaped = self
                .alt
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let parsed = scraper::Html::parse_fragment(&format!(
                "<img src='data:image/png;base64,' alt='{escaped}'>"
            ));
            let selector = scraper::Selector::parse("img").expect("literal image selector");
            let alt = parsed
                .select(&selector)
                .next()
                .and_then(|n| n.value().attr("alt"))
                .unwrap_or("");
            vec![json!({"type":"rich_text_section","elements":[{
                "type":"text","text":if alt.is_empty(){"Image"}else{alt}
            }]})]
        } else {
            Vec::new()
        };
        json!({"type":"rich_text","elements":elements})
    }

    pub(super) fn snippet(
        &self,
        section: &Section,
        control: &str,
        quip: &str,
        slack: &str,
        export: ImageExport,
    ) -> Result<Value, Error> {
        let mut snippet = super::snippet::capture_rendered(
            section,
            control,
            quip,
            slack,
            self.rich(export),
            String::new(),
        )?;
        // Actual yHAx.gL.FILE is numeric 4. Both associations survive an empty
        // native export; they derive from canonical original.slack_file_id.
        snippet["fileIds"] = json!(self.file.iter().collect::<Vec<_>>());
        snippet["objectStubs"] = json!(
            self.file
                .iter()
                .map(|id| json!({"type":4,"id":id}))
                .collect::<Vec<_>>()
        );
        Ok(snippet)
    }
}

// ImageContainerImpl installs the first widest hashed size as its local
// original only when the original is absent or lacks BOTH hash/file fields.
// Derive that value without rewriting any captured canonical bytes.
fn effective_original(image: &RawMessage) -> Result<Option<RawMessage>, Error> {
    let original = image.message(3)?;
    if let Some(value) = &original {
        value.string(1)?;
        value.string(5)?;
        if value.has(1) || value.has(5) {
            return Ok(original);
        }
    }
    let mut sizes = Vec::new();
    for size in image.messages(4)? {
        if size.string(1)?.is_none_or(str::is_empty) {
            continue;
        }
        let width = size.uint(2)?.unwrap_or(0);
        if width > i32::MAX as u64 {
            return Err(Error::Unsupported);
        }
        sizes.push((width, size));
    }
    // The retained source computes max with getWidthOrDefault(), but its
    // first-match lookup uses getWidth(). An absent width is not explicit 0.
    let max = sizes.iter().map(|(width, _)| *width).max();
    for (_, size) in sizes {
        if size.uint(2)? == max {
            return Ok(Some(size));
        }
    }
    Ok(original)
}
