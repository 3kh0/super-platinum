//! Separate annotation-aware read model. Ordinary TextDraft admission stays closed.
//! Removing wrappers here builds a typed projection only: every original mark is
//! retained separately and must validate against the original CRDT before writing.
use super::{Error, TextDraft};
use crate::canvas::wire::Section;
use automerge::marks::Mark;
use scraper::{ElementRef, Html, Node};

pub(in crate::canvas::edit::annotation) struct Existing {
    pub draft: TextDraft,
    pub marks: Vec<Mark>,
    pub spans: Vec<Span>,
}
#[derive(Clone, PartialEq, Eq)]
pub(in crate::canvas::edit::annotation) struct Span {
    pub id: String,
    pub start: usize,
    pub end: usize,
    pub escaped_start: usize,
    pub escaped_end: usize,
}

pub(super) fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && !id
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '&'))
}

pub(super) fn parse(section: &Section) -> Result<Existing, Error> {
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    if content.fields().iter().any(|f| f.tag() != 1) {
        return Err(Error::Unsupported);
    }
    let text = content.message(1)?.ok_or(Error::Unsupported)?;
    if text.fields().iter().any(|f| !matches!(f.tag(), 1 | 2)) {
        return Err(Error::Unsupported);
    }
    text.string(2)?;
    let original = from_rtml(text.string(1)?.unwrap_or(""))?;
    super::super::underline_shape(section, &original.draft)?;
    Ok(original)
}

pub(super) fn from_rtml(source: &str) -> Result<Existing, Error> {
    if source.len() > 128 * 1024 || source.contains('\0') {
        return Err(Error::Unsupported);
    }
    let document = Html::parse_fragment(source);
    if !document.errors.is_empty() {
        return Err(Error::Unsupported);
    }
    let mut walk = Walk::default();
    walk.children(document.root_element(), 0, false)?;
    let draft = TextDraft::from_rtml(&walk.stripped)?;
    let mut model = draft.model()?;
    if model.text.encode_utf16().count() != walk.escaped
        || draft
            .runs
            .iter()
            .map(|r| r.text.encode_utf16().count())
            .sum::<usize>()
            != walk.visible
    {
        return Err(Error::Unsupported);
    }
    for span in &walk.spans {
        model.marks.push(Mark::new(
            "annotation".into(),
            span.id.clone(),
            span.escaped_start,
            span.escaped_end,
        ));
    }
    super::compact::render(&model.text, &model.marks)?;
    Ok(Existing {
        draft,
        marks: model.marks,
        spans: walk.spans,
    })
}

