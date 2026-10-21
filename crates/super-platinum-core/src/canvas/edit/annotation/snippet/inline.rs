//! Retained getControlSnippet: export the actual annotation subtree with
//! fromLocalDisplay=true, forExternalPaste=false, preserveFormatting=false.
//! Formatting ancestors outside that subtree are not copied into the snippet.
use super::*;
use scraper::{ElementRef, Html, Selector};

pub(in crate::canvas::edit::annotation) fn capture(
    section: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
    parent_rtml: &str,
) -> Result<Value, Error> {
    if parent_rtml.len() > 128 * 1024 || !position::modern_id(control) {
        return Err(Error::Unsupported);
    }
    let parsed = Html::parse_fragment(parent_rtml);
    if !parsed.errors.is_empty() {
        return Err(Error::Unsupported);
    }
    let selector = Selector::parse("annotation").map_err(|_| Error::Unsupported)?;
    let all: Vec<_> = parsed
        .select(&selector)
        .filter(|node| node.value().attr("id") == Some(control))
        .collect();
    let [annotation] = all.as_slice() else {
        return Err(Error::Unsupported);
    };
    if annotation.value().attrs().count() != 1 || annotation.value().attr("id") != Some(control) {
        return Err(Error::Unsupported);
    }
    let mut nodes = children(*annotation, 0, false)?;
    // D.output drops one final top-level BR for local-display export. A BR
    // inside a formatting node ends in its closing tag and is retained.
    if matches!(nodes.last(), Some(Node::Element("br", _, _))) {
        nodes.pop();
    }
    capture_nodes(section, control, quip_user, slack_user, nodes)
}

pub(super) fn body_nodes(section: &Section) -> Result<Vec<Node>, Error> {
    // Validate original controls, exact text shape and the bounded heading host
    // before removing annotation wrappers for external-paste presentation.
    super::super::inline::original(section)?;
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    let text = content.message(1)?.ok_or(Error::Unsupported)?;
    let parsed = Html::parse_fragment(text.string(1)?.unwrap_or(""));
    if !parsed.errors.is_empty() {
        return Err(Error::Unsupported);
    }
    let mut result = children(parsed.root_element(), 0, true)?;
    result.push(Node::Element("br", vec![], None));
    Ok(result)
}

fn children(
    parent: ElementRef<'_>,
    depth: usize,
    strip_annotations: bool,
) -> Result<Vec<Node>, Error> {
    if depth > 32 {
        return Err(Error::Unsupported);
    }
    let mut out = Vec::new();
    for child in parent.children() {
        match child.value() {
            scraper::Node::Text(value) => {
                // Exact qa+r normalizeSpaces used by the retained A.text.
                // This is snippet presentation only; original canonical text
                // and all original Automerge changes remain retained unchanged.
                let value = if strip_annotations {
                    // External body projection retains the existing draft text
                    // semantics; local-display normalization is separate.
                    value
                        .chars()
                        .filter_map(|c| match c {
                            '\t' | '\n' | '\r' => Some(' '),
                            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => None,
                            _ => Some(c),
                        })
                        .collect()
                } else {
                    value
                        .replace('\u{a0}', " ")
                        .replace(['\u{200b}', '\u{feff}'], "")
                };
                if !value.is_empty() {
                    out.push(Node::Text(value));
                }
            }
            scraper::Node::Element(element) => {
                if strip_annotations && element.name() == "annotation" {
                    // The complete original model above checked ID uniqueness,
                    // attributes and nonnested/nonempty control ranges.
                    out.extend(children(
                        ElementRef::wrap(child).ok_or(Error::Unsupported)?,
                        depth + 1,
                        true,
                    )?);
                    continue;
                }
                let name = match element.name() {
                    "b" => "b",
                    "i" => "i",
                    "u" => "u",
                    "del" => "del",
                    "code" => "code",
                    "a" => "a",
                    "br" => "br",
                    _ => return Err(Error::Unsupported),
                };
                let href = if name == "a" {
                    if element.attrs().any(|(key, _)| key != "href") {
                        return Err(Error::Unsupported);
                    }
                    Some(element.attr("href").ok_or(Error::Unsupported)?.to_owned())
                } else {
                    if element.attrs().next().is_some() {
                        return Err(Error::Unsupported);
                    }
                    None
                };
                let children = children(
                    ElementRef::wrap(child).ok_or(Error::Unsupported)?,
                    depth + 1,
                    strip_annotations,
                )?;
                if name == "br" && !children.is_empty() {
                    return Err(Error::Unsupported);
                }
                out.push(Node::Element(name, children, href));
            }
            _ => return Err(Error::Unsupported),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "temp:C:ABC0123456789abcdef012345678";
    fn snippet(html: &str) -> Value {
        let snapshot = super::super::super::tests::fixture();
        capture(snapshot.section("item").unwrap(), ID, "Q1", "U1", html).unwrap()
    }
    #[test]
    fn inline_export_does_not_copy_an_ancestor_style_or_paragraph_break() {
        let v = snippet(&format!("<b>A<annotation id=\"{ID}\">B</annotation>C</b>"));
        assert_eq!(v["mrkdwn"], "B");
        let rich: Value = serde_json::from_str(v["richText"].as_str().unwrap()).unwrap();
        assert_eq!(
            rich["blocks"][0]["elements"][0]["elements"],
            json!([{"type":"text","text":"B"}])
        );
        let v = snippet(&format!("<annotation id=\"{ID}\"><b>AB</b></annotation>"));
        assert_eq!(v["mrkdwn"], "*AB*");
    }
    #[test]
    fn inline_export_uses_actual_nested_styles_and_local_display_break_rule() {
        assert_eq!(
            snippet(&format!("<annotation id=\"{ID}\">A<br></annotation>"))["mrkdwn"],
            "A"
        );
        assert_eq!(
            snippet(&format!(
                "<annotation id=\"{ID}\"><b>A<br></b></annotation>"
            ))["mrkdwn"],
            "*A\n*"
        );
        assert_eq!(
            snippet(&format!(
                "<annotation id=\"{ID}\">a\u{a0}b\u{200b}\u{feff}</annotation>"
            ))["mrkdwn"],
            "a b"
        );
    }
    #[test]
    fn inline_export_refuses_wrong_or_multiple_control_identity() {
        let s = super::super::super::tests::fixture();
        for html in [
            format!("<annotation id=\"wrong\">A</annotation>"),
            format!("<annotation id=\"{ID}\">A</annotation><annotation id=\"{ID}\">B</annotation>"),
            format!("<annotation id=\"{ID}\"><control id=\"unknown\"></control></annotation>"),
        ] {
            assert!(capture(s.section("item").unwrap(), ID, "Q1", "U1", &html).is_err());
        }
    }
}
