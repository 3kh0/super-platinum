//! Retained TextContent.exportToHtml(EXTERNAL_PASTE), followed by the actual
//! IRZY block map and TbRQ Slack Markdown passes. Presentation only: this module
//! never admits an anchor, changes body bytes, or grants a body-edit permit.
use super::*;

pub(super) fn nodes(
    section: &Section,
    draft: &super::super::super::text::TextDraft,
) -> Result<Vec<Node>, Error> {
    // Nested layout clipboard delegates and non-Text section types need their
    // own exact exporter; a rendered paragraph is not evidence for this branch.
    if section.section_type()?.unwrap_or(0) != 0
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || section.raw().boolean(36)? == Some(true)
    {
        return Err(Error::Unsupported);
    }
    draft.validate()?;
    let tag = match section.style()?.unwrap_or(0) {
        0 => None,
        1 => Some("h1"),
        2 => Some("h2"),
        3 => Some("h3"),
        39 => Some("h4"),
        40 => Some("h5"),
        41 => Some("h6"),
        4 => Some("pre"),
        16 | 17 => Some("blockquote"),
        47 | 48 => Some("p"),
        _ => return Err(Error::Unsupported),
    };
    // Actual top-level hasDefaultContentDirectly reads Attributes.default_content
    // (tag 13) and compares it to NONE=0. TextContent obtains its RTML first,
    // then returns an empty export for EXTERNAL_PASTE. The canonical body stays
    // untouched; placeholder text must not leak into the comment snippet.
    if section
        .raw()
        .message(16)?
        .map(|attributes| attributes.uint(13))
        .transpose()?
        .flatten()
        .is_some_and(|value| value != 0)
    {
        return Ok(Vec::new());
    }
    let mut value = super::section_nodes(section, draft)?;
    if let Some(tag) = tag {
        // The ordinary paragraph builder adds exactly one export-only BR.
        // Wrapped TextContent export does not add it; a real final BR stays.
        if !matches!(value.pop(), Some(Node::Element("br", _, _))) {
            return Err(Error::Unsupported);
        }
        Ok(vec![Node::Element(tag, value, None)])
    } else {
        Ok(value)
    }
}

pub(super) fn markdown_blocks(nodes: &mut [Node]) {
    // The admitted TextDraft cannot contain block descendants; these are only
    // the one actual TextContent export wrapper. Existing inline paths therefore
    // retain their original pass sequence and byte output.
    for tag in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        apply(nodes, tag, "*");
    }
    for node in nodes {
        if let Node::Element(tag, children, _) = node {
            match *tag {
                "blockquote" => children.insert(0, Node::Text(">".into())),
                "pre" => {
                    children.insert(0, Node::Text("```\n".into()));
                    children.push(Node::Text("\n```".into()));
                }
                _ => {}
            }
            match *tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "blockquote" => {
                    children.push(Node::Text("\n\n".into()));
                }
                "p" => children.push(Node::Text("\n".into())),
                _ => {}
            }
        }
    }
}