#[derive(Default)]
struct Walk {
    stripped: String,
    visible: usize,
    escaped: usize,
    nodes: usize,
    spans: Vec<Span>,
    ids: std::collections::HashSet<String>,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
impl Walk {
    fn children(
        &mut self,
        parent: ElementRef<'_>,
        depth: usize,
        inside: bool,
    ) -> Result<(), Error> {
        if depth > 32 {
            return Err(Error::Unsupported);
        }
        for child in parent.children() {
            self.nodes += 1;
            if self.nodes > 16384 {
                return Err(Error::Unsupported);
            }
            match child.value() {
                Node::Text(text) => {
                    // Exact ordinary canonical normalization. Original raw text
                    // still lives in the section and must agree with CRDT.
                    let normalized: String = text
                        .chars()
                        .filter_map(|c| match c {
                            '\t' | '\n' | '\r' => Some(' '),
                            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => None,
                            _ => Some(c),
                        })
                        .collect();
                    self.visible += normalized.encode_utf16().count();
                    self.escaped += escape(&normalized).encode_utf16().count();
                    // A decoded leading FEFF is body text. html5ever's next
                    // tokenizer otherwise treats the literal as an input BOM.
                    // Keep it escaped across this internal reparse, preserving
                    // the exact visible/escaped UTF-16 checks below.
                    self.stripped
                        .push_str(&escape(text).replace('\u{feff}', "&#65279;"));
                }
                Node::Element(element) => {
                    let node = ElementRef::wrap(child).ok_or(Error::Unsupported)?;
                    let name = element.name();
                    if name == "annotation" {
                        let id = element.attr("id").ok_or(Error::Unsupported)?;
                        if inside
                            || element.attrs().count() != 1
                            || !safe_id(id)
                            || self.spans.len() >= 4096
                            || !self.ids.insert(id.into())
                        {
                            return Err(Error::Unsupported);
                        }
                        let start = self.visible;
                        let escaped_start = self.escaped;
                        self.children(node, depth + 1, true)?;
                        if start == self.visible {
                            return Err(Error::Unsupported);
                        }
                        self.spans.push(Span {
                            id: id.into(),
                            start,
                            end: self.visible,
                            escaped_start,
                            escaped_end: self.escaped,
                        });
                    } else if matches!(name, "b" | "i" | "u" | "del" | "code" | "a" | "br") {
                        self.stripped.push('<');
                        self.stripped.push_str(name);
                        if name == "a" {
                            if element.attrs().any(|(key, _)| key != "href") {
                                return Err(Error::Unsupported);
                            }
                            if let Some(href) = element.attr("href") {
                                self.stripped.push_str(" href=\"");
                                self.stripped.push_str(&escape(href).replace('"', "&quot;"));
                                self.stripped.push('"');
                            }
                        } else if element.attrs().next().is_some() {
                            return Err(Error::Unsupported);
                        }
                        self.stripped.push('>');
                        if name == "br" {
                            if node.children().next().is_some() {
                                return Err(Error::Unsupported);
                            }
                            self.visible += 1;
                            self.escaped += 4;
                        } else {
                            self.children(node, depth + 1, inside)?;
                            self.stripped.push_str("</");
                            self.stripped.push_str(name);
                            self.stripped.push('>');
                        }
                    } else {
                        return Err(Error::Unsupported);
                    }
                }
                _ => return Err(Error::Unsupported),
            }
            if self.stripped.len() > 64 * 1024 {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }
}

impl Existing {
    pub(super) fn disjoint(&self, start: usize, end: usize) -> Result<(), Error> {
        if self.spans.iter().any(|s| start < s.end && s.start < end) {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_leading_bom_remains_body_text_through_internal_reparse() {
        let plain = from_rtml("&#65279;<br>").unwrap();
        assert_eq!(plain.draft.model().unwrap().text, "\u{feff}<br>");
        assert_eq!(plain.draft.runs[0].text, "\u{feff}\n");
        assert!(plain.spans.is_empty());
        let marked = from_rtml("<annotation id=\"old\">&#65279;</annotation><br>").unwrap();
        assert_eq!(marked.draft.model().unwrap().text, "\u{feff}<br>");
        assert_eq!(marked.spans.len(), 1);
        let span = &marked.spans[0];
        assert_eq!(
            (span.start, span.end, span.escaped_start, span.escaped_end),
            (0, 1, 0, 1)
        );
        assert!(marked.disjoint(0, 1).is_err());
        marked.disjoint(1, 2).unwrap();
    }

    #[test]
    fn annotated_model_preserves_old_ranges_with_escaped_unicode_and_style() {
        let value =
            from_rtml("<b>A&amp;<annotation id=\"old-control\">\u{1f600}<br></annotation>Z</b>")
                .unwrap();
        assert_eq!(
            value
                .draft
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "A&\u{1f600}\nZ"
        );
        assert_eq!(value.spans.len(), 1);
        let span = &value.spans[0];
        assert_eq!(
            (span.start, span.end, span.escaped_start, span.escaped_end),
            (2, 5, 6, 12)
        );
        assert!(value.draft.runs[0].style.bold);
        assert!(value.disjoint(0, 2).is_ok());
        assert!(value.disjoint(5, 6).is_ok());
        for (a, b) in [(2, 3), (1, 4), (0, 6)] {
            assert!(value.disjoint(a, b).is_err());
        }
        assert!(TextDraft::from_rtml("<annotation id=\"old-control\">A</annotation>").is_err());
    }
    #[test]
    fn annotated_model_refuses_unknown_nested_empty_or_duplicate_control_markup() {
        for html in [
            "<annotation id=\"a\"><annotation id=\"b\">X</annotation></annotation>",
            "<annotation id=\"a\">X</annotation><annotation id=\"a\">Y</annotation>",
            "<annotation id=\"a\"></annotation>",
            "<annotation id=\"a\" future=\"yes\">X</annotation>",
            "<unknown>X</unknown>",
            "<!-- comment -->X",
        ] {
            assert!(from_rtml(html).is_err(), "{html}");
        }
    }
}