pub(super) fn rich_block(node: &Node) -> Result<Option<Value>, Error> {
    let Node::Element(tag, children, _) = node else {
        return Ok(None);
    };
    let (kind, level) = match *tag {
        "h1" => ("rich_text_header", Some(1)),
        "h2" => ("rich_text_header", Some(2)),
        "h3" => ("rich_text_header", Some(3)),
        "p" => ("rich_text_section", None),
        "pre" => ("rich_text_preformatted", None),
        "blockquote" => ("rich_text_quote", None),
        // Retained P/F omit H4..H6: normalizeSectionsDom wraps them in an
        // ordinary section. Do not silently remap them to header level 3.
        _ => return Ok(None),
    };
    let mut elements = Vec::new();
    for child in children {
        rich_node(child, &serde_json::Map::new(), &mut elements)?;
    }
    let mut block = json!({"type":kind,"elements":elements});
    if let Some(level) = level {
        block["level"] = json!(level);
    }
    Ok(Some(block))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::edit::text::TextDraft;

    #[test]
    fn retained_twelve_text_style_exports_preserve_block_types_and_exact_markdown() {
        let snapshot = super::super::super::tests::fixture();
        let original = snapshot.section("item").unwrap();
        let draft = TextDraft::from_rtml("<b>x</b><br>y").unwrap();
        for (style, kind, level, markdown) in [
            (0, "rich_text_section", None, "*x*\ny\n"),
            (1, "rich_text_header", Some(1), "**x*\ny*\n\n"),
            (2, "rich_text_header", Some(2), "**x*\ny*\n\n"),
            (3, "rich_text_header", Some(3), "**x*\ny*\n\n"),
            (39, "rich_text_section", None, "**x*\ny*\n\n"),
            (40, "rich_text_section", None, "**x*\ny*\n\n"),
            (41, "rich_text_section", None, "**x*\ny*\n\n"),
            (4, "rich_text_preformatted", None, "```\n*x*\ny\n```\n\n"),
            (16, "rich_text_quote", None, ">*x*\ny\n\n"),
            (17, "rich_text_quote", None, ">*x*\ny\n\n"),
            (47, "rich_text_section", None, "*x*\ny\n"),
            (48, "rich_text_section", None, "*x*\ny\n"),
        ] {
            let section = Section::from_raw(
                original
                    .raw()
                    .replace_fields(10, &[F::uint(10, style).unwrap()])
                    .unwrap(),
            )
            .unwrap();
            let nodes = nodes(&section, &draft).unwrap();
            assert_eq!(markdown_nodes(nodes.clone()).unwrap(), markdown);
            let rich = rich_nodes(&nodes).unwrap();
            assert_eq!(rich["elements"][0]["type"], kind);
            assert_eq!(
                rich["elements"][0].get("level").and_then(Value::as_u64),
                level
            );
            assert_eq!(
                rich["elements"].as_array().unwrap().len(),
                if style == 0 { 2 } else { 1 }
            );
            assert_eq!(
                rich["elements"][0]["elements"][0],
                json!({"type":"text","text":"x","style":{"bold":true}})
            );
        }
    }

    #[test]
    fn styled_snippets_do_not_admit_other_body_or_layout_exporters() {
        let snapshot = super::super::super::tests::fixture();
        let original = snapshot.section("item").unwrap();
        let draft = TextDraft::from_rtml("x").unwrap();
        for (tag, value) in [(9, 1), (10, 7), (36, 1)] {
            let section = Section::from_raw(
                original
                    .raw()
                    .replace_fields(tag, &[F::uint(tag, value).unwrap()])
                    .unwrap(),
            )
            .unwrap();
            assert!(nodes(&section, &draft).is_err());
        }
    }

    #[test]
    fn source_default_content_attribute_suppresses_only_exported_snippet() {
        let snapshot = super::super::super::tests::fixture();
        let original = snapshot.section("item").unwrap();
        let draft = TextDraft::from_rtml("placeholder").unwrap();
        for value in [1, 2] {
            let attributes = RawMessage::from_fields(&[F::uint(13, value).unwrap()]).unwrap();
            let section = Section::from_raw(
                original
                    .raw()
                    .replace_fields(16, &[F::message(16, &attributes).unwrap()])
                    .unwrap(),
            )
            .unwrap();
            let before = section.raw().raw().to_vec();
            let exported = nodes(&section, &draft).unwrap();
            assert!(exported.is_empty());
            assert_eq!(markdown_nodes(exported.clone()).unwrap(), "");
            assert_eq!(
                rich_nodes(&exported).unwrap(),
                json!({"type":"rich_text","elements":[]})
            );
            assert_eq!(section.raw().raw(), before);
        }
        assert_eq!(
            markdown_nodes(nodes(original, &draft).unwrap()).unwrap(),
            "placeholder\n"
        );
    }
}
